//! The match queue, and what a ruling on one produced.
//!
//! Every row here is a *question*: two registrations, a signal that suggested
//! they are one human, and a score that orders the queue and decides nothing.
//! No threshold merges anybody — proof does — so an operator console renders
//! the score as what it is, a ranking, and puts the disposition beside it.
//!
//! The drill-in is the answer half. A candidate that was ruled `confirmed`
//! left `person_link` rows behind, append-only, carrying the method and the
//! evidence the operator wrote; a merge that absorbed a person left a
//! `person_alias` row so the absorbed person's URLs keep resolving. Both are
//! read here, because "what did that ruling actually do" is the question an
//! operator asks immediately after making one.

use rn_kernel::bind;
use rn_kernel::ids::{Id, IdKey, Identity, MatchCandidate, Person};
use rn_kernel::store::{Cursor, FromRow, Reads, RowError};
use rn_kernel::{Outcome, Timestamp};
use serde_json::{Value, json};

use super::platform::{page, subject};
use super::{Params, Row};

pub(super) const QUEUE: &str = "SELECT c.id, c.identity_a, c.identity_b, c.signal, c.score, c.status, \
                            c.created_at \
                     FROM match_candidate c ORDER BY c.id DESC LIMIT $1";

pub(super) const ONE: &str = "SELECT id, identity_a, identity_b, signal, score, status, created_at \
                   FROM match_candidate WHERE id = $1";

/// The persons the pair's two registrations resolve to *now*, which is how a
/// confirmed candidate shows that both sides became one.
pub(super) const PERSON_OF: &str = "SELECT i.id, i.person_id, p.display_name \
                         FROM identity i LEFT JOIN party p ON p.id = i.person_id \
                         WHERE i.id = $1";

/// Append-only, so this is the whole history of how the pair was attached.
pub(super) const LINKS: &str = "SELECT l.identity_id, l.person_id, l.method, l.evidence, l.asserted_by, \
                            l.from_person_id, l.at \
                     FROM person_link l WHERE l.identity_id IN ($1, $2) ORDER BY l.id";

/// The alias a merge left behind, from either direction.
///
/// After a merge both registrations resolve to the *survivor*, and the
/// absorbed person is named by nothing but this table — so the survivor is
/// looked for on the `person_id` side as well as the `old_person_id` one.
pub(super) const ALIAS: &str = "SELECT old_person_id, person_id, at FROM person_alias \
                     WHERE old_person_id IN ($1, $2) OR person_id IN ($1, $2) \
                     ORDER BY old_person_id";

/// A queued pair, as both the list and an identity's own panel read it.
pub struct CandidateRow {
    id: Id<MatchCandidate>,
    identity_a: Id<Identity>,
    identity_b: Id<Identity>,
    signal: String,
    score: f64,
    status: String,
    created_at: Timestamp,
}

impl FromRow for CandidateRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            identity_a: row.id("identity_a")?,
            identity_b: row.id("identity_b")?,
            signal: row.text("signal")?,
            score: row.real("score")?,
            status: row.text("status")?,
            created_at: row.int("created_at")?,
        })
    }
}

impl CandidateRow {
    /// What a candidate says about itself, without the persons behind it.
    pub(super) fn shown(&self, key: &IdKey) -> Value {
        json!({
            "public_id": self.id.public(key),
            "identity_a": self.identity_a.public(key),
            "identity_b": self.identity_b.public(key),
            "signal": self.signal,
            "score": self.score,
            "status": self.status,
            "created_at": self.created_at,
        })
    }
}

pub(super) async fn list(reads: &impl Reads, params: &Params) -> Outcome<Vec<Row>> {
    let key = reads.ids();
    let mut rows = Vec::new();
    for candidate in reads
        .query::<CandidateRow>(QUEUE, bind![page(params)])
        .await?
    {
        rows.push(Row {
            key: candidate.id.public(key).as_str().to_owned(),
            value: with_persons(reads, &candidate, key).await?,
        });
    }
    Ok(rows)
}

pub(super) async fn one(reads: &impl Reads, params: &Params) -> Outcome<Vec<Row>> {
    let key = reads.ids();
    let Some(candidate) = subject::<MatchCandidate>(key, params) else {
        return Ok(Vec::new());
    };
    let Some(row) = reads
        .query_opt::<CandidateRow>(ONE, bind![candidate])
        .await?
    else {
        return Ok(Vec::new());
    };
    let mut value = with_persons(reads, &row, key).await?;
    let object = value.as_object_mut().expect("a candidate is an object");
    object.insert("links".to_owned(), links(reads, &row, key).await?);
    object.insert("alias".to_owned(), alias(reads, &row, key).await?);
    Ok(vec![Row {
        key: row.id.public(key).as_str().to_owned(),
        value,
    }])
}

