//! The lost-account path: a person acting on an identity that is not the one
//! they signed in with.
//!
//! This is the second reason merge exists. Somebody registered twice, lost the
//! password to the first registration and the mailbox behind it, and proved to
//! an operator that both are theirs. After the ruling the two identities are
//! one person — and the point of that is that the person can now sign in with
//! the registration they still hold and *rotate the factors of the one they
//! lost*: add a new address, remove the dead one, set a new password.
//!
//! So `AddFactor` and `RemoveFactor` take an optional identity, and the
//! authorisation for it is person-level ownership: the identity must resolve
//! to the party the caller is acting as. Omitted, the target is the acting
//! identity, which is exactly what those commands did before — the recovery
//! path is an addition to them, not a change of meaning.
//!
//! What it is not is a way to reach somebody else's registration. A merge is
//! never silent (a candidate, then a proof), it is reversible
//! ([`split`](super::split)), and the `person_link` row that made the two one
//! person carries the evidence — so the audit trail of a rotation reaches back
//! to the ruling that permitted it.

use rn_api::ids::PublicId;

use crate::bind;
use crate::error::{Outcome, decline};
use crate::ids::{self, Id, Identity, Person};
use crate::store::{Count, Reads};

/// Whether an identity resolves to a party.
pub const OWNED_SQL: &str = "SELECT count(*) AS n FROM identity WHERE id = $1 AND person_id = $2";

/// Whether this person holds this identity.
pub async fn owned_identity(
    store: &impl Reads,
    person: Id<Person>,
    identity: Id<Identity>,
) -> Outcome<bool> {
    let rows = store
        .query::<Count>(OWNED_SQL, bind![identity, person])
        .await?;
    Ok(rows.first().is_some_and(|row| row.0 > 0))
}

/// The identity a factor command should act on.
///
/// `None` is the acting identity. A named identity is accepted only when the
/// acting party holds it, which after a merge is every identity of the person
/// and before one is only their own.
pub async fn target_identity(
    store: &impl Reads,
    acting: Id<Identity>,
    person: Id<Person>,
    requested: Option<&PublicId>,
) -> Outcome<Id<Identity>> {
    let Some(requested) = requested else {
        return Ok(acting);
    };
    let Ok(target) = ids::decode::<Identity>(store.ids(), requested) else {
        return decline();
    };
    if target == acting {
        return Ok(target);
    }
    if owned_identity(store, person, target).await? {
        Ok(target)
    } else {
        decline()
    }
}
