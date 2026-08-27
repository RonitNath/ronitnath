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
mod rows;

use std::collections::BTreeMap;

use rn_api::DiffOp;
use rn_kernel::domain::{FactorKind, Vocabulary};
use rn_kernel::principal::{self, SubjectSet};
use rn_kernel::store::Reads;
use rn_kernel::{Outcome, Principal};
use serde_json::Value;

pub use named::{ALL, Named, Scope};

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
