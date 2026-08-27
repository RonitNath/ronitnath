//! `/links/<token>` — the claim page, and the only bearer surface there is.
//!
//! A link token is a `recipient/embed` principal (`docs/rebuild/plan.md`
//! §Product contract): it opens exactly this page and nothing else. That
//! restriction is structural rather than remembered. [`Bearer`] is implemented
//! for [`LinkState`] and for no other router state, so a bearer route under
//! `/api` does not compile:
//!
//! ```compile_fail
//! # use axum::{Router, routing::get, response::Response};
//! # use rn_site::links::Bearer;
//! # use rn_site::AppState;
//! async fn leak(_: Bearer) -> Response { unimplemented!() }
//! // `Bearer: FromRequestParts<AppState>` is not implemented.
//! let _: Router<AppState> = Router::new().route("/api/q/anything", get(leak));
//! ```
//!
//! The reverse holds by the same construction the other way round: the cookie
//! extractors *are* available here, because the claim page has to know whether
//! the reader is somebody yet — a link tells you what it grants, and only a
//! person can hold a grant.

use askama::Template;
use askama_web::WebTemplate;
use axum::Router;
use axum::extract::{FromRef, FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use rn_api::commands::{ClaimLink, VerifyEmail};
use rn_kernel::domain::{LinkRow, Token};
use rn_kernel::store::Reads;
use rn_kernel::{Principal, Timestamp, bind, invite};

use crate::api::cmd::{self, CommandError};
use crate::api::decline;
use crate::api::origin::SameOrigin;
use crate::auth::session::Visitor;
use crate::presence::theme_for;
use crate::state::AppState;

/// The links router's state. A newtype, because the type *is* the boundary:
/// see the module docs.
#[derive(Clone)]
pub struct LinkState(pub AppState);

impl FromRef<LinkState> for AppState {
    fn from_ref(state: &LinkState) -> Self {
        state.0.clone()
    }
}

/// The lookup a token resolves through. `token_hash` is UNIQUE, so this is a
/// seek and a forged token costs one index probe.
const BY_TOKEN: &str = "SELECT id, expires_at, claimed_by_identity_id, verifies_factor_id \
                        FROM link WHERE token_hash = $1";

/// The holder of a link token.
#[derive(Debug, Clone)]
pub struct Bearer {
    /// The principal a `check()` would run against.
    pub principal: Principal,
    /// The link row itself, so the page can say what it grants.
    pub link: LinkRow,
    /// The token as it arrived, for the command that consumes it.
    pub token: String,
}

impl FromRequestParts<LinkState> for Bearer {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &LinkState,
    ) -> Result<Self, Self::Rejection> {
        let Path(raw) = Path::<String>::from_request_parts(parts, state)
            .await
            .map_err(|_| decline::not_found())?;
        let token = Token::from_wire(&raw);
        let link = state
            .0
            .store
            .reads()
            .query_opt::<LinkRow>(BY_TOKEN, bind![token.digest().to_vec()])
            .await
            .map_err(|error| {
                tracing::warn!(%error, "a link could not be looked up");
                decline::not_found()
            })?;

        // Unknown, expired and already claimed are one answer: a token that
        // does not work now. Telling them apart would say whether a link ever
        // existed, which is what a token guesser is measuring.
        let now = state.0.store.clock().now();
        let link = link
            .filter(|row| row.is_claimable(now))
            .ok_or_else(decline::not_found)?;

        Ok(Self {
            principal: Principal::Bearer { link: link.id },
            link,
            token: raw,
        })
    }
}

pub fn router() -> Router<LinkState> {
    Router::new()
        .route("/links/{token}", get(page))
        .route("/links/{token}/claim", post(claim))
}

/// What a link grants, in the vocabulary this cut has.
///
/// Two purposes exist, and they are reached differently. Proving an address is
/// a column on the link (`link.verifies_factor_id`), because there is no
/// relation that expresses "this token verifies that factor". An invitation is
/// a relation row — `group:X #member @link:T` — so naming one means reading
/// the relation store, which is why this is a query and not a `match` on the
/// row the extractor already has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Grant {
    /// Proving an email address belongs to the identity that claims it.
    Email,
    /// Joining a container at a role, described well enough to be accepted.
    Invitation(invite::Description),
    /// A link that carries neither: no factor to verify and no relation row.
    /// Nothing mints one, and a page that made something up about it would be
    /// worse than a page that says it cannot.
    Unknown,
}

