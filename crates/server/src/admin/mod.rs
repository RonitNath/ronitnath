//! The way in that does not depend on the web tier — story **G21**.
//!
//! Two front doors, because they cover two different failures, and **one**
//! implementation behind them.
//!
//! * `rn-site admin <cmd>` ([`cli`]) opens a **stopped** node's store
//!   directly. That is the case where the process will not start, or where
//!   there is no operator yet to sign in as.
//! * `RN_SITE__ADMIN_ADDR` ([`route`]) opens a second, loopback-only listener
//!   serving `/admin/*` on a **running** node. That is the case the CLI cannot
//!   reach — hiqlite holds an exclusive lock on its data directory, so the
//!   subcommand and a live voter cannot both have it — and it is the one the
//!   story is actually about, because "when something is wrong" is usually
//!   "while it is still serving".
//!
//! Both parse their input into an [`Action`] and hand it to [`run`], which
//! runs the same kernel command the API runs, through the same
//! [`crate::api::cmd::invoke`] the browser's `POST /api/cmd/<name>` goes
//! through. **The CLI is a second caller, never a second authorisation path.**
//! Nothing here decides whether an action is permitted; the command does, in
//! its own transaction, against the principal it was handed — which is why
//! every action lands in the audit with an actor and why an operator relation
//! that was revoked yesterday refuses here today.
//!
//! ## Reaching the socket is the authority, and that is defensible only because it is loopback
//!
//! `/admin/*` has no cookie and no session. There is nothing to steal, nothing
//! to expire and nothing to phish, because there is no credential: the
//! authorisation *is* the fact that the caller could open a connection to
//! `127.0.0.1`. That is a real authorisation on a host where reaching loopback
//! means being on the host, and it is no authorisation at all one interface
//! further out — so [`crate::config::AppConfig::validate`] refuses a
//! non-loopback `RN_SITE__ADMIN_ADDR` at boot, in both modes, before the
//! listener exists. The routes are also absent from the public router
//! entirely, which `crates/server/tests/surface.rs` asserts for all six
//! principals: this is not a route that is guarded, it is a route the public
//! surface does not have.
//!
//! `--as <email>` is not a credential either, and it is not pretending to be.
//! It names *who is accountable*, so the audit row has an actor; it does not
//! grant anything, because the command still asks the database what that
//! person may do.
//!
//! ## The actor session
//!
//! A kernel command wants a [`Principal::Member`](rn_kernel::Principal), and a
//! `Member` carries a session — `refs::actor` reads it, and
//! `authority::fresh_enough` reads its `auth_time`. The CLI has no session, so
//! [`actor`] mints one for the duration of one command and deletes it
//! afterwards. Its `token_hash` is 32 random bytes that are the digest of
//! *nothing*: there is no token that opens it, so the row cannot be presented
//! as a cookie even in the window it exists.
//!
//! ## What is not here
//!
//! `backup` and `restore` are subcommands and are deliberately **not** routes.
//! A logical walk taken while commits are landing is consistent at no offset
//! at all, and a backup that cannot name its offset is not one
//! (`docs/ops-backup.md`, requirement F16.2).

pub mod actor;
pub mod backup;
pub mod cli;
pub mod lock;
pub mod restore;
pub mod route;

use rn_kernel::Offset;
use rn_kernel::bind;
use rn_kernel::ids::{Id, Person, Session};
use rn_kernel::store::{Count, Cursor, FromRow, Reads, RowError};
use serde_json::{Value, json};

use crate::state::AppState;

/// One thing an operator can ask for, from either front door.
///
/// It is a value rather than a function call so that the two front doors
/// cannot drift: `cli` builds one out of `argv`, `route` builds one out of a
/// request, and neither of them knows what running it involves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Who operates this deployment.
    Operators,
    /// Delegate operator authority.
    GrantOperator {
        /// An address or a person's public id.
        person: String,
        /// Why. The command requires it; this only carries it.
        reason: String,
    },
    /// Take it back.
    RevokeOperator {
        /// An address or a person's public id.
        person: String,
        /// Why.
        reason: String,
    },
    /// Live sessions, all of them or one person's.
    Sessions {
        /// An address or a person's public id.
        person: Option<String>,
    },
    /// End one, by its public id.
    RevokeSession {
        /// The session's public id.
        session: String,
    },
    /// Which products this deployment serves.
    Products,
    /// Turn one on or off at runtime.
    SetProduct {
        /// The product's slug.
        slug: String,
        /// On or off.
        enabled: bool,
    },
    /// The newest `tail` audit rows.
    Audit {
        /// How many.
        tail: usize,
    },
}

