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

pub mod org;
pub mod org_feed;
pub mod org_rows;
mod rows;

use std::collections::BTreeMap;

use rn_api::{DiffOp, PublicId};
use rn_kernel::domain::{FactorKind, Vocabulary};
use rn_kernel::ids::IdKey;
use rn_kernel::store::Reads;
use rn_kernel::{Event, Outcome, Principal};
use serde_json::Value;

pub use org::Touch;

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
    /// One organization: who owns it, when it was founded, and its counts.
    Org,
    /// The memberships of an organization, or of one of its groups.
    OrgMembers,
    /// The groups an organization owns.
    OrgGroups,
    /// Whose contact details are visible inside one of its groups.
    OrgContacts,
    /// The documents an organization owns, and the grants on each.
    OrgDocuments,
    /// The invitation links minted into its containers.
    OrgInvitations,
    /// Its audit tail, forward-paginated by offset.
    OrgAudit,
}

/// Every query name this build serves, for the route matrix.
pub const ALL: &[Named] = &[
    Named::Sessions,
    Named::Identities,
    Named::Audit,
    Named::Org,
    Named::OrgMembers,
    Named::OrgGroups,
    Named::OrgContacts,
    Named::OrgDocuments,
    Named::OrgInvitations,
    Named::OrgAudit,
];

impl Named {
    /// The `<name>` in `/api/q/<name>`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sessions => "sessions",
            Self::Identities => "identities",
            Self::Audit => "audit",
            Self::Org => "org",
            Self::OrgMembers => "org-members",
            Self::OrgGroups => "org-groups",
            Self::OrgContacts => "org-contacts",
            Self::OrgDocuments => "org-documents",
            Self::OrgInvitations => "org-invitations",
            Self::OrgAudit => "org-audit",
        }
    }

    /// Whether this query is scoped to an organization rather than to the
    /// acting identity — which is what decides whether it needs the `org`
    /// parameter and the membership re-read behind it.
    #[must_use]
    pub const fn is_org(self) -> bool {
        matches!(
            self,
            Self::Org
                | Self::OrgMembers
                | Self::OrgGroups
                | Self::OrgContacts
                | Self::OrgDocuments
                | Self::OrgInvitations
                | Self::OrgAudit
        )
    }

    /// What an event did to this query's rows, for the socket.
    ///
    /// The member tier's queries name the keys that moved; the organization's
    /// answer in whole sets where an event cannot say which of its rows is
    /// ours ([`org::touched`]).
    #[must_use]
    pub fn touched(
        self,
        event: &Event,
        principal: &Principal,
        key: &IdKey,
        params: &Params,
    ) -> Touch {
        if self.is_org() {
            return org::touched(self, event, key, params);
        }
        match self.changed(event, principal, key) {
            keys if keys.is_empty() => Touch::None,
            keys => Touch::Keys(keys),
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
        if event.touches_identity() != Some(mine) {
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
            // An organization's rows are not the acting identity's, so they
            // are decided by `org::touched` and never reach here.
            _ => Vec::new(),
        }
    }
}

/// What a query was asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Params {
    /// `audit`: the offset already seen.
    pub after: u64,
    /// `audit`: how many rows to return.
    pub limit: Option<usize>,
    /// The organization every `org*` query is scoped to.
    pub org: Option<PublicId>,
    /// A group inside it, where a query drills into one.
    pub group: Option<PublicId>,
    /// `org-audit`: show only this command. Empty means all of them.
    pub command: Option<String>,
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
                // A malformed id is dropped rather than carried: the query it
                // was meant for then declines for want of a scope, which is
                // the same answer as naming somebody else's organization.
                "org" => params.org = value.parse().ok(),
                "group" => params.group = value.parse().ok(),
                "command" => params.command = command_name(value),
                _ => {}
            }
        }
        params
    }

    pub(super) fn page(&self) -> i64 {
        self.limit.unwrap_or(AUDIT_PAGE).clamp(1, AUDIT_MAX) as i64
    }
}

/// A command name the contract declares, or nothing.
///
/// The audit filter goes into a statement as a value, never as text spliced
/// into SQL — but it is still checked against the vocabulary, because a filter
/// naming a command that does not exist is a caller bug worth answering with
/// every row rather than none.
fn command_name(raw: &str) -> Option<String> {
    rn_api::commands::ALL_COMMAND_NAMES
        .iter()
        .find(|name| **name == raw)
        .map(|name| (*name).to_owned())
}

/// Run a query for a principal.
pub async fn read(
    reads: &impl Reads,
    query: Named,
    principal: &Principal,
    params: &Params,
) -> Outcome<Vec<Row>> {
    if query.is_org() {
        return org::read(reads, query, principal, params).await;
    }
    let Some(identity) = principal.identity() else {
        return Ok(Vec::new());
    };
    let key = reads.ids();
    match query {
        Named::Sessions => rows::sessions(reads, identity, principal, key).await,
        Named::Identities => rows::identities(reads, identity, principal, key).await,
        Named::Audit => rows::audit(reads, identity, params).await,
        // Every organization query returned above.
        _ => Ok(Vec::new()),
    }
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

/// Turn a whole-set re-read into diff ops against what was last sent.
///
/// The socket uses this for the queries whose events cannot name a key —
/// "somebody created a group" does not say whose organization it landed in.
/// Diffing against what this connection actually sent is what keeps such a
/// query correct without ever naming a row the reader is not entitled to: a
/// `del` can only mention a key that was previously `put`.
#[must_use]
pub fn set_ops(sent: &mut BTreeMap<String, Value>, rows: Vec<Row>) -> Vec<DiffOp> {
    let current: BTreeMap<String, Value> =
        rows.into_iter().map(|row| (row.key, row.value)).collect();
    let mut ops = Vec::new();
    for (key, value) in &current {
        if sent.get(key) != Some(value) {
            ops.push(DiffOp::Put {
                key: key.clone(),
                row: value.clone(),
            });
        }
    }
    for key in sent.keys() {
        if !current.contains_key(key) {
            ops.push(DiffOp::Del { key: key.clone() });
        }
    }
    *sent = current;
    ops
}

/// Remember what a keyed diff sent, so a later whole-set diff over the same
/// query has something to compare against.
pub fn remember(sent: &mut BTreeMap<String, Value>, ops: &[DiffOp]) {
    for op in ops {
        match op {
            DiffOp::Put { key, row } => {
                sent.insert(key.clone(), row.clone());
            }
            DiffOp::Del { key } => {
                sent.remove(key);
            }
        }
    }
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
