//! `WS /api/sub` — the live half of the API.
//!
//! A bundle names the queries it wants and the offset it has already seen; the
//! server derives row diffs from the change feed and pushes them. Reconnecting
//! is resuming from an offset, not refetching, and invalidate-and-refetch
//! survives only as the resync after a release changes what a query means.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A named query with its parameters, as a subscription requests it.
///
/// The parameters are the same string pairs `GET /api/q/<name>?…` takes, so a
/// query has one shape whether it is polled or subscribed to.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct QueryRef {
    /// The query name — the `<name>` in `/api/q/<name>`.
    pub query: String,
    /// Its parameters. Ordered, so two equal subscriptions have one key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
}

/// What the client sends when it opens or updates a subscription.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubRequest {
    /// The queries to keep live. Sending this again replaces the set.
    pub subscribe: Vec<QueryRef>,
    /// The change-feed offset the client has already applied. A reconnecting
    /// client sends its last offset and receives only what it missed.
    pub from: u64,
}

/// One row-level change within a diff.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum DiffOp {
    /// The row now has this content — insert it or replace it.
    Put {
        /// The row's key within the query's result set.
        key: String,
        /// The row itself, shaped by the query.
        row: Value,
    },
    /// The row has left the result set — because it was deleted, or because it
    /// no longer matches. The client cannot tell the two apart and must not
    /// need to.
    Del {
        /// The row's key within the query's result set.
        key: String,
    },
}

/// What the server sends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SubMessage {
    /// Changes to one query's result set, at one offset.
    Diff {
        /// The change-feed offset these ops bring the client up to.
        offset: u64,
        /// Which subscribed query they belong to.
        query: String,
        /// The ops, in apply order.
        #[serde(rename = "diff")]
        ops: Vec<DiffOp>,
    },
    /// The server can no longer express the client's position as a diff —
    /// after a release changed a query's shape, or after the client's offset
    /// fell off the retained log. Refetch everything and resubscribe.
    Resync,
    /// Liveness, every 25 s. Carries nothing; a client that stops seeing them
    /// reconnects.
    Heartbeat,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::round_trip;

    #[test]
    fn a_request_names_queries_and_the_offset_to_resume_from() {
        let request = SubRequest {
            subscribe: vec![
                QueryRef {
                    query: "documents".into(),
                    params: BTreeMap::from([("owner".into(), "o_AAAAAAAAAAAAAAAAAAAAAA".into())]),
                },
                QueryRef {
                    query: "sessions".into(),
                    ..QueryRef::default()
                },
            ],
            from: 900,
        };
        let json = serde_json::to_value(&request).expect("serialises");
        assert!(
            json["subscribe"][1].get("params").is_none(),
            "an empty parameter set is absent, not an empty object"
        );
        round_trip(&request);
    }

    #[test]
    fn a_diff_is_tagged_and_carries_its_ops_under_diff() {
        let message = SubMessage::Diff {
            offset: 901,
            query: "documents".into(),
            ops: vec![
                DiffOp::Put {
                    key: "r_AAAAAAAAAAAAAAAAAAAAAA".into(),
                    row: serde_json::json!({"title": "Kernel report"}),
                },
                DiffOp::Del {
                    key: "r_BBBBBBBBBBBBBBBBBBBBBB".into(),
                },
            ],
        };
        let json = serde_json::to_value(&message).expect("serialises");
        assert_eq!(json["type"], "diff");
        assert_eq!(json["diff"][0]["op"], "put");
        assert_eq!(json["diff"][1]["op"], "del");
        assert!(json["diff"][1].get("row").is_none());
        round_trip(&message);
    }

    #[test]
    fn the_control_messages_are_bare_tags() {
        assert_eq!(
            serde_json::to_string(&SubMessage::Resync).unwrap(),
            r#"{"type":"resync"}"#
        );
        assert_eq!(
            serde_json::to_string(&SubMessage::Heartbeat).unwrap(),
            r#"{"type":"heartbeat"}"#
        );
        round_trip(&SubMessage::Resync);
        round_trip(&SubMessage::Heartbeat);
    }
}
