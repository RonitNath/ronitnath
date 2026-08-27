//! The match queue: what a signal is allowed to say, and how it says it.
//!
//! Every row here is a *question*. `status` starts at `proposed` and the only
//! statement in this module that writes it writes that literal — a signal
//! cannot express `confirmed` even if the code asking it to were wrong, which
//! is the point of the vocabulary being a CHECK constraint and the literal
//! being in the SQL rather than in a parameter.
//!
//! [`propose_match`] is the command a person or an operator uses to raise a
//! pair by hand. The scan raises the rest on the observation lane
//! ([`scan`](super::scan)) through the same statement.

use rn_api::commands::{MatchSignal, ProposeMatch};

use super::recovery::owned_identity;
use crate::bind;
use crate::cmd::{Applied, Batch, Ctx, is_platform_operator, run};
use crate::domain::Vocabulary;
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{self, Id, Identity, MatchCandidate};
use crate::store::{Cursor, FromRow, Reads, RowError, Sql, Value};

/// Where a candidate stands: the `match_candidate.status` vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CandidateStatus {
    /// A signal, or somebody, suggested the pair. Nothing follows from it.
    Proposed,
    /// Proof arrived and the merge ran.
    Confirmed,
    /// A ruling closed it. The row stays so the same pair does not re-queue
    /// forever.
    Rejected,
}

impl Vocabulary for CandidateStatus {
    const ALL: &'static [Self] = &[Self::Proposed, Self::Confirmed, Self::Rejected];
    const COLUMN: (&'static str, &'static str) = ("match_candidate", "status");

    fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected",
        }
    }
}

impl Vocabulary for MatchSignal {
    const ALL: &'static [Self] = &[
        Self::VerifiedPhone,
        Self::VerifiedEmail,
        Self::OidcSubject,
        Self::ClaimedLink,
        Self::NameAndGroup,
    ];
    const COLUMN: (&'static str, &'static str) = ("match_candidate", "signal");

    fn as_str(self) -> &'static str {
        match self {
            Self::VerifiedPhone => "verified_phone",
            Self::VerifiedEmail => "verified_email",
            Self::OidcSubject => "oidc_subject",
            Self::ClaimedLink => "claimed_link",
            Self::NameAndGroup => "name_and_group",
        }
    }
}

/// How much a signal is worth.
///
/// The numbers order the queue and nothing else. No threshold anywhere merges
/// anybody — proof does — so the strongest signal in the table is still only a
/// reason to *ask*. `name_and_group` is far below the rest because two people
/// with the same name in one group is a Tuesday, not evidence.
pub const fn score(signal: MatchSignal) -> f64 {
    match signal {
        MatchSignal::OidcSubject => 0.95,
        MatchSignal::VerifiedPhone => 0.9,
        MatchSignal::VerifiedEmail => 0.85,
        MatchSignal::ClaimedLink => 0.7,
        MatchSignal::NameAndGroup => 0.2,
    }
}

/// A queued pair.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CandidateRow {
    /// The row.
    pub id: Id<MatchCandidate>,
    /// The lower identity of the pair.
    pub identity_a: Id<Identity>,
    /// The higher one. The schema's CHECK keeps them that way round, so a pair
    /// has one row per signal however it was observed.
    pub identity_b: Id<Identity>,
    /// What suggested them.
    pub signal: MatchSignal,
    /// What that signal is worth.
    pub score: f64,
    /// Where it stands.
    pub status: CandidateStatus,
}

impl FromRow for CandidateRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let signal = row.text("signal")?;
        let status = row.text("status")?;
        Ok(Self {
            id: row.id("id")?,
            identity_a: row.id("identity_a")?,
            identity_b: row.id("identity_b")?,
            score: row.real("score")?,
            signal: MatchSignal::parse(&signal).ok_or_else(|| RowError::Column {
                column: "signal".to_owned(),
                wanted: "a match signal",
            })?,
            status: CandidateStatus::parse(&status).ok_or_else(|| RowError::Column {
                column: "status".to_owned(),
                wanted: "a candidate status",
            })?,
        })
    }
}