impl Action {
    /// Whether this action writes. Reads need no `--as`, because there is no
    /// row for an actor to appear on.
    #[must_use]
    pub const fn writes(&self) -> bool {
        matches!(
            self,
            Self::GrantOperator { .. }
                | Self::RevokeOperator { .. }
                | Self::RevokeSession { .. }
                | Self::SetProduct { .. }
        )
    }
}

/// What an action produced.
#[derive(Debug, Clone)]
pub struct Report {
    /// One line for a terminal, or several. The CLI prints this.
    pub text: String,
    /// The same answer, structured. The listener serves this.
    pub body: Value,
    /// Where a write landed, for a caller that wants to wait for it.
    pub offset: Option<Offset>,
}

impl Report {
    fn read(text: String, body: Value) -> Self {
        Self {
            text,
            body,
            offset: None,
        }
    }
}

/// Why an action did not happen.
///
/// Unlike the API's uniform decline, these are *loud*: the audience is an
/// operator at a terminal who has already proved they are on the host, and
/// telling them nothing would leave them retrying. The API's reticence exists
/// to stop a stranger enumerating the deployment, and there is no stranger
/// here.
#[derive(Debug, thiserror::Error)]
pub enum AdminError {
    /// The argument vector did not make sense.
    #[error("{0}")]
    Usage(String),
    /// A running node holds the data directory.
    #[error(
        "this node is running as pid {pid} and hiqlite holds an exclusive lock on {dir}. \
         Stop it first, or use the loopback admin listener (RN_SITE__ADMIN_ADDR)"
    )]
    Locked {
        /// The process holding it.
        pid: u32,
        /// The data directory it holds.
        dir: String,
    },
    /// No person is registered at that address, or that public id names none.
    #[error("no person is registered at {0}, and it is not a person's public id")]
    NoSuchPerson(String),
    /// The identity behind `--as` cannot hold a session.
    #[error("{0}")]
    NoActor(String),
    /// The kernel refused, or could not be reached.
    #[error("{0}")]
    Command(String),
    /// The database could not be read or written.
    #[error("{0}")]
    Store(String),
    /// The action is real and this build cannot do it yet.
    #[error("{0}")]
    NotYet(String),
    /// The action would have done something this deployment forbids.
    #[error("{0}")]
    Refused(String),
}

impl AdminError {
    fn store(error: impl std::fmt::Display) -> Self {
        Self::Store(error.to_string())
    }
}

/// Run an action as `who`, through the same command path the API uses.
///
/// `who` is the `--as` address; it is required for anything that writes,
/// because a write with no actor is a row nobody can account for, and ignored
/// for anything that reads.
///
/// # Errors
///
/// Every variant of [`AdminError`]. A kernel refusal arrives as
/// [`AdminError::Command`] carrying what the kernel said.
pub async fn run(
    state: &AppState,
    who: Option<&str>,
    action: &Action,
) -> Result<Report, AdminError> {
    if !action.writes() {
        return read(state, action).await;
    }
    let Some(email) = who else {
        return Err(AdminError::Usage(
            "this action writes, so it needs --as <email>: the audit row has to name somebody"
                .into(),
        ));
    };
    let actor = actor::Actor::open(state, email).await?;
    let outcome = write(state, &actor, action).await;
    actor.close(state).await;
    outcome
}

// ------------------------------------------------------------------ reads ---

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

