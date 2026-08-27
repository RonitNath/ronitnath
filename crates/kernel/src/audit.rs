//! The audit row, which is also the change feed.
//!
//! One row per command, written inside that command's transaction. Three jobs
//! ride on the same row, and they are the same row on purpose:
//!
//! * **the record** — who ran what, as whom, when;
//! * **the offset** — `audit.id` is where a feed consumer resumes, so there is
//!   no second counter to disagree with the order of the rows it describes;
//! * **the idempotency key** — `audit.key` is UNIQUE and NOT NULL, so a replay
//!   collides in the database rather than at a check some command might skip.
//!
//! [`replay`] is what turns that collision into an answer: the same body gets
//! the original reply, a different body under the same key gets
//! [`KernelError::Conflict`]. The digest is over the command's own arguments,
//! canonicalised by `serde_json`, so "the same body" does not depend on key
//! order or whitespace.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use crate::bind;
use crate::error::{KernelError, Outcome};
use crate::event::{Committed, Event};
use crate::store::{Cursor, FromRow, Reads, RowError};

/// The kinds `audit_object` carries.
///
/// A vocabulary rather than a set of table names, because a person and an
/// organization are both `party` rows and a caller asking about one is not
/// asking about the other's table. Stated here so a command and the query that
/// will read it back agree by construction.
pub mod object {
    /// A person, an organization or a group — a `party` row.
    pub const PARTY: &str = "party";
    /// A `session` row.
    pub const SESSION: &str = "session";
    /// A `resource` row: a document, an organization's or a group's resource.
    pub const RESOURCE: &str = "resource";
    /// A `link` row — an invitation.
    pub const LINK: &str = "link";
    /// An `identity` row.
    pub const IDENTITY: &str = "identity";
    /// A `factor` row.
    pub const FACTOR: &str = "factor";
    /// An `oidc_client` row — a registered relying party.
    pub const CLIENT: &str = "oidc_client";
    /// An `oidc_key` row, addressed by its rowid; the `kid` is on the payload.
    pub const KEY: &str = "oidc_key";
}

/// Record that this command's audit row was about a row.
///
/// The statement finds the audit row by the idempotency key rather than by a
/// `RETURNING id` the command would have to thread through, and that is not
/// only convenience: `audit.key` is UNIQUE and NOT NULL, the audit statement
/// is in this same batch, and every statement in a batch carries the command's
/// guard — so this either finds the row the batch just wrote or finds nothing
/// because the batch wrote nothing. There is no third case in which it could
/// attach to somebody else's ruling.
///
/// `INSERT OR IGNORE` because a command may name the same row twice — a
/// transfer whose new owner is also the container — and saying so once is the
/// fact.
///
/// [`Expect::Any`](crate::store::Expect::Any): a guarded audit statement that
/// wrote nothing means this writes nothing, which is right rather than a miss.
pub fn object_stmt(key: Uuid, kind: &'static str, id: i64) -> crate::store::Stmt {
    // The binding order is the statement's order of *first appearance*, not
    // the numbers: SQLite reads `$N` as a name and assigns it the index it is
    // first seen at. `$1` and `$2` are in the projection, `$3` in the `WHERE`,
    // so that is the order they are bound in.
    crate::store::Stmt::any(OBJECT_SQL, crate::bind![kind, id, key.to_string()])
}

/// [`object_stmt`]'s statement, exposed so the "no full scan" gate can assert
/// that finding the audit row by its key is the seek it looks like.
pub const OBJECT_SQL: &str = "INSERT OR IGNORE INTO audit_object (audit_id, kind, id) \
     SELECT a.id, $1, $2 FROM audit a WHERE a.key = $3";

/// The argument names whose values never reach a digest.
///
/// `audit.request_digest` is a SHA-256 over a command's arguments, and the
/// audit table is also the change feed and has no retention — so a command
/// that carried a password would leave a permanent offline-guessable digest of
/// it in the deployment's most-read table. `ReAuthenticate` already answered
/// this for itself by handing [`digest_of`] a redacted struct
/// (`cmd::reauthenticate::Redacted`); this list is the same answer for the
/// commands that hand over their real arguments, made where it cannot be
/// forgotten by the next one.
///
/// A *name* list rather than a type-level opt-in, because the alternative is a
/// trait implemented forty-nine times, forty-seven of which would say
/// "nothing", and the one that mattered would be the one somebody left out.
///
/// The cost of a false positive is small and bounded: two calls under one
/// idempotency key that differ *only* in a credential replay the first
/// instead of conflicting, which is the safer of the two answers for a retry.
pub const REDACTED_FIELDS: &[&str] = &[
    "client_secret",
    "code",
    "other_password",
    "password",
    "refresh_token",
    "secret",
    "token",
    // `AddFactor.value` — the address, the password, the credential.
    "value",
];