impl CandidateRow {
    /// The other identity of the pair, given one of them.
    pub fn other(&self, one: Id<Identity>) -> Option<Id<Identity>> {
        match one {
            _ if one == self.identity_a => Some(self.identity_b),
            _ if one == self.identity_b => Some(self.identity_a),
            _ => None,
        }
    }
}

/// One candidate by its row. A primary-key seek.
pub const BY_ID_SQL: &str = "SELECT id, identity_a, identity_b, signal, score, status \
     FROM match_candidate WHERE id = $1";

/// What is proposed about an identity. Rides `match_candidate_a_idx`; the
/// `identity_b` side rides `match_candidate_b_idx` with the same shape.
pub const BY_IDENTITY_SQL: &str = "SELECT id, identity_a, identity_b, signal, score, status \
     FROM match_candidate WHERE identity_a = $1 AND status = 'proposed'";

/// Read one candidate.
pub async fn by_id(
    store: &impl Reads,
    candidate: Id<MatchCandidate>,
) -> Outcome<Option<CandidateRow>> {
    store
        .query_opt::<CandidateRow>(BY_ID_SQL, bind![candidate])
        .await
        .map_err(Into::into)
}

/// The one statement that writes the queue.
///
/// `'proposed'` is a literal, not a parameter: there is no value a caller or a
/// scan could pass that would land a row in `confirmed`. A pair that is
/// already queued for the same signal keeps its status and takes the new
/// score, so re-observing a signal re-ranks the question rather than reopening
/// a decision.
pub const PROPOSE: &str = "INSERT INTO match_candidate \
     (identity_a, identity_b, signal, score, status, created_at) \
     VALUES ($1, $2, $3, $4, 'proposed', $5) \
     ON CONFLICT (identity_a, identity_b, signal) DO UPDATE SET score = excluded.score \
     RETURNING id";

/// The same statement without its `RETURNING` clause, for the observation
/// lane. A scan has no batch to hand the new row's id to and no audit row to
/// put it on — it raises a question and walks away — and `execute` refuses a
/// statement that returns rows.
pub const PROPOSE_QUIETLY: &str = "INSERT INTO match_candidate \
     (identity_a, identity_b, signal, score, status, created_at) \
     VALUES ($1, $2, $3, $4, 'proposed', $5) \
     ON CONFLICT (identity_a, identity_b, signal) DO UPDATE SET score = excluded.score";

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'propose-match', $2, $3, $4, $5, \
            json_object('event', 'propose-match', 'candidate', $6) \
     WHERE changes() > 0";

/// The parameters of [`PROPOSE`], with the pair put the right way round.
pub fn proposal(
    a: Id<Identity>,
    b: Id<Identity>,
    signal: MatchSignal,
    at: crate::Timestamp,
) -> Vec<Value> {
    let (low, high) = (a.min(b), a.max(b));
    bind![low, high, signal.as_str(), score(signal), at]
}

/// Put a pair on the queue.
///
/// A caller may propose about an identity they hold, or about anybody if they
/// are a platform operator. Proposing about two strangers is not a small
/// liberty: the queue is visible to the people in it, so an open `ProposeMatch`
/// would let anyone tell any two people that a third thinks they are the same
/// human.
pub async fn propose_match<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &ProposeMatch,
) -> Outcome<Committed> {
    let (Ok(a), Ok(b)) = (
        ids::decode::<Identity>(ctx.store.ids(), &args.identity_a),
        ids::decode::<Identity>(ctx.store.ids(), &args.identity_b),
    ) else {
        return decline();
    };
    if a == b {
        return decline();
    }
    let (identity, person, acting_as) = crate::cmd::refs::actor(&ctx.principal)?;
    let mine =
        owned_identity(ctx.store, person, a).await? || owned_identity(ctx.store, person, b).await?;
    if !mine && !is_platform_operator(&ctx.store.reads(), person).await? {
        return decline();
    }
    let now = ctx.now();

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        let row = batch.one(PROPOSE, proposal(a, b, args.signal, now));
        batch.one(
            AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                Value::from(identity),
                Value::from(acting_as),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                row.column("id"),
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(committed)
}
