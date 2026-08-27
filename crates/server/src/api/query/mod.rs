//! `GET /api/q/<name>` — the read half, and the diffs a subscription derives.
//!
//! Every handler here is given a [`ReadStore`], which has no `execute` and no
//! `commit` (`kernel::store`). "Nothing reachable from a GET writes" is
//! therefore a property of the type the handler holds rather than a rule
//! somebody has to remember, and `read_store_has_no_execute` asserts the
//! absence.
//!
//! A query is two things: the rows it returns, and — for `/api/sub` — which of
//! its keys a change-feed event could have moved. [`Named::changed`] is that
//! second half, one arm per query, and it answers in *keys* rather than in
//! rows: the socket re-reads only what an event named, so a command against
//! one session does not re-serialise every session a person owns.
//!
//! The result sets in this leg are all "mine" and bounded by a human's own
//! behaviour — sessions on their devices, identities they have registered, a
//! page of their own audit rows. Restricting a re-read to a key list is
//! therefore done in Rust over the same statement rather than with a second
//! parameterised one: the statement a subscription runs is the statement the
//! GET runs, which is what stops the two answering differently.

pub mod member_documents;
pub mod member_groups;
pub mod member_matches;
mod rows;

use std::collections::BTreeMap;

use rn_api::DiffOp;
use rn_kernel::domain::{FactorKind, Vocabulary};
use rn_kernel::ids::IdKey;
use rn_kernel::principal::{self, SubjectSet};
use rn_kernel::store::Reads;
use rn_kernel::{Event, Outcome, Principal};
use serde_json::Value;

/// How many audit rows one page carries.
pub const AUDIT_PAGE: usize = 100;

/// The largest page a caller may ask for.
const AUDIT_MAX: usize = 500;

/// One row of a result set: the key a diff addresses it by, and the row.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The key within this query's result set.
    pub key: String,
    /// The row, shaped by the query.
    pub value: Value,
}

/// The queries this leg serves.
///
/// A closed set, not a registry: every one of them is `mine`, and a query that
/// took an owner parameter would need an authorisation this enum's shape makes
/// unnecessary. U2–U4 add their own alongside their pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Named {
    /// The sessions of the acting identity — the device list.
    Sessions,
    /// The identities resolved onto the acting person, and their factors.
    Identities,
    /// The acting identity's own audit rows, forward-paginated by offset.
    Audit,
    /// The groups the acting person belongs to.
    Groups,
    /// Everyone in those groups.
    GroupMembers,
    /// The invitations the acting identity minted.
    Invitations,
    /// Every document the acting principal can reach.
    Documents,
    /// One document, in full. The only query here that takes a row.
    Document,
    /// Every grant on those documents.
    Shares,
    /// Proposed pairs involving the acting person's registrations.
    Matches,
}

/// How a subscription keeps a query up to date.
///
/// Two shapes, because the member tier has two shapes of result set. A
/// query whose rows all belong to the asking *identity* moves only when that
/// identity's own events do, and the event names the row — so the socket
/// re-reads one key. A query whose rows are a *set* the reader shares with
/// other people — a group's roster, a document somebody else edited — moves
/// on events about strangers, and the event cannot name a key the reader
/// holds without the socket knowing what the reader already has. So the set
/// is re-read whole and diffed against what was last sent, which is the only
/// way a row that *left* the set can be reported at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Keys named by the event ([`Named::changed`]).
    Identity,
    /// The whole result set, re-read and diffed ([`Named::watches`]).
    Set,
    /// Not subscribable: it takes a row, and a subscription names none.
    Once,
}

/// Every query name this build serves, for the route matrix.
pub const ALL: &[Named] = &[
    Named::Sessions,
    Named::Identities,
    Named::Audit,
    Named::Groups,
    Named::GroupMembers,
    Named::Invitations,
    Named::Documents,
    Named::Document,
    Named::Shares,
    Named::Matches,
];

