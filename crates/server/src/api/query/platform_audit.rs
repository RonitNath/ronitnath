//! The whole audit log, newest first.
//!
//! `audit.id` is the change-feed offset, so this query's key *is* the position
//! — the one result set here that is ordered by where a row sits rather than
//! by what it is about. That is also what lets the tail be live: a command
//! commits, its offset arrives on the feed, and the socket asks for exactly
//! that key.
//!
//! The payload is the typed event the command's own transaction wrote, handed
//! over as it was stored. It carries what the command decided — a disable's
//! reason, a ruling's evidence — and that is the point of an audit log: this
//! surface is the one principal the whole deployment is already visible to,
//! and an audit row that withheld the operator's own reasoning would make the
//! ruling unattributable to the only reader who needs to attribute it.
//!
//! The filters live next door (`platform_filters`), because which statement a
//! combination of controls rides is a question about indexes and this file is
//! about what an audit row *says*. What is shared is the row shape: every one
//! of the six statements returns the same columns, so [`Entry`] reads them all.

use rn_kernel::ids::{Id, IdKey, Identity};
use rn_kernel::store::{Cursor, FromRow, Reads, RowError};
use rn_kernel::{Outcome, Timestamp};
use serde_json::{Value, json};

use super::platform::{offset_key, page};
use super::platform_filters::Filter;
use super::{Params, Row};

struct Entry {
    offset: i64,
    command: String,
    actor: Option<Id<Identity>>,
    actor_display: Option<String>,
    acting_as: Option<i64>,
    acting_kind: Option<String>,
    acting_display: Option<String>,
    at: Timestamp,
    payload: String,
}

impl FromRow for Entry {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            offset: row.int("id")?,
            command: row.text("command")?,
            actor: row.id_opt("actor_identity_id")?,
            actor_display: row.text_opt("actor_display")?,
            acting_as: row.int_opt("acting_as")?,
            acting_kind: row.text_opt("acting_kind")?,
            acting_display: row.text_opt("acting_display")?,
            at: row.int("at")?,
            payload: row.text("payload")?,
        })
    }
}

pub(super) async fn list(reads: &impl Reads, params: &Params) -> Outcome<Vec<Row>> {
    let key: &IdKey = reads.ids();
    // Absent, the tail starts past the newest row there could ever be.
    let before = params
        .before
        .map_or(i64::MAX, |raw| i64::try_from(raw).unwrap_or(i64::MAX));
    let (sql, args) = Filter::of(key, params).plan(before, page(params));
    Ok(reads
        .query::<Entry>(sql, args)
        .await?
        .into_iter()
        .map(|row| Row {
            key: offset_key(row.offset.max(0) as u64),
            value: json!({
                "offset": row.offset,
                "command": row.command,
                "actor": row.actor.map(|id| id.public(key)),
                "actor_display": row.actor_display,
                "acting_as": acting_as(row.acting_as, row.acting_kind.as_deref(), key),
                "acting_display": row.acting_display,
                "at": row.at,
                // Stored as JSON text by the command's own statement; handed
                // over parsed so the panel renders a shape rather than a
                // string, and as a string if this build cannot read it.
                "payload": serde_json::from_str::<Value>(&row.payload)
                    .unwrap_or_else(|_| json!(row.payload)),
            }),
        })
        .collect())
}

/// The acting party's public id, derived under the tag its own kind carries.
///
/// The column is a `party` rowid and the four kinds share that table, so the
/// kind is read in the same statement rather than guessed: an id derived under
/// a guessed tag would decode successfully as the wrong thing.
fn acting_as(raw: Option<i64>, kind: Option<&str>, key: &IdKey) -> Value {
    match (raw, kind) {
        (Some(raw), Some(kind)) => super::platform_parties::object_id(kind, raw, key),
        _ => Value::Null,
    }
}
