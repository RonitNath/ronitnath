//! `GET /api/whoami` — the only chrome DTO, and the tier computation behind it.
//!
//! What is *not* here is the point. No email address: an identity that has
//! resolved to a person is named by that person, and one that has not is named
//! by its address *masked* — `r…t@` — which is enough for somebody to
//! recognise their own registration and not enough for anybody to read it. No
//! internal id: every identifier is derived on the way out. No relation the
//! chrome does not draw. `whoami_leaks_nothing` reads the rendered JSON back
//! and asserts both, so widening this type without a screen that renders the
//! new field fails a test rather than a review.
//!
//! The source is never the label. `rn-site` was the display of every identity
//! this deployment has ever minted, which named the deployment rather than the
//! reader — a rail that said "rn-site" to everybody told nobody who they were.
//!
//! Tiers are computed from relations, never from a column
//! (`docs/kernel/index.html` — platform administration is
//! `platform:* #operator @person`, which is why there is no `is_admin`
//! anywhere in this tree):
//!
//! * `member` — every session has it.
//! * `org` — at least one organization membership at `admin` or above.
//! * `platform` — the operator relation row.

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::get};
use rn_api::whoami::{IdentityRef, MemberRole, OrganizationRef, PartyRef, PersonRef, Tier, Whoami};
use rn_kernel::bind;
use rn_kernel::cmd::is_platform_operator;
use rn_kernel::domain::PartyRow;
use rn_kernel::ids::{Id, IdKey, Organization, Person};
use rn_kernel::principal::identity_of;
use rn_kernel::store::{Count, Cursor, FromRow, Reads, RowError};
use rn_kernel::{Outcome, Principal};

use super::decline;
use super::party::public_of;
use crate::auth::session::Session;
use crate::state::AppState;

pub(crate) mod name;

use name::{address_of, masked};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/whoami", get(whoami))
}

async fn whoami(State(state): State<AppState>, session: Session) -> Response {
    match build(&state, &session.principal, session.expires_at).await {
        Ok(Some(body)) => Json(body).into_response(),
        // A live session whose rows have gone is not a server error: the
        // identity was disabled or absorbed between resolving and reading.
        Ok(None) => decline::forbidden(),
        Err(error) => decline::from_kernel(&error),
    }
}

/// One organization the principal belongs to, as the chrome lists it.
struct Belongs {
    party: i64,
    role: String,
    display: String,
}

impl FromRow for Belongs {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            party: row.int("group_id")?,
            role: row.text("role")?,
            display: row.text("display_name")?,
        })
    }
}

/// The organizations of a person, with the role the row records.
const ORGANIZATIONS: &str = "SELECT m.group_id, m.role, p.display_name \
                             FROM membership m JOIN party p ON p.id = m.group_id \
                             WHERE m.party_id = $1 AND p.kind = 'organization' \
                             ORDER BY p.display_name";

const PARTY: &str = "SELECT id, kind, display_name, status, created_at FROM party WHERE id = $1";

/// Whether a person operates any organization at all — the `/org` tier, asked
/// without gathering the list the chrome would render.
const OPERATES: &str = "SELECT count(*) AS n FROM membership m \
                        JOIN party p ON p.id = m.group_id \
                        WHERE m.party_id = $1 AND p.kind = 'organization' \
                        AND m.role IN ('admin', 'owner')";

/// The tiers a principal holds, for the shell router.
///
/// The same computation `whoami` reports, in two counting queries rather than
/// the full DTO: serving `/app` should not cost the organization list that
/// only the chrome draws.
pub async fn tiers_of(state: &AppState, principal: &Principal) -> Outcome<Vec<Tier>> {
    let mut tiers = vec![Tier::Member];
    let Principal::Member { person, .. } = principal else {
        return Ok(Vec::new());
    };
    let Some(person) = person else {
        return Ok(tiers);
    };
    let reads = state.store.reads();
    if counted(&reads, OPERATES, *person).await? {
        tiers.push(Tier::Org);
    }
    if is_platform_operator(&reads, *person).await? {
        tiers.push(Tier::Platform);
    }
    Ok(tiers)
}

async fn counted(reads: &impl Reads, sql: &'static str, person: Id<Person>) -> Outcome<bool> {
    Ok(reads
        .query::<Count>(sql, bind![person])
        .await?
        .first()
        .is_some_and(|count| count.0 > 0))
}