impl Named {
    /// The `<name>` in `/api/q/<name>`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sessions => "sessions",
            Self::Identities => "identities",
            Self::Audit => "audit",
            Self::Groups => "groups",
            Self::GroupMembers => "group-members",
            Self::Invitations => "invitations",
            Self::Documents => "documents",
            Self::Document => "document",
            Self::Shares => "shares",
            Self::Matches => "matches",
        }
    }

    /// How a subscription keeps this query up to date.
    #[must_use]
    pub const fn scope(self) -> Scope {
        match self {
            Self::Sessions | Self::Identities | Self::Audit => Scope::Identity,
            Self::Groups
            | Self::GroupMembers
            | Self::Invitations
            | Self::Documents
            | Self::Shares
            | Self::Matches => Scope::Set,
            Self::Document => Scope::Once,
        }
    }

    /// Whether an event could have moved a [`Scope::Set`] query's rows.
    ///
    /// Coarse on purpose: it answers about the *kind* of event, not about
    /// whose it was, because whose it was is a question only a query can
    /// answer and the answer is the re-read. Naming an event that turns out to
    /// have changed nothing costs one read and sends nothing; missing one
    /// leaves a reader looking at a stale page, so the arms err towards
    /// reading.
    #[must_use]
    pub const fn watches(self, event: &Event) -> bool {
        match self {
            Self::Groups => matches!(
                event,
                Event::GroupCreated { .. }
                    | Event::LinkClaimed { .. }
                    | Event::RoleSet { .. }
                    | Event::Left { .. }
                    | Event::Transferred { .. }
            ),
            Self::GroupMembers => matches!(
                event,
                Event::GroupCreated { .. }
                    | Event::LinkClaimed { .. }
                    | Event::RoleSet { .. }
                    | Event::Left { .. }
                    | Event::Shared { .. }
                    | Event::Revoked { .. }
                    | Event::PersonMerged { .. }
            ),
            Self::Invitations => matches!(event, Event::Invited { .. } | Event::LinkClaimed { .. }),
            Self::Documents => matches!(
                event,
                Event::DocumentCreated { .. }
                    | Event::DocumentEdited { .. }
                    | Event::DocumentPublished { .. }
                    | Event::Shared { .. }
                    | Event::Revoked { .. }
                    | Event::Transferred { .. }
            ),
            Self::Shares => matches!(
                event,
                Event::Shared { .. }
                    | Event::Revoked { .. }
                    | Event::DocumentCreated { .. }
                    | Event::Transferred { .. }
            ),
            Self::Matches => matches!(
                event,
                Event::MatchProposed { .. }
                    | Event::MatchRejected { .. }
                    | Event::PersonMerged { .. }
                    | Event::IdentityLinked { .. }
                    | Event::PersonSplit { .. }
            ),
            Self::Sessions | Self::Identities | Self::Audit | Self::Document => false,
        }
    }

    /// Read a name, refusing anything not served.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        ALL.iter().copied().find(|q| q.as_str() == raw)
    }

    /// Which keys an event could have moved, for this principal.
    ///
    /// An empty list means the query is untouched and the socket sends
    /// nothing. It is deliberately allowed to name a key that turns out not to
    /// have changed — a re-read that finds the same row emits no op — but it
    /// must never *miss* one, which is why the arms match on the event's
    /// subject rather than on its name alone.
    #[must_use]
    pub fn changed(self, event: &Event, principal: &Principal, key: &IdKey) -> Vec<String> {
        let Some(mine) = principal.identity() else {
            return Vec::new();
        };
        // Only the identity-scoped queries answer here, and only about their
        // own reader: a set-scoped query moves on other people's events, which
        // is precisely what a key named off this event cannot express.
        if self.scope() != Scope::Identity || event.touches_identity() != Some(mine) {
            return Vec::new();
        }
        match (self, event) {
            (Self::Sessions, Event::Registered { session, .. })
            | (Self::Sessions, Event::SignedIn { session, .. })
            | (Self::Sessions, Event::SignedOut { session, .. })
            | (Self::Sessions, Event::SessionRevoked { session, .. }) => {
                vec![session.public(key).as_str().to_owned()]
            }
            (Self::Sessions, _) => Vec::new(),
            // A factor's row is nested inside its identity's, so the identity
            // is the key that moved, not the factor.
            (Self::Identities, Event::Registered { identity, .. })
            | (Self::Identities, Event::FactorAdded { identity, .. })
            | (Self::Identities, Event::FactorRemoved { identity, .. })
            | (Self::Identities, Event::EmailVerified { identity, .. }) => {
                vec![identity.public(key).as_str().to_owned()]
            }
            (Self::Identities, _) => Vec::new(),
            (Self::Audit, _) => Vec::new(),
            _ => Vec::new(),
        }
    }
}

