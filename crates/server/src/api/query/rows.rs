//! The three result sets this leg serves, and the statements behind them.
//!
//! One function per query, each handed a [`Reads`] and nothing else. The
//! shapes are deliberately narrow: a device list says which row is the browser
//! reading it, an identity says which factors it holds and which are proven,
//! and neither says what any of them *is* — a factor's `value` is an address
//! or an argon2 PHC string and never leaves the process
//! (`kernel::domain::factor`).

use rn_kernel::bind;
use rn_kernel::ids::{Factor, Id, IdKey, Identity, Session};
use rn_kernel::store::{Cursor, FromRow, Reads, RowError};
use rn_kernel::{Outcome, Principal};
use serde_json::json;

use super::{Params, Row};

const SESSIONS: &str = "SELECT id, expires_at, created_at, last_seen_at FROM session \
                        WHERE identity_id = $1 ORDER BY created_at DESC";

const MY_IDENTITY: &str = "SELECT id, source, status, created_at FROM identity WHERE id = $1";

const OUR_IDENTITIES: &str = "SELECT id, source, status, created_at FROM identity \
                              WHERE person_id = $1 ORDER BY created_at";

const FACTORS: &str = "SELECT id, kind, verified_at FROM factor \
                       WHERE identity_id = $1 ORDER BY id";

const AUDIT: &str = "SELECT id, command, at FROM audit \
                     WHERE actor_identity_id = $1 AND id > $2 ORDER BY id LIMIT $3";

struct SessionRow {
    id: Id<Session>,
    expires_at: i64,
    created_at: i64,
    last_seen_at: i64,
}

impl FromRow for SessionRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            expires_at: row.int("expires_at")?,
            created_at: row.int("created_at")?,
            last_seen_at: row.int("last_seen_at")?,
        })
    }
}

pub(super) async fn sessions(
    reads: &impl Reads,
    identity: Id<Identity>,
    principal: &Principal,
    key: &IdKey,
) -> Outcome<Vec<Row>> {
    let current = principal.session();
    Ok(reads
        .query::<SessionRow>(SESSIONS, bind![identity])
        .await?
        .into_iter()
        .map(|row| {
            let public_id = row.id.public(key);
            Row {
                key: public_id.as_str().to_owned(),
                value: json!({
                    "public_id": public_id,
                    "created_at": row.created_at,
                    "expires_at": row.expires_at,
                    "last_seen_at": row.last_seen_at,
                    // The one thing a device list has to say: which row is the
                    // browser reading it, so revoking it reads as signing out.
                    "current": current == Some(row.id),
                }),
            }
        })
        .collect())
}

struct IdentitySummary {
    id: Id<Identity>,
    source: String,
    status: String,
    created_at: i64,
}

impl FromRow for IdentitySummary {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            source: row.text("source")?,
            status: row.text("status")?,
            created_at: row.int("created_at")?,
        })
    }
}

struct FactorSummary {
    id: Id<Factor>,
    kind: String,
    verified_at: Option<i64>,
}

impl FromRow for FactorSummary {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            kind: row.text("kind")?,
            verified_at: row.int_opt("verified_at")?,
        })
    }
}

pub(super) async fn identities(
    reads: &impl Reads,
    identity: Id<Identity>,
    principal: &Principal,
    key: &IdKey,
) -> Outcome<Vec<Row>> {
    // Unresolved identities have no person to gather siblings by, so the set
    // is the one registration that authenticated.
    let summaries = match person_of(principal) {
        Some(person) => {
            reads
                .query::<IdentitySummary>(OUR_IDENTITIES, bind![person])
                .await?
        }
        None => {
            reads
                .query::<IdentitySummary>(MY_IDENTITY, bind![identity])
                .await?
        }
    };

    let mut rows = Vec::with_capacity(summaries.len());
    for summary in summaries {
        let factors = reads
            .query::<FactorSummary>(FACTORS, bind![summary.id])
            .await?;
        let public_id = summary.id.public(key);
        rows.push(Row {
            key: public_id.as_str().to_owned(),
            value: json!({
                "public_id": public_id,
                "source": summary.source,
                "status": summary.status,
                "created_at": summary.created_at,
                "current": summary.id == identity,
                // A factor's `value` is its secret half — an argon2 PHC string
                // or an address — and never leaves the process
                // (`kernel::domain::factor`). What a person needs from this
                // list is which kinds they hold and which are proven.
                "factors": factors.iter().map(|factor| json!({
                    "public_id": factor.id.public(key),
                    "kind": factor.kind,
                    "verified": factor.verified_at.is_some(),
                })).collect::<Vec<_>>(),
            }),
        });
    }
    Ok(rows)
}

fn person_of(principal: &Principal) -> Option<Id<rn_kernel::ids::Person>> {
    match principal {
        Principal::Member { person, .. } => *person,
        _ => None,
    }
}

struct AuditRow {
    offset: i64,
    command: String,
    at: i64,
}

impl FromRow for AuditRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            offset: row.int("id")?,
            command: row.text("command")?,
            at: row.int("at")?,
        })
    }
}

pub(super) async fn audit(
    reads: &impl Reads,
    identity: Id<Identity>,
    params: &Params,
) -> Outcome<Vec<Row>> {
    let after = i64::try_from(params.after).unwrap_or(i64::MAX);
    Ok(reads
        .query::<AuditRow>(AUDIT, bind![identity, after, params.page()])
        .await?
        .into_iter()
        .map(|row| Row {
            // Zero-padded so the client's key-ordered map reads chronologically
            // rather than lexicographically wrong at every power of ten.
            key: format!("{:020}", row.offset.max(0)),
            value: json!({
                "offset": row.offset,
                "command": row.command,
                "at": row.at,
            }),
        })
        .collect())
}
