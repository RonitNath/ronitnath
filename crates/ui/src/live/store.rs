//! The keyed row set behind a subscription, and the rules for moving it.
//!
//! Pure: no signals, no socket, no clock. A store is one query's result set
//! plus the change-feed offset it has been brought up to, and every message
//! the server can send has exactly one effect on it. Keeping that here means
//! the ordering rules — the ones that decide whether a reconnect resumes or
//! refetches — are testable without a browser.

use std::collections::BTreeMap;

use serde::de::DeserializeOwned;

use rn_api::{DiffOp, SubMessage};

/// What applying a message did. The caller uses this to decide what to ask
/// for next; nothing here is a user-visible state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    /// The rows moved and the offset advanced.
    Changed,
    /// A diff for another query. Not ours, not an error.
    Elsewhere,
    /// A heartbeat. The connection is alive; the rows did not move.
    Alive,
    /// The server could not express our position as a diff. The rows are gone
    /// and the offset is back to zero: resubscribe from scratch.
    Resynced,
    /// The message was refused, and why.
    Refused(Refusal),
}

/// Why a message was refused. A refusal is never applied — a store that
/// accepted a stale or unparseable diff would be silently wrong, which is
/// worse than a visible gap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// A diff at or below the offset already applied: a duplicate delivery, or
    /// a server that rewound. Either way the rows already contain it.
    StaleOffset {
        /// The offset the message carried.
        got: u64,
        /// The offset already applied.
        have: u64,
    },
    /// A row did not parse into the store's row type.
    Unparseable(String),
}

/// One query's live result set.
#[derive(Debug, Clone)]
pub struct Store<T> {
    query: String,
    rows: BTreeMap<String, T>,
    offset: u64,
}