async fn read(state: &AppState, action: &Action) -> Result<Report, AdminError> {
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
                    let who = person_of(state, needle).await?;
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

// ----------------------------------------------------------------- writes ---

async fn write(
    state: &AppState,
    actor: &actor::Actor,
    action: &Action,
) -> Result<Report, AdminError> {
    let key = state.ids();
    match action {
        Action::GrantOperator { person, reason } => {
            let target = person_of(state, person).await?;
            let args = rn_api::commands::GrantOperator {
                person: target.public(key),
                reason: reason.clone(),
            };
            let executed = invoke(state, actor, &args).await?;
            Ok(Report {
                text: format!("{person} now holds platform:* #operator\n"),
                body: json!({ "granted": args.person.as_str(), "offset": executed }),
                offset: Some(executed),
            })
        }
        Action::RevokeOperator { person, reason } => {
            let target = person_of(state, person).await?;
            let args = rn_api::commands::RevokeOperator {
                person: target.public(key),
                reason: reason.clone(),
            };
            let executed = invoke(state, actor, &args).await?;
            Ok(Report {
                text: format!("{person} no longer holds platform:* #operator\n"),
                body: json!({ "revoked": args.person.as_str(), "offset": executed }),
                offset: Some(executed),
            })
        }
        Action::RevokeSession { session } => {
            let id = session.parse().map_err(|_| {
                AdminError::Usage(format!("{session} is not a session's public id"))
            })?;
            let args = rn_api::commands::RevokeSession { session: id };
            let executed = invoke(state, actor, &args).await?;
            Ok(Report {
                text: format!("{session} is ended\n"),
                body: json!({ "revoked": session, "offset": executed }),
                offset: Some(executed),
            })
        }
        Action::SetProduct { slug, .. } => Err(AdminError::NotYet(format!(
            "there is no command that turns {slug} on or off in this build: requirement B5's \
             product commands are leg P2's and have not landed on this branch"
        ))),
        Action::Operators | Action::Sessions { .. } | Action::Products | Action::Audit { .. } => {
            unreachable!("reads do not reach the write path")
        }
    }
}

/// Run one command as the actor, through the API's own dispatch.
async fn invoke<A: rn_api::Command + serde::Serialize>(
    state: &AppState,
    actor: &actor::Actor,
    args: &A,
) -> Result<Offset, AdminError> {
    crate::api::cmd::invoke(state, actor.principal.clone(), args)
        .await
        .map(|executed| executed.committed.offset)
        .map_err(|error| AdminError::Command(describe(&error)))
}

/// What a command refusal says to somebody at a terminal.
fn describe(error: &crate::api::cmd::CommandError) -> String {
    use crate::api::cmd::CommandError;
    use rn_kernel::KernelError;
    match error {
        CommandError::Kernel(KernelError::Decline(_)) => {
            "the kernel declined it. The commonest reason from here is that --as does not hold \
             platform:* #operator; the others are that the target is not what the command \
             expects, or that the last operator cannot be revoked"
                .to_owned()
        }
        CommandError::Kernel(KernelError::ReAuthRequired) => {
            "the actor session was not fresh enough, which cannot happen from here — report it"
                .to_owned()
        }
        CommandError::Kernel(other) => other.to_string(),
        CommandError::Unbound => "no such command in this build".to_owned(),
        CommandError::Malformed(what) => format!("malformed arguments: {what}"),
    }
}

/// The person an address or a public id names.
///
/// Both are accepted because both are how somebody names a human here: an
/// address is what an operator at a terminal has, and a public id is what a
/// screen or a previous line of output gave them.
pub(crate) async fn person_of(state: &AppState, needle: &str) -> Result<Id<Person>, AdminError> {
    let needle = needle.trim();
    if let Ok(person) = rn_kernel::ids::parse::<Person>(state.ids(), needle) {
        return Ok(person);
    }
    let rows: Vec<Count> = state
        .store
        .reads()
        .query(
            "SELECT i.person_id AS n FROM factor f JOIN identity i ON i.id = f.identity_id \
             WHERE f.kind = 'email' AND f.value = $1 AND i.person_id IS NOT NULL",
            bind![needle],
        )
        .await
        .map_err(AdminError::store)?;
    rows.first()
        .map(|row| Id::new(row.0))
        .ok_or_else(|| AdminError::NoSuchPerson(needle.to_owned()))
}
