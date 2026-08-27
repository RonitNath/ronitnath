//! What disabling a party ends, counted before it happens and recorded after.
//!
//! Requirement C9.3, and the reason it is a module rather than four `SELECT
//! count(*)`s inside `Disable`: the *same* four numbers are what an operator
//! is shown on the confirm control before they decide, and what the audit row
//! carries afterwards. Two spellings of the same question would diverge the
//! first time somebody added a fifth thing to the cascade.
//!
//! The two readings will not always agree, and the requirement says which one
//! is true. The preview said *at the time of asking*; the audit row is the
//! record of what actually went. A session minted between the two is in the
//! second number and not the first, and that is not an error in either — it is
//! the honest difference between a question and an act.
//!
//! ## What a disable ends, and what it only suspends
//!
//! Sessions, tokens and codes are **deleted**. That is right: they are
//! re-mintable by signing in, so `Enable` does not have to put them back and
//! could not sensibly try.
//!
//! Links are **suspended**, not deleted, and that is finding **F3**. Before
//! this, a disabled person's outstanding invitations kept working — nothing in
//! `Disable` touched `link` or the `group:X #member @link:T` rows that say
//! what a token is worth. Deleting them would have been the easy fix and the
//! wrong one: `Enable` has to put the party back the way it found them, and
//! `RevokeLink` already means *gone for good*. A link somebody deliberately
//! withdrew and a link that is waiting for its minter are different facts.
//!
//! Consents are counted and left alone. Withdrawing them is `RevokeConsent`,
//! which is the person's own decision, and a disable that quietly revoked
//! every consent would be an operator making it for them — while the tokens
//! those consents produced are gone anyway, which is the part that had effect.

use crate::bind;
use crate::error::Outcome;
use crate::ids::{Id, Person};
use crate::store::{Cursor, FromRow, Reads, RowError};

/// The four numbers a disable is about.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    /// Live sessions across every identity resolved onto the party. Deleted.
    pub sessions: i64,
    /// Unexpired, unrevoked OpenID tokens. Deleted.
    pub tokens: i64,
    /// Unclaimed, unsuspended invitations the party minted or that grant into
    /// it. Suspended, and restored by `Enable`.
    pub links: i64,
    /// Consents the person has given relying parties. Counted and untouched —
    /// withdrawing one is theirs to do.
    pub consents: i64,
}

impl FromRow for Counts {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            sessions: row.int("sessions")?,
            tokens: row.int("tokens")?,
            links: row.int("links")?,
            consents: row.int("consents")?,
        })
    }
}

impl Counts {
    /// The payload member `Disable` writes and `about-me` reads back.
    #[must_use]
    pub fn as_json(&self) -> String {
        format!(
            r#"{{"sessions":{},"tokens":{},"links":{},"consents":{}}}"#,
            self.sessions, self.tokens, self.links, self.consents
        )
    }
}