impl<T: DeserializeOwned + Clone> Store<T> {
    /// An empty store for `query`, positioned before the first offset.
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            rows: BTreeMap::new(),
            offset: 0,
        }
    }

    /// The query this store follows.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// The change-feed offset applied so far — what a reconnect resumes from.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// The rows, in key order.
    pub fn rows(&self) -> &BTreeMap<String, T> {
        &self.rows
    }

    /// Replace the whole set — the result of the initial query, before any
    /// diff arrives. The offset moves with it, because the snapshot already
    /// contains everything up to it.
    pub fn seed(&mut self, rows: BTreeMap<String, T>, offset: u64) {
        self.rows = rows;
        self.offset = offset;
    }

    /// Apply one server message.
    ///
    /// Ops within a diff are applied in the order they arrive; the offset moves
    /// only once the whole diff is in, and a diff that cannot be parsed leaves
    /// the store exactly where it was.
    pub fn apply(&mut self, message: &SubMessage) -> Applied {
        match message {
            SubMessage::Heartbeat => Applied::Alive,
            SubMessage::Resync => {
                self.rows.clear();
                self.offset = 0;
                Applied::Resynced
            }
            SubMessage::Diff { offset, query, ops } => {
                if query != &self.query {
                    return Applied::Elsewhere;
                }
                if *offset <= self.offset {
                    return Applied::Refused(Refusal::StaleOffset {
                        got: *offset,
                        have: self.offset,
                    });
                }
                let mut staged = self.rows.clone();
                for op in ops {
                    match op {
                        DiffOp::Put { key, row } => match serde_json::from_value(row.clone()) {
                            Ok(row) => {
                                staged.insert(key.clone(), row);
                            }
                            Err(e) => {
                                return Applied::Refused(Refusal::Unparseable(e.to_string()));
                            }
                        },
                        DiffOp::Del { key } => {
                            staged.remove(key);
                        }
                    }
                }
                self.rows = staged;
                self.offset = *offset;
                Applied::Changed
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    struct Row {
        title: String,
    }

    fn diff(offset: u64, ops: Vec<DiffOp>) -> SubMessage {
        SubMessage::Diff {
            offset,
            query: "documents".into(),
            ops,
        }
    }

    fn put(key: &str, title: &str) -> DiffOp {
        DiffOp::Put {
            key: key.into(),
            row: json!({ "title": title }),
        }
    }

    #[test]
    fn a_put_inserts_and_a_later_put_replaces() {
        let mut store = Store::<Row>::new("documents");
        assert_eq!(
            store.apply(&diff(1, vec![put("r_a", "one")])),
            Applied::Changed
        );
        assert_eq!(store.rows()["r_a"].title, "one");
        assert_eq!(
            store.apply(&diff(2, vec![put("r_a", "two")])),
            Applied::Changed
        );
        assert_eq!(store.rows().len(), 1);
        assert_eq!(store.rows()["r_a"].title, "two");
        assert_eq!(store.offset(), 2);
    }

    #[test]
    fn a_del_removes_and_an_unknown_key_is_not_an_error() {
        let mut store = Store::<Row>::new("documents");
        store.apply(&diff(1, vec![put("r_a", "one")]));
        let ops = vec![
            DiffOp::Del { key: "r_a".into() },
            DiffOp::Del {
                key: "r_ghost".into(),
            },
        ];
        assert_eq!(store.apply(&diff(2, ops)), Applied::Changed);
        assert!(store.rows().is_empty());
    }

    #[test]
    fn ops_apply_in_order_within_one_diff() {
        let mut store = Store::<Row>::new("documents");
        let ops = vec![
            put("r_a", "first"),
            DiffOp::Del { key: "r_a".into() },
            put("r_a", "last"),
        ];
        store.apply(&diff(7, ops));
        assert_eq!(store.rows()["r_a"].title, "last");
    }

    #[test]
    fn an_offset_at_or_below_the_applied_one_is_refused() {
        let mut store = Store::<Row>::new("documents");
        store.apply(&diff(5, vec![put("r_a", "one")]));

        let refused = store.apply(&diff(5, vec![put("r_a", "replay")]));
        assert_eq!(
            refused,
            Applied::Refused(Refusal::StaleOffset { got: 5, have: 5 })
        );
        let refused = store.apply(&diff(4, vec![put("r_b", "older")]));
        assert_eq!(
            refused,
            Applied::Refused(Refusal::StaleOffset { got: 4, have: 5 })
        );

        assert_eq!(
            store.rows()["r_a"].title,
            "one",
            "a refusal changes nothing"
        );
        assert_eq!(store.rows().len(), 1);
        assert_eq!(store.offset(), 5);
    }

    #[test]
    fn an_unparseable_row_leaves_the_store_untouched() {
        let mut store = Store::<Row>::new("documents");
        store.apply(&diff(1, vec![put("r_a", "one")]));
        let ops = vec![
            DiffOp::Del { key: "r_a".into() },
            DiffOp::Put {
                key: "r_b".into(),
                row: json!({ "not_a_title": 3 }),
            },
        ];
        let applied = store.apply(&diff(2, ops));
        assert!(matches!(applied, Applied::Refused(Refusal::Unparseable(_)),));
        assert_eq!(store.rows()["r_a"].title, "one");
        assert_eq!(store.offset(), 1);
    }

    #[test]
    fn a_diff_for_another_query_is_ignored() {
        let mut store = Store::<Row>::new("sessions");
        assert_eq!(
            store.apply(&diff(9, vec![put("r_a", "one")])),
            Applied::Elsewhere
        );
        assert_eq!(store.offset(), 0);
        assert!(store.rows().is_empty());
    }

    #[test]
    fn a_resync_empties_the_store_and_rewinds_to_zero() {
        let mut store = Store::<Row>::new("documents");
        store.apply(&diff(11, vec![put("r_a", "one")]));
        assert_eq!(store.apply(&SubMessage::Resync), Applied::Resynced);
        assert!(store.rows().is_empty());
        assert_eq!(store.offset(), 0);

        // And the offsets it already saw are welcome again after a resync.
        assert_eq!(
            store.apply(&diff(11, vec![put("r_a", "one")])),
            Applied::Changed
        );
    }

    #[test]
    fn a_heartbeat_moves_nothing() {
        let mut store = Store::<Row>::new("documents");
        store.apply(&diff(3, vec![put("r_a", "one")]));
        assert_eq!(store.apply(&SubMessage::Heartbeat), Applied::Alive);
        assert_eq!(store.offset(), 3);
        assert_eq!(store.rows().len(), 1);
    }

    #[test]
    fn a_seed_positions_the_store_at_the_snapshot_offset() {
        let mut store = Store::<Row>::new("documents");
        store.seed(
            BTreeMap::from([(
                "r_a".to_owned(),
                Row {
                    title: "one".into(),
                },
            )]),
            40,
        );
        assert_eq!(store.offset(), 40);
        assert_eq!(
            store.apply(&diff(40, vec![put("r_a", "again")])),
            Applied::Refused(Refusal::StaleOffset { got: 40, have: 40 }),
            "the snapshot already contains its own offset"
        );
        assert_eq!(
            store.apply(&diff(41, vec![put("r_a", "next")])),
            Applied::Changed
        );
    }
}
