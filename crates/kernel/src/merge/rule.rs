//! `RuleMatch` — a platform operator rules on a pair the person cannot prove
//! themselves.
//!
//! This is the path for the account somebody lost: the mailbox is gone, the
//! password with it, and there is no second session to hold up. What is left
//! is a human being convinced by something outside the system — a passport, a
//! support call, a payment record — and the model's answer is not to trust
//! that silently but to *write it down*. Evidence is mandatory, it lands on
//! every `person_link` row the merge writes and on the audit row, and both are
//! append-only, so a ruling is attributable for as long as the rows exist.
//!
//! A ruling can also go the other way. `same_person: false` closes the
//! candidate as `rejected` rather than deleting it, so the pair stops
//! re-queueing every time the same weak signal fires again.

use rn_api::commands::RuleMatch;

use super::candidate::{self, CandidateStatus};
use super::{LinkMethod, Plan, apply};
use crate::authority::{self, Want};
use crate::bind;
use crate::cmd::{Applied, Batch, Ctx, run};
use crate::error::{Invalid, Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{self, Id, Identity, MatchCandidate, Person};
use crate::store::{Count, Reads, Sql, Value};

/// How long a ruling's evidence may be. Long enough for the paragraph an
/// operator should write, short enough that the column is not a document store.
pub const EVIDENCE_LIMIT: usize = 2_000;

const CONFIRMED: &str =
    "UPDATE match_candidate SET status = 'confirmed' WHERE id = $1 AND status = 'proposed'";

/// A rejection is the whole change, so it carries the guard itself.
const REJECTED: &str =
    "UPDATE match_candidate SET status = 'rejected' WHERE id = $1 AND status = 'proposed'";

const AUDIT_MERGE: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'rule-match', $2, $3, $4, $5, \
            json_object('event', 'rule-match', 'candidate', $6, 'survivor', $7, \
                        'absorbed', $8, 'method', 'operator', 'evidence', $9) \
     WHERE changes() > 0";

const AUDIT_LINK: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'rule-match', $2, $3, $4, $5, \
            json_object('event', 'rule-match', 'candidate', $6, 'identity', $7, \
                        'person', $8, 'method', 'operator', 'evidence', $9) \
     WHERE changes() > 0";

const AUDIT_REJECT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'rule-match', $2, $3, $4, $5, \
            json_object('event', 'rule-match', 'candidate', $6, 'evidence', $7) \
     WHERE changes() > 0";

/// Rule on a queued pair.
pub async fn rule_match<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &RuleMatch,
) -> Outcome<Committed> {
    let (identity, person, acting_as) = crate::cmd::refs::actor(&ctx.principal)?;
    let evidence = args.evidence.trim();
    if evidence.is_empty() {
        return Err(Invalid::Missing("evidence").into());
    }
    if evidence.chars().count() > EVIDENCE_LIMIT {
        return Err(Invalid::TooLong {
            field: "evidence",
            limit: EVIDENCE_LIMIT,
        }
        .into());
    }
    // Platform administration is a relation, not a column: `platform:*
    // #operator @person`. An operator who loses the row loses the command.
    let _ = person;
    authority::require(ctx, Want::Platform).await?;
    let Ok(candidate) = ids::decode::<MatchCandidate>(ctx.store.ids(), &args.candidate) else {
        return decline();
    };
    let Some(row) = candidate::by_id(&ctx.store.reads(), candidate).await? else {
        return decline();
    };
    if row.status != CandidateStatus::Proposed {
        return decline();
    }
    let plan = super::plan(&ctx.store.reads(), row.identity_a, row.identity_b).await?;
    if args.same_person && plan == Plan::Nothing {
        return decline();
    }
    let now = ctx.now();

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        let head = vec![
            Value::from(ctx.key.to_string()),
            Value::from(identity),
            Value::from(acting_as),
            Value::from(now),
            Value::from(crate::audit::digest_of(args)),
            Value::from(candidate),
        ];
        if args.same_person {
            batch.any(CONFIRMED, bind![candidate]);
            merged(&mut batch, plan, identity, evidence, now, head);
        } else {
            batch.one(REJECTED, bind![candidate]);
            let mut params = head;
            params.push(Value::from(evidence));
            batch.one(AUDIT_REJECT, params);
        }
        Ok(batch)
    })
    .await?;
    Ok(committed)
}

/// The merge itself, and the audit row that carries the evidence.
fn merged(
    batch: &mut Batch,
    plan: Plan,
    asserted_by: Id<Identity>,
    evidence: &str,
    now: crate::Timestamp,
    head: Vec<Value>,
) {
    let (sql, subject, object): (_, Value, Value) = match plan {
        Plan::Absorb { survivor, absorbed } => {
            apply::Merge {
                survivor,
                absorbed,
                method: LinkMethod::Operator,
                evidence: Some(evidence.to_owned()),
                asserted_by: Some(asserted_by),
                at: now,
            }
            .push(batch);
            (AUDIT_MERGE, Value::from(survivor), Value::from(absorbed))
        }
        Plan::Link { identity, person } => {
            apply::Link {
                identity,
                person,
                method: LinkMethod::Operator,
                evidence: Some(evidence.to_owned()),
                asserted_by: Some(asserted_by),
                at: now,
            }
            .push(batch);
            (AUDIT_LINK, Value::from(identity), Value::from(person))
        }
        // Refused before the batch was built.
        Plan::Nothing => return,
    };
    let mut params = head;
    params.extend([subject, object, Value::from(evidence)]);
    batch.one(sql, params);
}

/// The `platform:* #operator @person` row. Not a command: `SetRole` writes it
/// in the ordinary way, and this is the bootstrap that has to exist before
/// there is anybody to run that command.
pub const OPERATOR_SQL: &str = "INSERT OR IGNORE INTO relation \
     (object_kind, object_id, relation, subject_kind, subject_id, granted_by, at) \
     VALUES ('platform', 0, 'operator', 'person', $1, NULL, $2)";

/// Whether this deployment has an operator at all.
const ANY_OPERATOR_SQL: &str = "SELECT count(*) AS n FROM relation \
     WHERE object_kind = 'platform' AND object_id = 0 AND relation = 'operator'";

/// Grant `platform:* #operator` to a person — once, for the *first* operator.
///
/// The server's CLI reaches this as [`bootstrap_operator`](crate::bootstrap_operator).
/// It writes no audit row and authorises nobody, which is exactly why it must
/// not be a second, quieter `SetRole`: an unaudited grant is defensible as the
/// act that creates the first operator out of nothing and indefensible as the
/// act that creates the second. So it declines the moment any operator row
/// exists, and from there administration is a command with an actor on it.
///
/// The check and the insert are two statements rather than one guarded
/// `INSERT … WHERE NOT EXISTS`, because the answer wanted here is a refusal
/// the caller can print, not a silent no-op.
pub async fn make_operator<S: Sql>(
    store: &crate::store::Store<S>,
    person: Id<Person>,
) -> Outcome<()> {
    let existing = store.query::<Count>(ANY_OPERATOR_SQL, bind![]).await?;
    if existing.first().is_some_and(|row| row.0 > 0) {
        return decline();
    }
    let now = store.clock().now();
    store.execute(OPERATOR_SQL, bind![person, now]).await?;
    Ok(())
}