/// The four counts in one statement.
///
/// Four correlated subqueries rather than four round trips, because the
/// preview runs on a confirm control that a person is waiting on and the
/// command runs one more time inside a transaction's read phase. Every branch
/// is a seek: `session_identity_idx` through `identity_person_idx`,
/// `oidc_token_person_client_idx`, `relation_granted_by_idx` and
/// `relation_object_idx` for the halves of the link question, and
/// `oidc_consent_person_idx`.
///
/// The link question is spelled as three `UNION` branches rather than one with
/// `object_kind IN (…)`, because an `IN` over the leading column of
/// `relation_object_idx` is not a prefix constraint and the planner drops to
/// `relation_subject_idx (subject_kind = 'link')` — which is a seek that walks
/// every invitation in the deployment. Three seeks are cheaper than one walk,
/// and `explain` asserts which one this is.
///
/// The `CROSS JOIN` on the first branch is the same pin `relation::CHECK_SQL`
/// uses and for the same reason: left to itself the planner drives from
/// `relation` on `subject_kind = 'link'`, which is one pass over every
/// invitation the deployment has ever minted to find the handful one person
/// minted. Pinned this way it is one seek per identity of that person.
///
/// `$1` is the party and `$2` is now, in that order of first appearance —
/// SQLite assigns `$N` the index it is first seen at, not the number.
pub const COUNTS_SQL: &str = "SELECT \
     (SELECT count(*) FROM session s \
       WHERE s.identity_id IN (SELECT id FROM identity WHERE person_id = $1)) AS sessions, \
     (SELECT count(*) FROM oidc_token t \
       WHERE t.person_id = $1 AND t.revoked_at IS NULL AND t.expires_at > $2) AS tokens, \
     (SELECT count(*) FROM link l \
       WHERE l.claimed_by_identity_id IS NULL AND l.suspended_at IS NULL \
         AND l.expires_at > $2 \
         AND l.id IN (SELECT rel.subject_id FROM identity i \
                       CROSS JOIN relation rel ON rel.granted_by = i.id \
                       WHERE i.person_id = $1 AND rel.subject_kind = 'link' \
                      UNION \
                      SELECT rel.subject_id FROM relation rel \
                       WHERE rel.object_kind = 'group' AND rel.object_id = $1 \
                         AND rel.subject_kind = 'link' \
                      UNION \
                      SELECT rel.subject_id FROM relation rel \
                       WHERE rel.object_kind = 'organization' AND rel.object_id = $1 \
                         AND rel.subject_kind = 'link')) AS links, \
     (SELECT count(*) FROM oidc_consent c WHERE c.person_id = $1) AS consents";

/// What disabling this party would end.
pub async fn counts(store: &impl Reads, party: Id<Person>) -> Outcome<Counts> {
    let now = store.now();
    Ok(store
        .query::<Counts>(COUNTS_SQL, bind![party, now])
        .await?
        .pop()
        .unwrap_or_default())
}

/// Put every unclaimed invitation the party is behind to sleep.
///
/// Two halves, and both are the same fact from different ends. An invitation a
/// *person* minted stops working because they are disabled — that is the
/// finding. An invitation into a disabled *organization or group* stops
/// working because there is nothing left to join; a container that cannot be
/// used is not one somebody should still be accepting a seat in.
///
/// The stamp is idempotent by its own guard, so a party disabled twice
/// suspends nothing the second time and `Enable` restores exactly the set the
/// disable took.
pub const SUSPEND_LINKS_SQL: &str = "UPDATE link SET suspended_at = $1 \
     WHERE claimed_by_identity_id IS NULL AND suspended_at IS NULL \
       AND id IN (SELECT rel.subject_id FROM identity i \
                   CROSS JOIN relation rel ON rel.granted_by = i.id \
                   WHERE i.person_id = $2 AND rel.subject_kind = 'link' \
                  UNION \
                  SELECT rel.subject_id FROM relation rel \
                   WHERE rel.object_kind = 'group' AND rel.object_id = $2 \
                     AND rel.subject_kind = 'link' \
                  UNION \
                  SELECT rel.subject_id FROM relation rel \
                   WHERE rel.object_kind = 'organization' AND rel.object_id = $2 \
                     AND rel.subject_kind = 'link')";

/// And wake them up again.
///
/// The mirror of [`SUSPEND_LINKS_SQL`], clearing only rows that are actually
/// asleep — so an `Enable` cannot resurrect a link that expired in the
/// meantime, and cannot touch one a second party's disable also stamped, since
/// that one is still suspended and the guard finds it either way. What it
/// cannot do is bring back a link `RevokeLink` deleted, which is the whole
/// reason this is a column and not a delete.
pub const RESUME_LINKS_SQL: &str = "UPDATE link SET suspended_at = NULL \
     WHERE suspended_at IS NOT NULL AND claimed_by_identity_id IS NULL \
       AND id IN (SELECT rel.subject_id FROM identity i \
                   CROSS JOIN relation rel ON rel.granted_by = i.id \
                   WHERE i.person_id = $1 AND rel.subject_kind = 'link' \
                  UNION \
                  SELECT rel.subject_id FROM relation rel \
                   WHERE rel.object_kind = 'group' AND rel.object_id = $1 \
                     AND rel.subject_kind = 'link' \
                  UNION \
                  SELECT rel.subject_id FROM relation rel \
                   WHERE rel.object_kind = 'organization' AND rel.object_id = $1 \
                     AND rel.subject_kind = 'link')";