/// Everything the chrome is allowed to know, or `None` if the rows are gone.
pub async fn build(
    state: &AppState,
    principal: &Principal,
    expires_at: i64,
) -> Outcome<Option<Whoami>> {
    let (Some(identity_id), Some(acting_as)) = (principal.identity(), principal.acting_as()) else {
        return Ok(None);
    };
    let reads = state.store.reads();
    let key = state.ids();

    let Some(identity) = identity_of(&reads, identity_id).await? else {
        return Ok(None);
    };
    let Some(acting) = party(&reads, acting_as).await? else {
        return Ok(None);
    };

    let person = match identity.person_id {
        Some(id) => party(&reads, id).await?.map(|row| PersonRef {
            public_id: id.public(key),
            display: row.display_name,
        }),
        None => None,
    };
    // Only an unresolved identity costs this read, and a registration resolves
    // in the same transaction that creates it — so in practice it is never
    // taken at all.
    let display = match &person {
        Some(person) => person.display.clone(),
        None => masked(address_of(&reads, identity_id).await?.as_deref()),
    };

    let organizations = match identity.person_id {
        Some(id) => memberships(&reads, id, key).await?,
        None => Vec::new(),
    };
    let platform = match identity.person_id {
        Some(id) => is_platform_operator(&reads, id).await?,
        None => false,
    };

    Ok(Some(Whoami {
        identity: IdentityRef {
            public_id: identity_id.public(key),
            display,
        },
        person,
        acting_as: PartyRef {
            kind: acting.kind.into(),
            public_id: public_of(acting.kind, acting_as.get(), key),
            display: acting.display_name,
        },
        handle: match identity.person_id {
            Some(id) => rn_kernel::oidc::handle::of_person(&reads, id).await?,
            None => None,
        },
        tiers: tiers(&organizations, platform),
        organizations,
        session_expires_at: expires_at,
    }))
}

/// The tiers a principal holds, in shell order.
fn tiers(organizations: &[OrganizationRef], platform: bool) -> Vec<Tier> {
    let mut tiers = vec![Tier::Member];
    if organizations
        .iter()
        .any(|org| matches!(org.role, MemberRole::Admin | MemberRole::Owner))
    {
        tiers.push(Tier::Org);
    }
    if platform {
        tiers.push(Tier::Platform);
    }
    tiers
}

async fn party(reads: &impl Reads, id: Id<Person>) -> Outcome<Option<PartyRow>> {
    Ok(reads.query_opt::<PartyRow>(PARTY, bind![id]).await?)
}

async fn memberships(
    reads: &impl Reads,
    person: Id<Person>,
    key: &IdKey,
) -> Outcome<Vec<OrganizationRef>> {
    Ok(reads
        .query::<Belongs>(ORGANIZATIONS, bind![person])
        .await?
        .into_iter()
        .filter_map(|row| {
            Some(OrganizationRef {
                public_id: Id::<Organization>::new(row.party).public(key),
                display: row.display,
                role: role(&row.role)?,
            })
        })
        .collect())
}

/// A membership role as the wire names it. An unknown value is dropped rather
/// than guessed at: a role this build does not understand is not a member
/// role it may safely round down to.
fn role(raw: &str) -> Option<MemberRole> {
    match raw {
        "member" => Some(MemberRole::Member),
        "admin" => Some(MemberRole::Admin),
        "owner" => Some(MemberRole::Owner),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn org(role: MemberRole) -> OrganizationRef {
        OrganizationRef {
            public_id: "o_AAAAAAAAAAAAAAAAAAAAAA".parse().expect("a shaped id"),
            display: "Isoastra".into(),
            role,
        }
    }

    #[test]
    fn a_session_is_always_a_member_and_nothing_more_by_default() {
        assert_eq!(tiers(&[], false), vec![Tier::Member]);
    }

    #[test]
    fn belonging_to_an_organization_is_not_operating_one() {
        assert_eq!(tiers(&[org(MemberRole::Member)], false), vec![Tier::Member]);
        assert_eq!(
            tiers(&[org(MemberRole::Admin)], false),
            vec![Tier::Member, Tier::Org]
        );
        assert_eq!(
            tiers(&[org(MemberRole::Owner)], false),
            vec![Tier::Member, Tier::Org]
        );
    }

    #[test]
    fn the_platform_tier_comes_from_the_relation_and_stands_alone() {
        assert_eq!(tiers(&[], true), vec![Tier::Member, Tier::Platform]);
        assert_eq!(
            tiers(&[org(MemberRole::Owner)], true),
            vec![Tier::Member, Tier::Org, Tier::Platform]
        );
    }

    #[test]
    fn a_role_this_build_does_not_know_is_dropped_rather_than_rounded_down() {
        assert_eq!(role("member"), Some(MemberRole::Member));
        assert_eq!(role("owner"), Some(MemberRole::Owner));
        assert_eq!(role("superuser"), None);
        assert_eq!(role(""), None);
    }

    #[test]
    fn the_party_vocabulary_is_the_kernels_and_not_a_copy() {
        use rn_api::whoami::PartyKind;
        assert_eq!(
            PartyKind::from(rn_kernel::domain::PartyKind::Group),
            PartyKind::Group
        );
    }
}
