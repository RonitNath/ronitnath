//! The first platform operator, and the only unaudited grant in the tree.
//!
//! Platform administration is a relation — `platform:* #operator @person:R`
//! (`docs/kernel/index.html`) — and every other way of writing one is a
//! command with an actor and an audit row. This is the one that cannot be:
//! before the first operator exists there is nobody whose ruling the row could
//! be attributed to. The refusal that keeps it honest lives in the kernel:
//! [`rn_kernel::bootstrap_operator`] declines the moment any operator row
//! exists, after which administration is `SetRole`.
//!
//! The email is how a human names the person, and it never leaves this module:
//! the address identifies a registration, the registration names a person, and
//! the person is what the relation row carries.
//!
//! Two ways in, and the second exists because the first cannot reach a formed
//! cluster:
//!
//! * **`rn-site bootstrap-operator <email>`** — [`operator`]. It opens the
//!   database directly, which is fine on a laptop and impossible on a
//!   deployment: hiqlite holds an exclusive lock on its data directory, so the
//!   subcommand cannot run beside a live voter, and taking the voter down to
//!   run it takes the deployment down.
//! * **`RN_SITE__BOOTSTRAP_OPERATOR_EMAIL`** — [`watch`]. The running process
//!   does it instead, at startup and whenever somebody registers. It is the
//!   same grant with the same kernel refusal behind it; what it adds is that
//!   it happens while the node is serving, which is the only state a cutover
//!   leaves it in.

use futures_util::StreamExt;
use rn_kernel::Offset;
use rn_kernel::bind;
use rn_kernel::event::Event;
use rn_kernel::feed::{Feed, READ_LIMIT};
use rn_kernel::ids::{Id, Person};
use rn_kernel::store::{Count, Reads, Sql, Store};

use crate::state::AppState;

/// The person an address registered. `factor.value` is unique deployment-wide
/// for `email`, so this is a seek on `factor_email_unique_idx` and one row.
const PERSON_OF_EMAIL: &str = "SELECT i.person_id AS n FROM factor f \
     JOIN identity i ON i.id = f.identity_id \
     WHERE f.kind = 'email' AND f.value = $1 AND i.person_id IS NOT NULL";

/// Why the bootstrap did not happen.
#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    /// No registration holds that address, or it has resolved to no person.
    #[error("no person is registered at {0}")]
    NoSuchPerson(String),
    /// An operator already exists, or the database refused.
    #[error("the deployment already has a platform operator")]
    Refused,
    /// The database could not be read.
    #[error("{0}")]
    Store(String),
}

/// Grant `platform:* #operator` to the person registered at `email`.
///
/// # Errors
///
/// [`BootstrapError::NoSuchPerson`] when the address names nobody, and
/// [`BootstrapError::Refused`] when this deployment already has an operator —
/// which is the kernel's refusal, not a check repeated here.
pub async fn operator(state: &AppState, email: &str) -> Result<Id<Person>, BootstrapError> {
    let person = person_of(state.store.as_ref(), email)
        .await?
        .ok_or_else(|| BootstrapError::NoSuchPerson(email.to_owned()))?;
    rn_kernel::bootstrap_operator(state.store.as_ref(), person)
        .await
        .map_err(|_| BootstrapError::Refused)?;
    Ok(person)
}

async fn person_of<S: Sql>(
    store: &Store<S>,
    email: &str,
) -> Result<Option<Id<Person>>, BootstrapError> {
    let rows = store
        .reads()
        .query::<Count>(PERSON_OF_EMAIL, bind![email])
        .await
        .map_err(|error| BootstrapError::Store(error.to_string()))?;
    Ok(rows.first().map(|row| Id::new(row.0)))
}

/// Take the configured first operator if it can be taken, and say so once.
///
/// Three things have to be true, and each of them is somebody else's answer
/// rather than a rule restated here: the deployment names an address, that
/// address has registered, and there is no operator yet. The last is the
/// kernel's — [`rn_kernel::bootstrap_operator`] refuses a second one — so a
/// deployment that keeps the variable set after the first grant does nothing
/// on every subsequent call rather than accumulating operators.
///
/// Quiet by design: the ordinary case is "the address has not registered yet",
/// which happens on every boot until somebody does, and a line about it per
/// startup is a log nobody reads. The grant itself is one line, once.
pub async fn ensure(state: &AppState) -> bool {
    let Some(email) = state.config.bootstrap_operator_email.as_deref() else {
        return false;
    };
    let email = email.trim();
    if email.is_empty() {
        return false;
    }
    match operator(state, email).await {
        Ok(person) => {
            tracing::info!(
                %email,
                "the configured address is now this deployment's platform operator"
            );
            let _ = person;
            true
        }
        // Nobody has registered it yet, or somebody already operates this
        // deployment. Both are the steady state, not a fault.
        Err(BootstrapError::NoSuchPerson(_) | BootstrapError::Refused) => false,
        Err(BootstrapError::Store(error)) => {
            tracing::warn!(%error, "the configured operator could not be checked");
            false
        }
    }
}

/// Run [`ensure`] now, and again on every registration.
///
/// A registration is the only event that can make the missing half true, and
/// the feed is where this process hears about one — including a registration
/// that happened on another voter, which is the case a startup-only check
/// would miss on a three-node cluster.
///
/// Dropping the returned handle stops the task, for the same reason
/// [`crate::observe::Lane`] is held rather than detached: a task still writing
/// while hiqlite is being handed back is what turns a restart into an
/// auto-heal boot.
#[must_use]
pub fn watch(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if state.config.bootstrap_operator_email.is_none() || ensure(&state).await {
            return;
        }
        let mut wakes = Box::pin(state.feed.subscribe());
        let mut cursor: Offset = 0;
        while let Some(offset) = wakes.next().await {
            if offset <= cursor {
                continue;
            }
            let events = match state.feed.read(cursor, READ_LIMIT).await {
                Ok(events) => events,
                Err(error) => {
                    tracing::warn!(%error, "the change feed could not be read for bootstrap");
                    continue;
                }
            };
            let mut registered = false;
            for committed in events {
                registered |= matches!(committed.event, Event::Registered { .. });
                cursor = cursor.max(committed.offset);
            }
            if registered && ensure(&state).await {
                return;
            }
        }
    })
}
