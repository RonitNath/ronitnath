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
pub mod read;
pub mod restore;
pub mod route;
pub mod write;

use rn_kernel::Offset;
use rn_kernel::bind;
use rn_kernel::ids::{Id, Person};
use rn_kernel::store::{Count, Reads};
use serde_json::Value;

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
        return read::run(state, action).await;
    }
    let Some(email) = who else {
        return Err(AdminError::Usage(
            "this action writes, so it needs --as <email>: the audit row has to name somebody"
                .into(),
        ));
    };
    let actor = actor::Actor::open(state, email).await?;
    let outcome = write::run(state, &actor, action).await;
    actor.close(state).await;
    outcome
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