/// The candidate, plus which person each side resolves to today.
///
/// This is what makes a merged pair legible: the two registrations still have
/// their own rows, and both now name the same person.
async fn with_persons(reads: &impl Reads, row: &CandidateRow, key: &IdKey) -> Outcome<Value> {
    let mut value = row.shown(key);
    let object = value.as_object_mut().expect("a candidate is an object");
    object.insert(
        "person_a".to_owned(),
        person_of(reads, row.identity_a, key).await?,
    );
    object.insert(
        "person_b".to_owned(),
        person_of(reads, row.identity_b, key).await?,
    );
    Ok(value)
}

struct Resolved {
    person_id: Option<Id<Person>>,
    display_name: Option<String>,
}

impl FromRow for Resolved {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let _: i64 = row.int("id")?;
        Ok(Self {
            person_id: row.id_opt("person_id")?,
            display_name: row.text_opt("display_name")?,
        })
    }
}

async fn person_of(reads: &impl Reads, identity: Id<Identity>, key: &IdKey) -> Outcome<Value> {
    let Some(row) = reads
        .query_opt::<Resolved>(PERSON_OF, bind![identity])
        .await?
    else {
        return Ok(Value::Null);
    };
    Ok(match row.person_id {
        Some(person) => json!({
            "public_id": person.public(key),
            "display": row.display_name.unwrap_or_default(),
        }),
        None => Value::Null,
    })
}

struct LinkRow {
    identity_id: Id<Identity>,
    person_id: Id<Person>,
    method: String,
    evidence: Option<String>,
    asserted_by: Option<Id<Identity>>,
    from_person_id: Option<Id<Person>>,
    at: Timestamp,
}

impl FromRow for LinkRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            identity_id: row.id("identity_id")?,
            person_id: row.id("person_id")?,
            method: row.text("method")?,
            evidence: row.text_opt("evidence")?,
            asserted_by: row.id_opt("asserted_by")?,
            from_person_id: row.id_opt("from_person_id")?,
            at: row.int("at")?,
        })
    }
}

async fn links(reads: &impl Reads, row: &CandidateRow, key: &IdKey) -> Outcome<Value> {
    Ok(Value::Array(
        reads
            .query::<LinkRow>(LINKS, bind![row.identity_a, row.identity_b])
            .await?
            .into_iter()
            .map(|link| {
                json!({
                    "identity": link.identity_id.public(key),
                    "person": link.person_id.public(key),
                    "from_person": link.from_person_id.map(|id| id.public(key)),
                    "method": link.method,
                    // The operator wrote this and it is why the merge stands.
                    "evidence": link.evidence,
                    "asserted_by": link.asserted_by.map(|id| id.public(key)),
                    "at": link.at,
                })
            })
            .collect(),
    ))
}

struct AliasRow {
    old_person_id: Id<Person>,
    person_id: Id<Person>,
    at: Timestamp,
}

impl FromRow for AliasRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            old_person_id: row.id("old_person_id")?,
            person_id: row.id("person_id")?,
            at: row.int("at")?,
        })
    }
}

/// The alias rows this pair produced, so an old URL is visibly still an
/// address for somebody.
async fn alias(reads: &impl Reads, row: &CandidateRow, key: &IdKey) -> Outcome<Value> {
    let (a, b) = (
        person_id(reads, row.identity_a).await?,
        person_id(reads, row.identity_b).await?,
    );
    Ok(Value::Array(
        reads
            .query::<AliasRow>(ALIAS, bind![a, b])
            .await?
            .into_iter()
            .map(|alias| {
                json!({
                    "was": alias.old_person_id.public(key),
                    "now": alias.person_id.public(key),
                    "at": alias.at,
                })
            })
            .collect(),
    ))
}

async fn person_id(reads: &impl Reads, identity: Id<Identity>) -> Outcome<i64> {
    Ok(reads
        .query_opt::<Resolved>(PERSON_OF, bind![identity])
        .await?
        .and_then(|row| row.person_id)
        .map_or(0, Id::get))
}
