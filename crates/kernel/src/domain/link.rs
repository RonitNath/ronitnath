//! `link` — a bearer secret with an expiry.
//!
//! A link is claimed, once, by whoever holds the token. What claiming an
//! invitation *grants* is a relation row (`group:X #member @link:T`). The
//! other purpose a link is minted for is proving an email address, and that
//! one has no relation to express it, so it stays the `verifies_factor_id`
//! column.

use crate::Timestamp;
use crate::ids::{Factor, Id, Identity, Link};
use crate::store::{Cursor, FromRow, RowError};

/// How long an email verification link lasts: a day.
pub const VERIFY_TTL: i64 = 60 * 60 * 24;

/// A `link` row.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkRow {
    /// The rowid.
    pub id: Id<Link>,
    /// When it stops working, unix seconds.
    pub expires_at: Timestamp,
    /// Who claimed it, if anyone. A link is claimed once.
    pub claimed_by_identity_id: Option<Id<Identity>>,
    /// The factor it proves, for this leg's only link purpose.
    pub verifies_factor_id: Option<Id<Factor>>,
    /// When the party behind it was disabled, if it was.
    ///
    /// Not a status and not a delete. `Disable` stamps it, `ClaimLink` refuses
    /// a stamped one, `Enable` clears it — which is the whole of finding F3's
    /// fix, and the reason it is a nullable timestamp rather than a third
    /// state is that `RevokeLink` already means *gone for good*.
    pub suspended_at: Option<Timestamp>,
}

impl LinkRow {
    /// The columns this row reads.
    pub const COLUMNS: &'static str =
        "id, expires_at, claimed_by_identity_id, verifies_factor_id, suspended_at";

    /// Whether the link may still be claimed at `now`.
    pub const fn is_claimable(&self, now: Timestamp) -> bool {
        self.expires_at > now
            && self.claimed_by_identity_id.is_none()
            && self.suspended_at.is_none()
    }
}

impl FromRow for LinkRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            expires_at: row.int("expires_at")?,
            claimed_by_identity_id: row.id_opt("claimed_by_identity_id")?,
            verifies_factor_id: row.id_opt("verifies_factor_id")?,
            suspended_at: row.int_opt("suspended_at")?,
        })
    }
}
