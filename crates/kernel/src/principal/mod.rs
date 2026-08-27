//! Who is asking, and what that expands to.
//!
//! A [`Principal`] is the one input to every authorisation decision. There are
//! three of them and no fourth: a member holding a session cookie, a bearer
//! holding a link token, and anonymous. The server never invents a principal
//! of its own — a dev bypass is a fourth kind by another name, and the
//! pre-rebuild code had one.
//!
//! [`expand`] turns a principal into the [`SubjectSet`] a relation query runs
//! against: the person, the organizations and groups it belongs to,
//! `authenticated`, and `public`. That expansion is what makes `check()` one
//! indexed query instead of a walk, and it is done once per request and cached
//! per session digest.

mod cache;
#[cfg(test)]
mod tests;

pub use cache::{Cache, Digest};

use crate::bind;
use crate::domain::{IdentityRow, SessionRow, Token};
use crate::error::Outcome;
use crate::ids::{Group, Id, Identity, Link, Organization, Person, Session};
use crate::store::{Cursor, FromRow, Reads, RowError};

/// Who is asking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    /// A signed-in registration, speaking as some party.
    Member {
        /// The registration that authenticated.
        identity: Id<Identity>,
        /// The human it has been resolved to, if it has been.
        person: Option<Id<Person>>,
        /// The party it is speaking as. Its own person today; an organization
        /// once K2 lands the command that switches it.
        acting_as: Id<Person>,
        /// The session row, so `SignOut` knows what to delete.
        session: Id<Session>,
        /// The operator behind this session, when it is an impersonation.
        ///
        /// `None` for every ordinary session. It is not authority: `person`
        /// above is the *target*, so this principal expands to the target's
        /// real subject set and `check()` learns no new case. What it changes
        /// is who the audit row names as actor ([`crate::cmd::refs::actor`])
        /// and which commands are refused outright
        /// ([`crate::authority::FORBIDDEN_WHILE_IMPERSONATING`]).
        impersonated_by: Option<Id<Identity>>,
    },
    /// The holder of a link token. Reaches the claim page and nothing else.
    Bearer {
        /// The link the token opened.
        link: Id<Link>,
    },
    /// Nobody.
    Anonymous,
}

impl Principal {
    /// The signed-in identity, if there is one.
    pub const fn identity(&self) -> Option<Id<Identity>> {
        match self {
            Self::Member { identity, .. } => Some(*identity),
            _ => None,
        }
    }

    /// The session, if there is one.
    pub const fn session(&self) -> Option<Id<Session>> {
        match self {
            Self::Member { session, .. } => Some(*session),
            _ => None,
        }
    }

    /// The party being spoken as, if there is one.
    pub const fn acting_as(&self) -> Option<Id<Person>> {
        match self {
            Self::Member { acting_as, .. } => Some(*acting_as),
            _ => None,
        }
    }

    /// The operator wearing this session, if somebody is.
    pub const fn impersonated_by(&self) -> Option<Id<Identity>> {
        match self {
            Self::Member {
                impersonated_by, ..
            } => *impersonated_by,
            _ => None,
        }
    }
}

/// What a principal expands to, for one indexed relation query.
///
/// `organizations` and `groups` are read from `membership`, which migration 1
/// creates; the commands that *write* those rows arrive with K2, so today the
/// lists are usually empty and the shape is already the one `check()` needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubjectSet {
    /// The registration itself — some rows are granted to an identity rather
    /// than to the human behind it.
    pub identity: Option<Id<Identity>>,
    /// The human.
    pub person: Option<Id<Person>>,
    /// Organizations the person belongs to.
    pub organizations: Vec<Id<Organization>>,
    /// Groups the person belongs to.
    pub groups: Vec<Id<Group>>,
    /// The link a bearer is holding.
    pub link: Option<Id<Link>>,
    /// Whether the principal is signed in at all.
    pub authenticated: bool,
}

impl SubjectSet {
    /// How many subjects a relation query would have to test. `public` is
    /// always one of them and is not stored.
    pub fn len(&self) -> usize {
        1 + usize::from(self.authenticated)
            + usize::from(self.identity.is_some())
            + usize::from(self.person.is_some())
            + usize::from(self.link.is_some())
            + self.organizations.len()
            + self.groups.len()
    }

    /// Never — `public` is always present.
    pub const fn is_empty(&self) -> bool {
        false
    }
}

/// The rows a cookie resolves to, cached together because they are read
/// together.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    /// The principal itself.
    pub principal: Principal,
    /// When the session stops working — `whoami` reports it.
    pub expires_at: crate::Timestamp,
    /// Whether the session is close enough to expiry to be worth renewing.
    /// Acted on by the observation lane, never by the read that noticed.
    pub wants_renewal: bool,
}

/// A cache of resolved cookies, keyed by session digest.
pub type PrincipalCache = Cache<Resolved>;

/// The cookie-to-principal query, exposed so the "no full scan" gate can put
/// it under `EXPLAIN QUERY PLAN` and assert the index it must use.
pub const RESOLVE_SQL: &str = "SELECT s.id, s.identity_id, s.acting_as, s.expires_at, s.created_at, \
                           s.last_seen_at, s.auth_time, s.impersonated_by_identity_id, \
                           i.person_id, i.status \
                           FROM session s JOIN identity i ON i.id = s.identity_id \
                           WHERE s.token_hash = $1";