/// What a query was asked for.
#[derive(Debug, Clone, Default)]
pub struct Params {
    /// `audit`: the offset already seen.
    pub after: u64,
    /// `audit`: how many rows to return.
    pub limit: Option<usize>,
    /// `document`: which row. A public id, validated where it is decoded.
    pub id: Option<String>,
}

impl Params {
    /// Read the parameters out of a query string's pairs.
    #[must_use]
    pub fn from_pairs<'a>(pairs: impl Iterator<Item = (&'a str, &'a str)>) -> Self {
        let mut params = Self::default();
        for (name, value) in pairs {
            match name {
                "after" => params.after = value.parse().unwrap_or(0),
                "limit" => params.limit = value.parse().ok(),
                // A public id is 24 base64url characters and a prefix, so
                // there is nothing here to percent-decode; the length bound is
                // what keeps a junk query string out of the id decoder.
                "id" if value.len() <= 32 => params.id = Some(value.to_owned()),
                _ => {}
            }
        }
        params
    }

    pub(super) fn page(&self) -> i64 {
        self.limit.unwrap_or(AUDIT_PAGE).clamp(1, AUDIT_MAX) as i64
    }
}

/// Run a query for a principal.
pub async fn read(
    reads: &impl Reads,
    query: Named,
    principal: &Principal,
    params: &Params,
) -> Outcome<Vec<Row>> {
    let Some(identity) = principal.identity() else {
        return Ok(Vec::new());
    };
    let key = reads.ids();
    match query {
        Named::Sessions => rows::sessions(reads, identity, principal, key).await,
        Named::Identities => rows::identities(reads, identity, principal, key).await,
        Named::Audit => rows::audit(reads, identity, params).await,
        Named::Invitations => member_groups::invitations(reads, identity, key).await,
        Named::Groups => {
            member_groups::groups(reads, &expanded(reads, principal).await?, key).await
        }
        Named::GroupMembers => {
            member_groups::group_members(reads, &expanded(reads, principal).await?, key).await
        }
        Named::Documents => {
            member_documents::documents(reads, &expanded(reads, principal).await?, key).await
        }
        Named::Document => {
            member_documents::document(reads, &expanded(reads, principal).await?, params, key).await
        }
        Named::Shares => {
            member_documents::shares(reads, &expanded(reads, principal).await?, key).await
        }
        Named::Matches => {
            member_matches::matches(reads, &expanded(reads, principal).await?, key).await
        }
    }
}

/// The subject set a relation-shaped query runs against.
///
/// One expansion per request, from the principal the cookie resolved to — the
/// same call `check()` makes, so a listing and an authorisation cannot see
/// different memberships.
async fn expanded(reads: &impl Reads, principal: &Principal) -> Outcome<SubjectSet> {
    principal::expand(reads, principal).await
}

/// Turn a whole re-read into diff ops against what a subscription last sent.
///
/// A key whose row is unchanged produces nothing, a key whose row moved
/// produces a `put`, and a key the re-read no longer finds produces a `del` —
/// which is the half [`ops`] cannot do for a set the reader shares with other
/// people, because there the event names no key the reader holds.
#[must_use]
pub fn diff(previous: &BTreeMap<String, Value>, rows: &[Row]) -> Vec<DiffOp> {
    let mut ops = Vec::new();
    for row in rows {
        if previous.get(&row.key) != Some(&row.value) {
            ops.push(DiffOp::Put {
                key: row.key.clone(),
                row: row.value.clone(),
            });
        }
    }
    for key in previous.keys() {
        if !rows.iter().any(|row| &row.key == key) {
            ops.push(DiffOp::Del { key: key.clone() });
        }
    }
    ops
}

/// A result set as a subscription remembers it.
#[must_use]
pub fn remember(rows: &[Row]) -> BTreeMap<String, Value> {
    rows.iter()
        .map(|row| (row.key.clone(), row.value.clone()))
        .collect()
}

