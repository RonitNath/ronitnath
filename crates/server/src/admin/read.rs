//! The four questions an operator asks a deployment that will not answer any
//! other way: who operates it, who is signed in, what it serves, and what has
//! happened.
//!
//! Every one of these is a read and none of them is a command, so none of them
//! takes `--as`: there is no row for an actor to appear on. They are the
//! CLI's own statements rather than the query handlers' — those are
//! `crates/server/src/api/query`, they answer a browser, and they are keyed on
//! a principal this front door does not have. What is shared is the database,
//! which is the right thing to share.

use rn_kernel::bind;
use rn_kernel::ids::{Id, Person, Session};
use rn_kernel::store::{Count, Cursor, FromRow, Reads, RowError};
use serde_json::{Value, json};

use super::{Action, AdminError, Report};
use crate::state::AppState;

/// Every live session, newest first, with the person behind it.
const SESSIONS_SQL: &str = "SELECT s.id, s.identity_id, s.acting_as, s.expires_at, \
     s.impersonated_by_identity_id, p.display_name AS who \
     FROM session s JOIN identity i ON i.id = s.identity_id \
     LEFT JOIN party p ON p.id = i.person_id \
     WHERE s.expires_at > $1 ORDER BY s.id DESC LIMIT 200";

/// The same, for one person.
const SESSIONS_OF_SQL: &str = "SELECT s.id, s.identity_id, s.acting_as, s.expires_at, \
     s.impersonated_by_identity_id, p.display_name AS who \
     FROM session s JOIN identity i ON i.id = s.identity_id \
     LEFT JOIN party p ON p.id = i.person_id \
     WHERE s.expires_at > $1 AND i.person_id = $2 ORDER BY s.id DESC LIMIT 200";

/// Who holds `platform:* #operator`, and who granted it.
const OPERATORS_SQL: &str = "SELECT r.subject_id AS person, r.at, p.display_name AS who, \
     r.granted_by \
     FROM relation r JOIN party p ON p.id = r.subject_id \
     WHERE r.object_kind = 'platform' AND r.object_id = 0 AND r.relation = 'operator' \
       AND r.subject_kind = 'person' ORDER BY r.at ASC";

/// The newest rows of the deployment's whole history.
const AUDIT_SQL: &str = "SELECT id, command, actor_identity_id, acting_as, at, payload \
     FROM audit ORDER BY id DESC LIMIT $1";

struct SessionRow {
    id: i64,
    identity: i64,
    acting_as: i64,
    expires_at: i64,
    impersonated_by: Option<i64>,
    who: Option<String>,
}

impl FromRow for SessionRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.int("id")?,
            identity: row.int("identity_id")?,
            acting_as: row.int("acting_as")?,
            expires_at: row.int("expires_at")?,
            impersonated_by: row.int_opt("impersonated_by_identity_id")?,
            who: row.text_opt("who")?,
        })
    }
}

struct OperatorRow {
    person: i64,
    at: i64,
    who: String,
    granted_by: Option<i64>,
}

impl FromRow for OperatorRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            person: row.int("person")?,
            at: row.int("at")?,
            who: row.text("who")?,
            granted_by: row.int_opt("granted_by")?,
        })
    }
}

struct AuditRow {
    id: i64,
    command: String,
    actor: Option<i64>,
    acting_as: Option<i64>,
    at: i64,
    payload: String,
}

impl FromRow for AuditRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.int("id")?,
            command: row.text("command")?,
            actor: row.int_opt("actor_identity_id")?,
            acting_as: row.int_opt("acting_as")?,
            at: row.int("at")?,
            payload: row.text("payload")?,
        })
    }
}