impl Grant {
    /// One extra indexed read, on a page that is one link and one button.
    async fn of(reads: &impl Reads, link: &LinkRow) -> Self {
        if link.verifies_factor_id.is_some() {
            return Self::Email;
        }
        match invite::describe(reads, link.id).await {
            Ok(Some(description)) => Self::Invitation(description),
            Ok(None) => Self::Unknown,
            Err(error) => {
                // The page still opens; it just cannot say what it opens on.
                tracing::warn!(%error, "an invitation could not be described");
                Self::Unknown
            }
        }
    }

    /// The heading: what accepting this does.
    fn headline(&self) -> String {
        match self {
            Self::Email => "Confirm an email address".to_owned(),
            Self::Invitation(what) => format!("Join {}", what.container),
            Self::Unknown => "An invitation".to_owned(),
        }
    }

    /// The facts under it. Empty for a verification link, which grants nothing
    /// to describe — its wording is the whole of what it says.
    fn facts(&self, now: Timestamp) -> Vec<Fact> {
        let Self::Invitation(what) = self else {
            return Vec::new();
        };
        let container = match what.container_kind.as_str() {
            "organization" => "Organization",
            "group" => "Group",
            // The statement only joins those two kinds, so this is
            // unreachable; naming it beats an empty label if it ever is not.
            _ => "Container",
        };
        let mut facts = vec![
            Fact::new(container, what.container.clone()),
            Fact::new("Role", what.role.clone()),
        ];
        if let Some(minted_by) = &what.minted_by {
            facts.push(Fact::new("From", minted_by.clone()));
        }
        facts.push(Fact::new("Expires", within(what.expires_at - now)));
        facts
    }
}

/// One label and one value, under the heading.
pub struct Fact {
    /// What it is.
    pub label: &'static str,
    /// What it says.
    pub value: String,
}

impl Fact {
    fn new(label: &'static str, value: String) -> Self {
        Self { label, value }
    }
}

/// How long is left, in the largest unit that is still a whole number of them.
///
/// A duration and not a date: the page is rendered by the server and read in a
/// timezone the server does not know, and "in 29 days" is true everywhere
/// while a printed timestamp is true in one place.
fn within(seconds: Timestamp) -> String {
    const MINUTE: Timestamp = 60;
    const HOUR: Timestamp = 60 * MINUTE;
    const DAY: Timestamp = 24 * HOUR;
    let plural = |count: Timestamp, unit: &str| {
        format!("in {count} {unit}{}", if count == 1 { "" } else { "s" })
    };
    match seconds {
        i64::MIN..=0 => "now".to_owned(),
        1..HOUR => plural((seconds / MINUTE).max(1), "minute"),
        HOUR..DAY => plural(seconds / HOUR, "hour"),
        _ => plural(seconds / DAY, "day"),
    }
}

#[derive(Template, WebTemplate)]
#[template(path = "claim.html")]
struct ClaimPage {
    theme: &'static str,
    version: String,
    /// The token, so the form can post it back.
    token: String,
    /// What the link grants, in words.
    grant: String,
    /// The container, the role, who minted it and when it runs out — empty for
    /// a verification link, which has none of them.
    facts: Vec<Fact>,
    /// Whether the reader is somebody yet.
    signed_in: bool,
    /// Where a sign-in should return to.
    next: String,
    /// What went wrong last time, if anything.
    error: String,
}

impl ClaimPage {
    fn new(
        state: &AppState,
        headers: &HeaderMap,
        bearer: &Bearer,
        visitor: &Visitor,
        grant: &Grant,
    ) -> Self {
        Self {
            theme: theme_for(headers).as_str(),
            version: state.version.to_string(),
            token: bearer.token.clone(),
            grant: grant.headline(),
            facts: grant.facts(state.store.clock().now()),
            signed_in: matches!(visitor.principal, Principal::Member { .. }),
            next: format!("/links/{}", bearer.token),
            error: String::new(),
        }
    }
}

async fn page(
    State(state): State<LinkState>,
    bearer: Bearer,
    visitor: Visitor,
    headers: HeaderMap,
) -> Response {
    let grant = Grant::of(&state.0.store.reads(), &bearer.link).await;
    ClaimPage::new(&state.0, &headers, &bearer, &visitor, &grant).into_response()
}

