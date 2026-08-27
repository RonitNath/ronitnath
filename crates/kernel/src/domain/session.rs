//! `session` — the row is the session.
//!
//! There is no status column: ending a session is deleting its row, so there
//! is no such thing as a revoked-but-present session for a query to forget to
//! filter out. A session binds an *identity*, never a person, which is why a
//! merge leaves every open tab working.
//!
//! Expiry slides, but not on the read path. Resolving a principal is a read,
//! and a read that wrote would be both the slowest thing in the request and
//! the one place the "read paths never write" rule could quietly die. The
//! renewal rides the observation lane with `last_seen`
//! ([`crate::observe`]), at most once per session per throttle window.

use crate::Timestamp;
use crate::ids::{Id, Identity, Person, Session};
use crate::store::{Cursor, FromRow, RowError};

/// How long a session lasts from its last renewal: two weeks.
pub const SESSION_TTL: i64 = 60 * 60 * 24 * 14;

/// How much of the remaining life must be gone before a renewal is worth a
/// write. At half, a session in daily use is renewed once a week.
pub const RENEW_AFTER: i64 = SESSION_TTL / 2;

/// How long an operator's session lasts: eight hours, one working day.
///
/// Chosen at `SignIn` when the person holds `platform:* #operator`, and
/// imposed on their live sessions by `GrantOperator` — a promotion that left
/// a fourteen-day cookie in place would be handing out fourteen days of god
/// mode, which is the thing the shorter TTL exists to stop.
pub const OPERATOR_SESSION_TTL: i64 = 60 * 60 * 8;

/// How long an impersonated session lasts: thirty minutes.
///
/// Not renewable and not a fraction of anything else. An operator wearing
/// somebody else's hat is doing one piece of work, and the session is sized to
/// that piece of work rather than to a working day.
pub const IMPERSONATION_TTL: i64 = 60 * 30;

/// A `session` row.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionRow {
    /// The rowid.
    pub id: Id<Session>,
    /// The registration that authenticated.
    pub identity_id: Id<Identity>,
    /// The party the principal is speaking as.
    pub acting_as: Id<Person>,
    /// When it stops resolving, unix seconds.
    pub expires_at: Timestamp,
    /// When it was created.
    pub created_at: Timestamp,
    /// When it was last observed. An observation, not an authority.
    pub last_seen_at: Timestamp,
    /// When a password was last presented on this session.
    ///
    /// Not `created_at`: `ReAuthenticate` moves this and mints no new row,
    /// which is the whole of what a re-authentication window is. Zero on a
    /// session minted before migration 7, which reads as "long ago" and
    /// refuses the sensitive commands until its holder proves themselves —
    /// the honest default, since the alternative hands every open session a
    /// free fifteen minutes.
    pub auth_time: Timestamp,
    /// The operator behind this session, when somebody is wearing it.
    ///
    /// `None` for every ordinary session. Set by `SignInAs`, and read by
    /// `refs::actor` so that the audit row names the operator as the actor and
    /// the target as the hat — one function, every command.
    pub impersonated_by_identity_id: Option<Id<Identity>>,
}

impl SessionRow {
    /// The columns this row reads.
    pub const COLUMNS: &'static str = "id, identity_id, acting_as, expires_at, created_at, \
         last_seen_at, auth_time, impersonated_by_identity_id";

    /// Whether the session is still live at `now`.
    pub const fn is_live(&self, now: Timestamp) -> bool {
        self.expires_at > now
    }

    /// Whether enough of its life has passed that renewing it is worth a
    /// write.
    pub const fn wants_renewal(&self, now: Timestamp) -> bool {
        self.expires_at - now < RENEW_AFTER
    }
}

impl FromRow for SessionRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            identity_id: row.id("identity_id")?,
            acting_as: row.id("acting_as")?,
            expires_at: row.int("expires_at")?,
            created_at: row.int("created_at")?,
            last_seen_at: row.int("last_seen_at")?,
            auth_time: row.int("auth_time")?,
            impersonated_by_identity_id: row.id_opt("impersonated_by_identity_id")?,
        })
    }
}
