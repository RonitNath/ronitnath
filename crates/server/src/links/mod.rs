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
use rn_kernel::Principal;
use rn_kernel::bind;
use rn_kernel::domain::{LinkRow, Token};
use rn_kernel::store::Reads;

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
/// One purpose exists today — proving an address — because
/// `link.verifies_factor_id` is the only grant column migration 1 carries. K2
/// replaces it with a relation row (`group:X #member @link:T`), at which point
/// this reads the relation and gains its other arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grant {
    /// Proving an email address belongs to the identity that claims it.
    Email,
    /// A grant this build cannot name.
    Unknown,
}

impl Grant {
    fn of(link: &LinkRow) -> Self {
        match link.verifies_factor_id {
            Some(_) => Self::Email,
            None => Self::Unknown,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Email => "Confirm an email address",
            Self::Unknown => "An invitation",
        }
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
    grant: &'static str,
    /// Whether the reader is somebody yet.
    signed_in: bool,
    /// Where a sign-in should return to.
    next: String,
    /// What went wrong last time, if anything.
    error: String,
}

impl ClaimPage {
    fn new(state: &AppState, headers: &HeaderMap, bearer: &Bearer, visitor: &Visitor) -> Self {
        Self {
            theme: theme_for(headers).as_str(),
            version: state.version.to_string(),
            token: bearer.token.clone(),
            grant: Grant::of(&bearer.link).label(),
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
    ClaimPage::new(&state.0, &headers, &bearer, &visitor).into_response()
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

    let outcome = match Grant::of(&bearer.link) {
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
        // K2 owns `ClaimLink`. Until it lands the dispatch table has no
        // binding for the name and the answer is the uniform decline — not a
        // stub that pretends to have granted something.
        Grant::Unknown => {
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

    #[test]
    fn a_verification_link_says_what_it_proves() {
        assert_eq!(Grant::of(&link(Some(9))), Grant::Email);
        assert_eq!(
            Grant::of(&link(Some(9))).label(),
            "Confirm an email address"
        );
    }

    #[test]
    fn a_grant_this_build_cannot_name_is_not_described_as_one_it_can() {
        assert_eq!(Grant::of(&link(None)), Grant::Unknown);
        assert_ne!(Grant::Unknown.label(), Grant::Email.label());
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