/// Whether the operator behind an impersonated session still holds the
/// relation.
///
/// One extra indexed read, made only for a session that has an operator on it
/// — which is a handful in a deployment's whole history. It is the third of
/// C11.2's five endings and the one that has no event to hang off: revoking an
/// operator does not know which hats they are wearing, so the hat asks at the
/// moment it is put on the head. The identity's own person is one seek by
/// rowid and the relation is `relation_unique_idx`.
pub const OPERATOR_STILL_SQL: &str = "SELECT count(*) AS n FROM identity i \
     JOIN relation r ON r.object_kind = 'platform' AND r.object_id = 0 \
                    AND r.relation = 'operator' AND r.subject_kind = 'person' \
                    AND r.subject_id = i.person_id \
     WHERE i.id = $1";

struct ResolveRow {
    session: SessionRow,
    person_id: Option<Id<Person>>,
    identity_status: String,
}

impl FromRow for ResolveRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            session: SessionRow::from_row(row)?,
            person_id: row.id_opt("person_id")?,
            identity_status: row.text("status")?,
        })
    }
}

/// Resolve a cookie into a principal, without a cache.
///
/// An expired session, a disabled identity and an unknown token all produce
/// [`Principal::Anonymous`]. Not a decline: being signed out is not an error,
/// and the caller decides whether the route it is guarding cares.
pub async fn resolve(store: &impl Reads, token: &Token) -> Outcome<Resolved> {
    resolve_digest(store, &token.digest()).await
}

/// [`resolve`], for a caller that already holds the digest — which is every
/// caller that consulted the cache first.
pub async fn resolve_digest(store: &impl Reads, digest: &Digest) -> Outcome<Resolved> {
    let now = store.now();
    let anonymous = Resolved {
        principal: Principal::Anonymous,
        expires_at: 0,
        wants_renewal: false,
    };

    let Some(row) = store
        .query_opt::<ResolveRow>(RESOLVE_SQL, bind![digest.to_vec()])
        .await?
    else {
        return Ok(anonymous);
    };
    if !row.session.is_live(now) || row.identity_status != "active" {
        return Ok(anonymous);
    }
    // An impersonated session ends when the operator behind it stops being
    // one. Checked here rather than by an event, because `RevokeOperator` has
    // no way to know which sessions somebody is wearing — and checked only for
    // the sessions that have an operator on them, so an ordinary request pays
    // nothing for the rule.
    if let Some(operator) = row.session.impersonated_by_identity_id
        && !operator_still(store, operator).await?
    {
        return Ok(anonymous);
    }

    Ok(Resolved {
        principal: Principal::Member {
            identity: row.session.identity_id,
            person: row.person_id,
            acting_as: row.session.acting_as,
            session: row.session.id,
            impersonated_by: row.session.impersonated_by_identity_id,
        },
        expires_at: row.session.expires_at,
        wants_renewal: row.session.wants_renewal(now),
    })
}

/// Whether an identity's person still holds `platform:* #operator`.
async fn operator_still(store: &impl Reads, operator: Id<Identity>) -> Outcome<bool> {
    Ok(store
        .query::<crate::store::Count>(OPERATOR_STILL_SQL, bind![operator])
        .await?
        .first()
        .is_some_and(|row| row.0 > 0))
}

/// Resolve through a cache, falling back to the database on a miss.
pub async fn resolve_cached(
    store: &impl Reads,
    cache: &PrincipalCache,
    token: &Token,
) -> Outcome<Resolved> {
    let digest = token.digest();
    if let Some(hit) = cache.get(&digest) {
        // A cached session can still have run out of time, and expiry is the
        // one thing that changes without a command to invalidate on.
        if hit.principal == Principal::Anonymous || hit.expires_at > store.now() {
            return Ok(hit);
        }
        cache.forget(&digest);
    }

    let resolved = resolve_digest(store, &digest).await?;
    if let Principal::Member {
        identity, person, ..
    } = resolved.principal
    {
        cache.put(digest, resolved.clone(), identity, person);
    }
    Ok(resolved)
}

/// One identity's row, for a caller that has a principal and wants the rest.
pub async fn identity_of(store: &impl Reads, id: Id<Identity>) -> Outcome<Option<IdentityRow>> {
    Ok(store
        .query_opt::<IdentityRow>(
            "SELECT id, source, person_id, home_zone, status, created_at \
             FROM identity WHERE id = $1",
            bind![id],
        )
        .await?)
}

struct Membership {
    party: i64,
    kind: String,
}

impl FromRow for Membership {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            party: row.int("group_id")?,
            kind: row.text("kind")?,
        })
    }
}

/// Expand a principal into the subject set a relation query runs against.
pub async fn expand(store: &impl Reads, principal: &Principal) -> Outcome<SubjectSet> {
    let mut set = SubjectSet::default();
    match principal {
        Principal::Anonymous => return Ok(set),
        Principal::Bearer { link } => {
            set.link = Some(*link);
            return Ok(set);
        }
        Principal::Member {
            identity, person, ..
        } => {
            set.authenticated = true;
            set.identity = Some(*identity);
            set.person = *person;
        }
    }

    let Some(person) = set.person else {
        return Ok(set);
    };
    for row in store
        .query::<Membership>(
            "SELECT m.group_id, p.kind FROM membership m \
             JOIN party p ON p.id = m.group_id WHERE m.party_id = $1",
            bind![person],
        )
        .await?
    {
        match row.kind.as_str() {
            "organization" => set.organizations.push(Id::new(row.party)),
            "group" => set.groups.push(Id::new(row.party)),
            // A person or a service is not a container; a membership row
            // naming one is a K2 bug, and dropping it here would hide it.
            other => {
                return Err(crate::KernelError::Invariant(format!(
                    "membership of a {other}"
                )));
            }
        }
    }
    Ok(set)
}