/// Claim the link, as whoever the cookie says is holding it.
///
/// The bearer opens the page; the *session* is who the grant lands on. Both
/// are required, which is why an anonymous claim is refused here rather than
/// silently creating somebody.
async fn claim(
    State(state): State<LinkState>,
    _: SameOrigin,
    bearer: Bearer,
    visitor: Visitor,
) -> Response {
    let Principal::Member { .. } = visitor.principal else {
        // Sign in first; the page comes back with the same link.
        return (
            StatusCode::SEE_OTHER,
            [(
                header::LOCATION,
                format!("/auth?next=/links/{}", bearer.token),
            )],
        )
            .into_response();
    };

    let outcome = match Grant::of(&state.0.store.reads(), &bearer.link).await {
        // The one grant this cut mints. `VerifyEmail` consumes the token and
        // marks the link claimed in the same transaction.
        Grant::Email => {
            cmd::invoke(
                &state.0,
                visitor.principal,
                &VerifyEmail {
                    token: bearer.token.clone(),
                },
            )
            .await
        }
        // An invitation. The bearer opened the page; `ClaimLink` reads the
        // token again itself, because what the token grants and who the grant
        // lands on are two separate facts and the command needs both. A link
        // this build could not describe goes the same way and is declined
        // there: the command is the authority on what a token is worth, not
        // the page.
        Grant::Invitation(_) | Grant::Unknown => {
            cmd::invoke(
                &state.0,
                visitor.principal,
                &ClaimLink {
                    token: bearer.token.clone(),
                },
            )
            .await
        }
    };

    match outcome {
        Ok(_) => (StatusCode::SEE_OTHER, [(header::LOCATION, "/app")]).into_response(),
        Err(CommandError::Unbound) => decline::not_found(),
        Err(error) => error.response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rn_kernel::ids::Id;

    fn link(verifies: Option<i64>) -> LinkRow {
        LinkRow {
            id: Id::new(1),
            expires_at: 2_000_000_000,
            claimed_by_identity_id: None,
            verifies_factor_id: verifies.map(Id::new),
        }
    }

    fn invitation() -> Grant {
        Grant::Invitation(invite::Description {
            container: "Isoastra".to_owned(),
            container_kind: "organization".to_owned(),
            role: "admin".to_owned(),
            minted_by: Some("Ronit Nath".to_owned()),
            expires_at: 1_000 + 3 * 24 * 60 * 60,
        })
    }

    #[test]
    fn a_verification_link_says_what_it_proves_and_nothing_else() {
        assert_eq!(Grant::Email.headline(), "Confirm an email address");
        // The wording R1 found correct stays exactly as it was, and a
        // verification link has no container, role or minter to list.
        assert!(Grant::Email.facts(1_000).is_empty());
    }

    #[test]
    fn an_invitation_names_the_container_the_role_the_minter_and_the_expiry() {
        let grant = invitation();
        assert_eq!(grant.headline(), "Join Isoastra");
        let said: Vec<(&str, String)> = grant
            .facts(1_000)
            .into_iter()
            .map(|fact| (fact.label, fact.value))
            .collect();
        assert_eq!(
            said,
            vec![
                ("Organization", "Isoastra".to_owned()),
                ("Role", "admin".to_owned()),
                ("From", "Ronit Nath".to_owned()),
                ("Expires", "in 3 days".to_owned()),
            ]
        );
    }

    #[test]
    fn a_grant_with_no_minter_still_names_what_it_grants() {
        let Grant::Invitation(mut what) = invitation() else {
            unreachable!()
        };
        what.minted_by = None;
        what.container_kind = "group".to_owned();
        let grant = Grant::Invitation(what);
        let labels: Vec<&str> = grant.facts(1_000).iter().map(|fact| fact.label).collect();
        assert_eq!(labels, vec!["Group", "Role", "Expires"]);
    }

    #[test]
    fn a_grant_this_build_cannot_name_is_not_described_as_one_it_can() {
        assert_eq!(Grant::Unknown.headline(), "An invitation");
        assert!(Grant::Unknown.facts(1_000).is_empty());
        assert_ne!(Grant::Unknown.headline(), Grant::Email.headline());
    }

    #[test]
    fn how_long_is_left_is_said_in_whole_units_of_the_largest_that_fits() {
        assert_eq!(within(0), "now");
        assert_eq!(within(-5), "now");
        assert_eq!(within(30), "in 1 minute");
        assert_eq!(within(60), "in 1 minute");
        assert_eq!(within(59 * 60), "in 59 minutes");
        assert_eq!(within(60 * 60), "in 1 hour");
        assert_eq!(within(23 * 60 * 60), "in 23 hours");
        assert_eq!(within(24 * 60 * 60), "in 1 day");
        assert_eq!(within(30 * 24 * 60 * 60), "in 30 days");
    }

    #[test]
    fn an_expired_or_claimed_link_is_not_claimable() {
        let mut expired = link(Some(1));
        expired.expires_at = 10;
        assert!(!expired.is_claimable(100));

        let mut claimed = link(Some(1));
        claimed.claimed_by_identity_id = Some(Id::new(3));
        assert!(!claimed.is_claimable(100));

        assert!(link(Some(1)).is_claimable(100));
    }
}