/// Turn a re-read into diff ops for exactly the keys an event named.
///
/// A key the re-read still finds is a `put`; one it does not is a `del`. The
/// client cannot tell a deletion from a row that left the set, and must not
/// need to (`rn_api::sub`).
#[must_use]
pub fn ops(keys: &[String], rows: Vec<Row>) -> Vec<DiffOp> {
    let found: BTreeMap<String, Value> = rows.into_iter().map(|row| (row.key, row.value)).collect();
    keys.iter()
        .map(|key| match found.get(key) {
            Some(row) => DiffOp::Put {
                key: key.clone(),
                row: row.clone(),
            },
            None => DiffOp::Del { key: key.clone() },
        })
        .collect()
}

/// The whole result set as the array a `GET` returns.
#[must_use]
pub fn body(rows: &[Row]) -> Value {
    Value::Array(rows.iter().map(|row| row.value.clone()).collect())
}

/// Whether a factor kind is one this build produces — used by the identities
/// row to keep the vocabulary honest against the kernel's.
#[must_use]
pub fn is_built(kind: &str) -> bool {
    FactorKind::parse(kind).is_some_and(FactorKind::is_built)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rn_kernel::ids::Id;
    use serde_json::json;

    #[test]
    fn a_name_the_build_does_not_serve_is_refused_rather_than_guessed() {
        assert_eq!(Named::parse("sessions"), Some(Named::Sessions));
        assert_eq!(Named::parse("audit"), Some(Named::Audit));
        assert_eq!(Named::parse("documents"), None);
        assert_eq!(Named::parse(""), None);
        assert_eq!(Named::parse("../whoami"), None);
    }

    #[test]
    fn the_audit_page_is_bounded_however_it_is_asked_for() {
        assert_eq!(Params::default().page(), AUDIT_PAGE as i64);
        assert_eq!(
            Params {
                limit: Some(0),
                ..Params::default()
            }
            .page(),
            1
        );
        assert_eq!(
            Params {
                limit: Some(100_000),
                ..Params::default()
            }
            .page(),
            AUDIT_MAX as i64
        );
    }

    #[test]
    fn parameters_survive_junk_without_becoming_it() {
        let params = Params::from_pairs(
            [("after", "40"), ("limit", "banana"), ("owner", "p_x")].into_iter(),
        );
        assert_eq!(params.after, 40);
        assert_eq!(params.limit, None);
    }

    #[test]
    fn a_key_the_reread_lost_becomes_a_deletion() {
        let keys = vec!["a".to_owned(), "b".to_owned()];
        let ops = ops(
            &keys,
            vec![Row {
                key: "a".to_owned(),
                value: json!({"n": 1}),
            }],
        );
        assert_eq!(
            ops,
            vec![
                DiffOp::Put {
                    key: "a".to_owned(),
                    row: json!({"n": 1})
                },
                DiffOp::Del {
                    key: "b".to_owned()
                },
            ]
        );
    }

    #[test]
    fn an_event_about_somebody_else_moves_nothing() {
        let key = IdKey::from_hex("000102030405060708090a0b0c0d0e0f").expect("a test key");
        let mine = Principal::Member {
            identity: Id::new(1),
            person: Some(Id::new(2)),
            acting_as: Id::new(2),
            session: Id::new(3),
        };
        let theirs = Event::SignedIn {
            identity: Id::new(99),
            session: Id::new(98),
        };
        assert!(Named::Sessions.changed(&theirs, &mine, &key).is_empty());
        assert!(Named::Identities.changed(&theirs, &mine, &key).is_empty());

        let ours = Event::SignedIn {
            identity: Id::new(1),
            session: Id::new(4),
        };
        assert_eq!(Named::Sessions.changed(&ours, &mine, &key).len(), 1);
        assert!(Named::Identities.changed(&ours, &mine, &key).is_empty());
    }

    #[test]
    fn an_anonymous_principal_has_no_rows_to_move() {
        let key = IdKey::from_hex("000102030405060708090a0b0c0d0e0f").expect("a test key");
        let event = Event::SignedIn {
            identity: Id::new(1),
            session: Id::new(2),
        };
        for query in ALL {
            assert!(
                query
                    .changed(&event, &Principal::Anonymous, &key)
                    .is_empty(),
                "{}",
                query.as_str()
            );
        }
    }

    #[test]
    fn the_factor_vocabulary_is_the_kernels() {
        assert!(is_built("email"));
        assert!(is_built("password"));
        assert!(!is_built("passkey"));
        assert!(!is_built("nonsense"));
    }
}
