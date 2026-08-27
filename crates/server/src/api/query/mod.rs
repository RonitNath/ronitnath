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
mod named;
mod oidc;
pub mod org;
pub mod org_feed;
pub mod org_rows;
pub mod platform;
mod platform_audit;
mod platform_changed;
pub mod platform_filters;
mod platform_find;
mod platform_identities;
mod platform_keys;
mod platform_links;
mod platform_matches;
mod platform_nodes;
mod platform_operators;
mod platform_parties;
mod platform_person;
mod platform_resources;
mod platform_sessions;
mod platform_statements;
mod rows;

use std::collections::BTreeMap;

use rn_api::{DiffOp, PublicId};
use rn_kernel::domain::{FactorKind, Vocabulary};
use rn_kernel::principal::{self, SubjectSet};
use rn_kernel::store::Reads;
use rn_kernel::{Outcome, Principal};
use serde_json::Value;

pub use named::{ALL, Named, Scope};
pub use org::Touch;
pub use platform::Platform;

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

/// What a query was asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Params {
    /// `audit`: the offset already seen.
    pub after: u64,
    /// `audit`: how many rows to return.
    pub limit: Option<usize>,
    /// `document`: which row. A public id, validated where it is decoded.
    pub id: Option<String>,
    /// The organization every `org*` query is scoped to.
    pub org: Option<PublicId>,
    /// A group inside it, where a query drills into one.
    pub group: Option<PublicId>,
    /// `org-audit`: show only this command. Empty means all of them.
    pub command: Option<String>,
    /// A `/platform` drill-in's subject: the public id of the row it opened.
    ///
    /// Text rather than an integer, and safe to take undecoded: a public id is
    /// a prefix over base64url, which has nothing a percent-decoder would
    /// change. It is validated by decryption before it reaches a statement
    /// (`platform::subject`), so a forged one addresses nothing.
    pub subject: Option<String>,
    /// `platform-audit`: the offset to walk backwards from.
    pub before: Option<u64>,
    /// `platform-find`: the one box. A handle, an address or a public id —
    /// which of the three it is, is decided by its shape and not by a mode
    /// switch the reader has to set (`platform_find`).
    pub q: Option<String>,
    /// `platform-audit`: only rows this identity ran. A public id, validated
    /// by decryption before it reaches a statement.
    pub actor: Option<String>,
    /// `platform-audit`: only rows attributed to this party — the *hat*,
    /// which after `ActAs` and under impersonation is not the actor.
    pub hat: Option<String>,
    /// `platform-audit`: only rows that were *about* this row, read off the
    /// `audit_object` side table.
    pub object: Option<String>,
    /// `platform-audit`: the start of the time range, in unix seconds.
    pub from: Option<i64>,
    /// `platform-audit`: the end of it. Half-open, so two adjacent ranges
    /// neither drop a row nor count one twice.
    pub to: Option<i64>,
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
                // A malformed id is dropped rather than carried: the query it
                // was meant for then declines for want of a scope, which is
                // the same answer as naming somebody else's organization.
                "org" => params.org = value.parse().ok(),
                "group" => params.group = value.parse().ok(),
                "command" => params.command = command_name(value),
                // Bounded like `id` above and for the same reason: it is
                // carried until `platform::subject` decrypts it, and a
                // subscription holds one per subscribed query for the life
                // of the socket. A public id is 24 characters.
                "subject" if value.len() <= 32 => params.subject = Some(value.to_owned()),
                "before" => params.before = value.parse().ok(),
                // Bounded before it reaches any of the three seeks; the seeks
                // themselves decide which of them this shape could answer.
                "q" if value.len() <= 320 => params.q = Some(value.to_owned()),
                // Public ids, carried undecoded and bounded exactly like
                // `subject` above, for the same reason.
                "actor" if value.len() <= 32 => params.actor = Some(value.to_owned()),
                "hat" if value.len() <= 32 => params.hat = Some(value.to_owned()),
                "object" if value.len() <= 32 => params.object = Some(value.to_owned()),
                "from" => params.from = value.parse().ok(),
                "to" => params.to = value.parse().ok(),
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
    if let Named::Platform(platform) = query {
        return platform::read(reads, platform, principal, params).await;
    }
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
        // The reader's own, always: the statement is keyed on the acting
        // person and takes no parameter that could name anybody else's.
        Named::Authorizations => match expanded(reads, principal).await?.person {
            Some(person) => oidc::authorizations(reads, person, key).await,
            None => Ok(Vec::new()),
        },
        // Every organization and platform query returned above.
        Named::Org
        | Named::OrgMembers
        | Named::OrgGroups
        | Named::OrgContacts
        | Named::OrgDocuments
        | Named::OrgInvitations
        | Named::OrgAudit
        | Named::Platform(_) => Ok(Vec::new()),
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
    use serde_json::json;

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
    fn the_factor_vocabulary_is_the_kernels() {
        assert!(is_built("email"));
        assert!(is_built("password"));
        assert!(!is_built("passkey"));
        assert!(!is_built("nonsense"));
    }
}