/// What a redacted value reads as. A marker rather than a removal, so the
/// shape of the arguments still separates two different commands.
const REDACTED: &str = "[redacted]";

/// A digest of a command's arguments, for telling a replay from a reuse.
///
/// Credentials are stripped on the way in ([`REDACTED_FIELDS`]). Every writer
/// of `audit.request_digest` goes through this one function — `cmd::run` for
/// the replay lookup and each command's own audit statement for the stored
/// value — so the two cannot disagree about what was hashed.
pub fn digest_of<T: Serialize>(args: &T) -> String {
    let canonical = match serde_json::to_value(args) {
        Ok(json) => serde_json::to_vec(&redact(json)).unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let mut hasher = Sha256::new();
    hasher.update(&canonical);
    B64.encode(hasher.finalize())
}

/// Replace every credential-bearing member, at any depth.
fn redact(json: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match json {
        Value::Object(members) => Value::Object(
            members
                .into_iter()
                .map(|(name, value)| {
                    if REDACTED_FIELDS.contains(&name.as_str()) {
                        (name, Value::String(REDACTED.to_owned()))
                    } else {
                        (name, redact(value))
                    }
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(redact).collect()),
        other => other,
    }
}

/// The idempotency lookup, exposed so the "no full scan" gate can assert that
/// it rides the UNIQUE index on `audit.key` — every command runs it twice.
pub const BY_KEY_SQL: &str = "SELECT id, request_digest, payload FROM audit WHERE key = $1";

/// What an audit row says, for a caller asking about a key it has used.
#[derive(Debug, Clone, PartialEq)]
pub struct Recorded {
    /// The offset — the row's own id.
    pub offset: u64,
    /// The digest of the body that produced it.
    pub digest: String,
    /// The event, if this build understands it.
    pub event: Option<Event>,
}

impl FromRow for Recorded {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let payload = row.text("payload")?;
        Ok(Self {
            offset: row.int("id")?.max(0) as u64,
            digest: row.text("request_digest")?,
            event: Event::from_payload(&payload),
        })
    }
}

/// Look up what a key already produced.
pub async fn recorded(store: &impl Reads, key: Uuid) -> Outcome<Option<Recorded>> {
    Ok(store
        .query_opt::<Recorded>(BY_KEY_SQL, bind![key.to_string()])
        .await?)
}

/// Answer a key that has been used before.
///
/// The same body replays the original reply — that is what makes a retry after
/// a dropped response safe. A different body is a conflict, because two
/// different requests sharing one key means the caller lost track of which is
/// which, and picking either would be a guess.
pub async fn replay(store: &impl Reads, key: Uuid, digest: &str) -> Outcome<Option<Committed>> {
    let Some(row) = recorded(store, key).await? else {
        return Ok(None);
    };
    if row.digest != digest {
        return Err(KernelError::Conflict);
    }
    let event = row.event.ok_or_else(|| {
        KernelError::Invariant(format!(
            "audit row {} was written by a build this one cannot read",
            row.offset
        ))
    })?;
    Ok(Some(Committed {
        offset: row.offset,
        event,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Args {
        email: &'static str,
        display: &'static str,
    }

    #[test]
    fn a_password_never_reaches_the_digest() {
        // The addendum's test, spelled as the finding states it: the digest of
        // a SignIn with password A equals the digest with password B.
        let a = digest_of(&rn_api::commands::SignIn {
            email: "a@b.co".into(),
            password: "hunter2".into(),
        });
        let b = digest_of(&rn_api::commands::SignIn {
            email: "a@b.co".into(),
            password: "an entirely different password".into(),
        });
        assert_eq!(a, b, "the password is part of the digest input");
        // And the arguments that are not credentials still separate two calls.
        let c = digest_of(&rn_api::commands::SignIn {
            email: "somebody-else@b.co".into(),
            password: "hunter2".into(),
        });
        assert_ne!(a, c);
    }

    #[test]
    fn a_nested_credential_is_redacted_too() {
        let with = digest_of(&serde_json::json!({
            "outer": { "password": "one" },
            "list": [{ "secret": "two" }],
        }));
        let without = digest_of(&serde_json::json!({
            "outer": { "password": "three" },
            "list": [{ "secret": "four" }],
        }));
        assert_eq!(with, without);
    }

    #[test]
    fn the_digest_is_over_the_arguments_and_nothing_else() {
        let a = digest_of(&Args {
            email: "a@b.co",
            display: "A",
        });
        let b = digest_of(&Args {
            email: "a@b.co",
            display: "A",
        });
        let c = digest_of(&Args {
            email: "a@b.co",
            display: "B",
        });
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 43, "sha-256, base64url, unpadded");
    }
}
