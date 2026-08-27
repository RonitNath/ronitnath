//! Signals: what the database already knows that might mean two registrations
//! are one human.
//!
//! Every function here reads, decides nothing, and writes at most
//! `match_candidate` rows in `proposed`. That is the whole contract, and it is
//! the reason the scan is allowed to run on the observation lane: a proposal
//! authorises nobody, so losing one costs a question that the next
//! registration or verification asks again, and duplicating one hits the
//! UNIQUE index on the pair.
//!
//! The signals, strongest first:
//!
//! | signal | what it reads | reachable today |
//! |---|---|---|
//! | `oidc_subject` | the same subject verified at the same issuer | when OIDC lands |
//! | `verified_phone` | the same number, verified on both | when `phone` is a factor kind |
//! | `verified_email` | the same address, verified on both | see below |
//! | `claimed_link` | one identity claimed a link the other minted | yes |
//! | `name_and_group` | the same display name in a shared group | yes |
//!
//! `verified_email` is written against the schema rather than against today's
//! data: migration 1 puts a UNIQUE index on `factor(value) WHERE kind =
//! 'email'`, so two identities *cannot* currently hold the same address, and
//! the signal fires only once that index becomes source-scoped — which is what
//! "identity is source-scoped" in the report implies and what an import from
//! an acquired company needs. The query is the same one either way, which is
//! why it is here and tested through the `oidc` kind that the index does not
//! cover.

use rn_api::commands::MatchSignal;

use super::candidate::{PROPOSE_QUIETLY, proposal};
use crate::bind;
use crate::error::Outcome;
use crate::ids::{Id, Identity};
use crate::store::{Cursor, FromRow, Reads, RowError, Sql, Store};

/// Two identities that have proven the same value.
pub const SHARED_FACTOR_SQL: &str = "SELECT b.identity_id AS identity_id, a.kind AS kind \
     FROM factor a \
     JOIN factor b ON b.value = a.value AND b.kind = a.kind AND b.identity_id <> a.identity_id \
     WHERE a.identity_id = $1 AND a.verified_at IS NOT NULL AND b.verified_at IS NOT NULL \
       AND a.kind IN ('email', 'oidc', 'phone')";

/// An identity that claimed a link another identity's factor minted.
pub const CLAIMED_LINK_SQL: &str = "SELECT f.identity_id AS identity_id, 'link' AS kind \
     FROM link l JOIN factor f ON f.id = l.verifies_factor_id \
     WHERE l.claimed_by_identity_id = $1 AND f.identity_id <> $1";

/// The same display name, in a group they both belong to.
///
/// The weakest signal in the table and the only one that is about a *person*
/// rather than a proof, which is why it scores 0.2 and why no amount of it
/// merges anybody.
pub const NAME_AND_GROUP_SQL: &str = "SELECT DISTINCT other.id AS identity_id, \
            'name' AS kind \
     FROM identity me \
     JOIN party mine ON mine.id = me.person_id \
     JOIN membership m ON m.party_id = me.person_id \
     JOIN membership theirs ON theirs.group_id = m.group_id AND theirs.party_id <> m.party_id \
     JOIN party them ON them.id = theirs.party_id AND them.kind = 'person' \
          AND them.display_name = mine.display_name \
     JOIN identity other ON other.person_id = them.id AND other.id <> me.id \
     WHERE me.id = $1";

/// One identity a signal named, and what named it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signalled {
    /// The other identity.
    pub identity: Id<Identity>,
    /// The factor kind, or the signal's own tag.
    pub kind: String,
}

impl FromRow for Signalled {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            identity: row.id("identity_id")?,
            kind: row.text("kind")?,
        })
    }
}

impl Signalled {
    /// Which signal this row is.
    ///
    /// A kind the vocabulary does not cover proposes nothing at all: a factor
    /// kind added to the schema is not silently a signal about people until
    /// somebody decides what it means.
    pub fn signal(&self) -> Option<MatchSignal> {
        Some(match self.kind.as_str() {
            "email" => MatchSignal::VerifiedEmail,
            "oidc" => MatchSignal::OidcSubject,
            "phone" => MatchSignal::VerifiedPhone,
            "link" => MatchSignal::ClaimedLink,
            "name" => MatchSignal::NameAndGroup,
            _ => return None,
        })
    }
}

/// Scan everything that might be said about one identity, and queue it.
///
/// Returns how many candidate rows were written or re-scored. Called from the
/// observation drain after `Register`, `VerifyEmail` and `AddFactor` — the
/// three events that change what an identity has proven.
pub async fn scan_identity<S: Sql>(store: &Store<S>, identity: Id<Identity>) -> Outcome<usize> {
    let mut written = 0;
    let now = store.clock().now();
    let mine = super::person_of(&store.reads(), identity).await?;

    for sql in [SHARED_FACTOR_SQL, CLAIMED_LINK_SQL, NAME_AND_GROUP_SQL] {
        let rows: Vec<Signalled> = store.reads().query(sql, bind![identity]).await?;
        for row in rows {
            let Some(signal) = row.signal() else { continue };
            if row.identity == identity {
                continue;
            }
            // Already one person: there is no question left to ask.
            let theirs = super::person_of(&store.reads(), row.identity).await?;
            if mine.is_some() && mine == theirs {
                continue;
            }
            written += store
                .execute(
                    PROPOSE_QUIETLY,
                    proposal(identity, row.identity, signal, now),
                )
                .await?;
        }
    }
    Ok(written)
}
