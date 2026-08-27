//! `POST /api/cmd/<name>` — the write half of the API.
//!
//! Every authoritative change is a command: one transaction, its audit row in
//! the same transaction, one typed event appended to the change feed. The
//! reply is the event's offset, which is what a caller waits on when it needs
//! its own write to be visible.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// A command's route name — the `<name>` in `/api/cmd/<name>`.
///
/// Implemented by every args struct, so a caller cannot post a body to the
/// wrong endpoint and the server's router is derived from the same source as
/// the client's.
pub trait Command {
    /// The route segment this args struct is posted to.
    const NAME: &'static str;
}

/// A command request: the idempotency key, then the command's own arguments
/// flattened alongside it, so the body reads `{"key": "…", "display_name": "…"}`.
///
/// The key is the caller's; replaying it with the same body replays the
/// original reply, and replaying it with a different body is a `409`. That is
/// what makes a retry after a dropped response safe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandEnvelope<T> {
    /// The idempotency key, minted by the caller.
    pub key: Uuid,
    /// The command's arguments.
    #[serde(flatten)]
    pub args: T,
}

impl<T> CommandEnvelope<T> {
    /// Wrap arguments with a freshly minted idempotency key.
    pub fn new(args: T) -> Self {
        Self {
            key: Uuid::new_v4(),
            args,
        }
    }
}

/// A successful command reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandReply {
    /// The change-feed offset the command's event was appended at. A
    /// subscription that has reached this offset has seen this write.
    pub offset: u64,
    /// Whatever the command returns — usually the public id it created.
    /// `null` when it returns nothing.
    pub result: Value,
}

/// The uniform decline body.
///
/// A `403` and a `404` return exactly these bytes. The status code is chosen
/// so browsers behave sensibly; the body says nothing, because a body that
/// distinguished "no such thing" from "not yours" would let a caller
/// enumerate what exists by reading the difference. `409` on a replayed key
/// with a different body uses it too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decline {
    /// Always the same word. Present so the body is valid JSON with a stable
    /// shape rather than an empty document.
    pub decline: String,
}

impl Default for Decline {
    fn default() -> Self {
        Self {
            decline: "declined".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::SignIn;
    use crate::testing::round_trip;

    #[test]
    fn an_envelope_flattens_its_arguments_beside_the_key() {
        let envelope = CommandEnvelope::new(SignIn {
            email: "ronit@isoastra.com".into(),
            password: "hunter2".into(),
        });
        let json = serde_json::to_value(&envelope).expect("serialises");
        assert!(json.get("key").is_some());
        assert_eq!(json["email"], "ronit@isoastra.com");
        assert!(json.get("args").is_none(), "args must not nest");
        round_trip(&envelope);
    }

    #[test]
    fn a_reply_carries_an_offset_and_whatever_the_command_returned() {
        round_trip(&CommandReply {
            offset: 42,
            result: serde_json::json!({"organization": "o_AAAAAAAAAAAAAAAAAAAAAA"}),
        });
        round_trip(&CommandReply {
            offset: 0,
            result: Value::Null,
        });
    }

    #[test]
    fn every_decline_is_the_same_bytes() {
        assert_eq!(
            serde_json::to_string(&Decline::default()).unwrap(),
            r#"{"decline":"declined"}"#
        );
        round_trip(&Decline::default());
    }
}