/// Answer one of the four read actions.
///
/// # Errors
///
/// [`AdminError::Store`] when the database refuses, and [`AdminError::NotYet`]
/// for `products` on a build with no product model.
pub async fn run(state: &AppState, action: &Action) -> Result<Report, AdminError> {
    let reads = state.store.reads();
    let key = state.ids();
    match action {
        Action::Operators => {
            let rows: Vec<OperatorRow> = reads
                .query(OPERATORS_SQL, bind![])
                .await
                .map_err(AdminError::store)?;
            let mut text = String::new();
            let mut body = Vec::new();
            for row in &rows {
                let id = Id::<Person>::new(row.person).public(key);
                text.push_str(&format!(
                    "{id}  {:<24} since {}{}\n",
                    row.who,
                    row.at,
                    if row.granted_by.is_some() {
                        ""
                    } else {
                        "  (bootstrap)"
                    }
                ));
                body.push(json!({
                    "person": id.as_str(),
                    "display_name": row.who,
                    "since": row.at,
                    "bootstrap": row.granted_by.is_none(),
                }));
            }
            if rows.is_empty() {
                text.push_str("no operators — this deployment has nobody who can grant one\n");
            }
            Ok(Report::read(text, json!({ "operators": body })))
        }
        Action::Sessions { person } => {
            let now = reads.now();
            let rows: Vec<SessionRow> = match person {
                None => reads.query(SESSIONS_SQL, bind![now]).await,
                Some(needle) => {
                    let who = super::person_of(state, needle).await?;
                    reads.query(SESSIONS_OF_SQL, bind![now, who]).await
                }
            }
            .map_err(AdminError::store)?;
            let mut text = String::new();
            let mut body = Vec::new();
            for row in &rows {
                let id = Id::<Session>::new(row.id).public(key);
                let hat = row.impersonated_by.map_or("", |_| "  impersonated");
                text.push_str(&format!(
                    "{id}  {:<24} expires {}{hat}\n",
                    row.who.as_deref().unwrap_or("(unresolved)"),
                    row.expires_at
                ));
                body.push(json!({
                    "session": id.as_str(),
                    "display_name": row.who,
                    "identity": row.identity,
                    "acting_as": Id::<Person>::new(row.acting_as).public(key).as_str(),
                    "expires_at": row.expires_at,
                    "impersonated": row.impersonated_by.is_some(),
                }));
            }
            if rows.is_empty() {
                text.push_str("no live sessions\n");
            }
            Ok(Report::read(text, json!({ "sessions": body })))
        }
        Action::Audit { tail } => {
            let limit = i64::try_from(*tail).unwrap_or(i64::MAX).clamp(1, 1_000);
            let rows: Vec<AuditRow> = reads
                .query(AUDIT_SQL, bind![limit])
                .await
                .map_err(AdminError::store)?;
            let mut text = String::new();
            let mut body = Vec::new();
            for row in rows.iter().rev() {
                text.push_str(&format!(
                    "{:>8}  {:<22} actor {:<6} at {}  {}\n",
                    row.id,
                    row.command,
                    row.actor.map_or_else(|| "-".to_owned(), |a| a.to_string()),
                    row.at,
                    row.payload
                ));
                body.push(json!({
                    "offset": row.id,
                    "command": row.command,
                    "actor_identity": row.actor,
                    "acting_as": row.acting_as.map(|p| Id::<Person>::new(p).public(key).as_str().to_owned()),
                    "at": row.at,
                    "payload": serde_json::from_str::<Value>(&row.payload).unwrap_or(Value::Null),
                }));
            }
            Ok(Report::read(text, json!({ "audit": body })))
        }
        Action::Products => products(state).await,
        Action::GrantOperator { .. }
        | Action::RevokeOperator { .. }
        | Action::RevokeSession { .. }
        | Action::SetProduct { .. } => unreachable!("writes do not reach the read path"),
    }
}

/// Which products this deployment serves.
///
/// The product model is leg P2's (requirement B5, migration 8) and is not on
/// this branch. Rather than invent a second answer to a question P2 is about
/// to answer for real, this reads the table if it is there and says plainly
/// that it is not if it is not — a subcommand that silently reported an empty
/// list would be indistinguishable from a deployment with every product off.
async fn products(state: &AppState) -> Result<Report, AdminError> {
    if !has_table(state, "product").await? {
        return Err(AdminError::NotYet(
            "this build has no product model: requirement B5 and migration 8 are leg P2's, and \
             they have not landed on this branch"
                .into(),
        ));
    }
    let rows: Vec<ProductRow> = state
        .store
        .reads()
        .query(
            "SELECT slug, enabled FROM product ORDER BY slug ASC",
            bind![],
        )
        .await
        .map_err(AdminError::store)?;
    let mut text = String::new();
    let mut body = Vec::new();
    for row in &rows {
        text.push_str(&format!(
            "{:<20} {}\n",
            row.slug,
            if row.enabled == 0 {
                "disabled"
            } else {
                "enabled"
            }
        ));
        body.push(json!({ "slug": row.slug, "enabled": row.enabled != 0 }));
    }
    Ok(Report::read(text, json!({ "products": body })))
}

struct ProductRow {
    slug: String,
    enabled: i64,
}

impl FromRow for ProductRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            slug: row.text("slug")?,
            enabled: row.int("enabled")?,
        })
    }
}

/// Whether the schema has a table by that name.
pub(crate) async fn has_table(state: &AppState, name: &str) -> Result<bool, AdminError> {
    let rows: Vec<Count> = state
        .store
        .reads()
        .query(
            "SELECT count(*) AS n FROM sqlite_master WHERE type = 'table' AND name = $1",
            bind![name],
        )
        .await
        .map_err(AdminError::store)?;
    Ok(rows.first().is_some_and(|row| row.0 > 0))
}
