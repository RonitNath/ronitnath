//! The member tier's match queue: pairs a signal has proposed that involve one
//! of this person's own registrations.
//!
//! A signal proposes and proof disposes (`crate::merge`). What this query
//! answers is therefore only "what has been asked about me" — the signal, its
//! score, and which of my registrations is on each side. It never answers
//! "who else is in the queue", because a candidate is reached through the
//! asking person's own identities and through nothing else: a stranger's pair
//! has no row here to address.
//!
//! Both sides are asked for, and separately. `match_candidate` stores the pair
//! ordered (`identity_a < identity_b`), so "candidates about me" is two index
//! seeks — `match_candidate_a_idx` and `match_candidate_b_idx` — unioned,
//! rather than one `OR` the planner would have to give up on.

use rn_kernel::bind;
use rn_kernel::ids::{Id, IdKey, Identity, MatchCandidate, Person};
use rn_kernel::principal::SubjectSet;
use rn_kernel::store::{Cursor, FromRow, Reads, RowError};
use rn_kernel::{Outcome, Timestamp};
use serde_json::json;

use super::Row;

/// The identities one person is made of.
pub const MY_IDENTITIES_SQL: &str =
    "SELECT id AS id FROM identity WHERE person_id = $1 ORDER BY id";

/// Proposed candidates naming any of a set of identities, either side.
pub const CANDIDATES_SQL: &str = "SELECT c.id AS id, c.identity_a AS identity_a, \
     c.identity_b AS identity_b, c.signal AS signal, c.score AS score, \
     c.created_at AS created_at \
     FROM json_each($1) m \
     CROSS JOIN match_candidate c ON c.identity_a = m.value \
     WHERE c.status = 'proposed' \
     UNION \
     SELECT c.id, c.identity_a, c.identity_b, c.signal, c.score, c.created_at \
     FROM json_each($1) m \
     CROSS JOIN match_candidate c ON c.identity_b = m.value \
     WHERE c.status = 'proposed'";

/// What to call the registration on the other side of a pair.
///
/// A person's display name when the identity has resolved to one; nothing when
/// it has not. Never the address it registered with — the other side of a
/// match is somebody the reader has not yet proved is themselves, and an
/// address would be readable before that proof, not after.
pub const NAMES_SQL: &str = "SELECT i.id AS id, p.display_name AS display \
     FROM json_each($1) x \
     CROSS JOIN identity i ON i.id = x.value \
     LEFT JOIN party p ON p.id = i.person_id";

struct RowId(Id<Identity>);

impl FromRow for RowId {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self(row.id("id")?))
    }
}

struct Candidate {
    id: Id<MatchCandidate>,
    identity_a: Id<Identity>,
    identity_b: Id<Identity>,
    signal: String,
    score: f64,
    created_at: Timestamp,
}

impl FromRow for Candidate {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            identity_a: row.id("identity_a")?,
            identity_b: row.id("identity_b")?,
            signal: row.text("signal")?,
            score: row.real("score")?,
            created_at: row.int("created_at")?,
        })
    }
}

struct Named {
    id: Id<Identity>,
    display: Option<String>,
}

impl FromRow for Named {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            display: row.text_opt("display")?,
        })
    }
}

/// The identities of a person, as the JSON array both statements take.
async fn mine(reads: &impl Reads, person: Id<Person>) -> Outcome<Vec<Id<Identity>>> {
    Ok(reads
        .query::<RowId>(MY_IDENTITIES_SQL, bind![person])
        .await?
        .into_iter()
        .map(|row| row.0)
        .collect())
}

fn as_json(ids: &[Id<Identity>]) -> String {
    let raw: Vec<i64> = ids.iter().map(|id| id.get()).collect();
    serde_json::to_string(&raw).unwrap_or_else(|_| "[]".to_owned())
}

/// The proposed pairs involving this person's registrations.
pub(super) async fn matches(
    reads: &impl Reads,
    subjects: &SubjectSet,
    key: &IdKey,
) -> Outcome<Vec<Row>> {
    let Some(person) = subjects.person else {
        return Ok(Vec::new());
    };
    let mine = mine(reads, person).await?;
    if mine.is_empty() {
        return Ok(Vec::new());
    }
    let candidates = reads
        .query::<Candidate>(CANDIDATES_SQL, bind![as_json(&mine)])
        .await?;

    let others: Vec<Id<Identity>> = candidates
        .iter()
        .flat_map(|row| [row.identity_a, row.identity_b])
        .filter(|id| !mine.contains(id))
        .collect();
    let names = reads
        .query::<Named>(NAMES_SQL, bind![as_json(&others)])
        .await?;

    Ok(candidates
        .into_iter()
        .map(|row| {
            let held = mine.contains(&row.identity_a);
            let (held_side, other) = if held {
                (row.identity_a, row.identity_b)
            } else {
                (row.identity_b, row.identity_a)
            };
            let display = names
                .iter()
                .find(|named| named.id == other)
                .and_then(|named| named.display.clone());
            let public_id = row.id.public(key);
            Row {
                key: public_id.as_str().to_owned(),
                value: json!({
                    "public_id": public_id,
                    "signal": row.signal,
                    "score": row.score,
                    "created_at": row.created_at,
                    "mine": held_side.public(key),
                    "other": other.public(key),
                    "display": display,
                    "both": mine.contains(&other),
                }),
            }
        })
        .collect())
}
