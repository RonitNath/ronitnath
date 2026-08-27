//! The first platform operator, and the only unaudited grant in the tree.
//!
//! Platform administration is a relation — `platform:* #operator @person:R`
//! (`docs/kernel/index.html`) — and every other way of writing one is a
//! command with an actor and an audit row. This is the one that cannot be:
//! before the first operator exists there is nobody whose ruling the row could
//! be attributed to. So it is a CLI subcommand rather than a route, it runs
//! against the process's own database, and the refusal that keeps it honest
//! lives in the kernel: [`rn_kernel::bootstrap_operator`] declines the moment
//! any operator row exists, after which administration is `SetRole`.
//!
//! The email is how a human names the person, and it never leaves this
//! function: the address identifies a registration, the registration names a
//! person, and the person is what the relation row carries.

use rn_kernel::bind;
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
