//! `Split` — the inverse of a merge, for one identity.
//!
//! A merge that should not have happened is not repaired by deleting rows:
//! `person_link` is append-only and the absorbed person is still an alias, so
//! there is nothing to delete and no history to lose. What `Split` does
//! instead is give the identity a person of its own again, and hand that
//! person back the rows the identity arrived with.
//!
//! "The rows it arrived with" is a fact the database holds rather than a
//! reconstruction: [`apply`](super::apply) unions the absorbed person's
//! memberships and grants onto the survivor and leaves the absorbed person's
//! own rows where they are, and every `person_link` row records the person the
//! identity came *from*. So a split reads that person and copies its rows onto
//! the new one — which is why `split ∘ merge` restores the membership set the
//! identity had, rather than an approximation of it.
//!
//! The survivor keeps what it gained. A merge told it those grants were its
//! own, other people have acted on that for however long the merge stood, and
//! silently revoking them would be a second unreviewed change on top of the
//! first. Taking a grant away is `Revoke`, and it is somebody's decision.
//!
//! ## What a split restores of ownership, and what it cannot
//!
//! Ownership is the one thing a merge moves that both parties cannot keep:
//! `resource.owner_party_id` holds exactly one party. So where a grant is
//! copied, ownership is *moved back* — and the survivor's `#owner` row for
//! those resources goes with it, because a column and the row that mirrors it
//! may not disagree.
//!
//! Which resources move back is a fact the database holds: the absorbed
//! person's own `#owner` rows were left in place by the merge, so
//! "the resources `from_person_id` owned when it was absorbed" is a query, not
//! a reconstruction. Two things bound it, and the tests say both:
//!
//! * **Only what the survivor still owns.** A resource the survivor has since
//!   `Transfer`red on is left alone. The split is undoing a merge, not
//!   reversing every decision taken while it stood.
//! * **Per person, never per identity.** A `resource` row names a party, and
//!   nothing anywhere records which *identity* of that party brought it. If
//!   the absorbed person was itself made of two identities, splitting one of
//!   them out hands that one all of the former person's resources. Splitting
//!   the second afterwards then moves them on again, because by then the first
//!   new person is the owner and holds the `#owner` row. Sorting that out is
//!   `Transfer`, and it is somebody's decision.

use rn_api::commands::Split;

use super::LinkMethod;
use super::recovery::owned_identity;
use super::rule::EVIDENCE_LIMIT;
use crate::bind;
use crate::cmd::{Applied, Batch, Ctx, is_platform_operator, member, run};
use crate::domain::Vocabulary;
use crate::error::{Invalid, Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{self, Id, Identity, Person};
use crate::store::{Count, Reads, Sql, Value};

/// Every statement below carries this guard: the identity is still on the
/// person it was read on. A lost race therefore writes nothing — the batch
/// rolls back rather than half-applying — and the caller re-reads.
const PARTY: &str = "INSERT INTO party (kind, display_name, status, created_at) \
     SELECT 'person', p.display_name, 'active', $1 FROM party p \
     WHERE p.id = $2 AND EXISTS (SELECT 1 FROM identity WHERE id = $3 AND person_id = $2) \
     RETURNING id";

const LINK: &str = "INSERT INTO person_link \
     (identity_id, person_id, method, evidence, asserted_by, from_person_id, at) \
     SELECT $1, $2, $3, $4, $5, $6, $7 \
     WHERE EXISTS (SELECT 1 FROM identity WHERE id = $1 AND person_id = $6)";

const MEMBERSHIPS: &str = "INSERT OR IGNORE INTO membership (group_id, party_id, role, at) \
     SELECT m.group_id, $1, m.role, $2 FROM membership m WHERE m.party_id = $3";

const RELATIONS_AS_SUBJECT: &str = "INSERT OR IGNORE INTO relation \
     (object_kind, object_id, relation, subject_kind, subject_id, granted_by, at) \
     SELECT r.object_kind, r.object_id, r.relation, 'person', $1, r.granted_by, $2 \
     FROM relation r WHERE r.subject_kind = 'person' AND r.subject_id = $3";

const RELATIONS_AS_OBJECT: &str = "INSERT OR IGNORE INTO relation \
     (object_kind, object_id, relation, subject_kind, subject_id, granted_by, at) \
     SELECT 'person', $1, r.relation, r.subject_kind, r.subject_id, r.granted_by, $2 \
     FROM relation r WHERE r.object_kind = 'person' AND r.object_id = $3";

/// Ownership moves back, for the resources the person this identity came from
/// owned at the moment it was absorbed and the survivor still owns today.
///
/// The `#owner` relation row the merge left on the absorbed person is the
/// record of what it owned; `COALESCE` is what lets one statement address both
/// shapes of object, because a document's `#owner` row names its `resource`
/// row and an organization's names its `party` row.
const RESOURCES: &str = "UPDATE resource SET owner_party_id = $1 WHERE owner_party_id = $2 \
     AND EXISTS (SELECT 1 FROM relation r WHERE r.relation = 'owner' \
                   AND r.subject_kind = 'person' AND r.subject_id = $3 \
                   AND r.object_kind = resource.kind \
                   AND r.object_id = COALESCE( \
                       (SELECT pr.party_id FROM party_resource pr \
                         WHERE pr.resource_id = resource.id), resource.id))";

/// And the survivor stops claiming to own what it no longer owns. Guarded on
/// the column having actually moved, so it can only ever remove a row the
/// statement above just contradicted.
/// Parameters are numbered in the order they appear, because that is the order
/// SQLite assigns `$n` names an index in: the survivor first, the new person
/// second.
const DROP_OWNER: &str = "DELETE FROM relation WHERE relation = 'owner' \
       AND subject_kind = 'person' AND subject_id = $1 \
       AND EXISTS (SELECT 1 FROM resource res WHERE res.owner_party_id = $2 \
                     AND res.kind = relation.object_kind \
                     AND relation.object_id = COALESCE( \
                         (SELECT pr.party_id FROM party_resource pr \
                           WHERE pr.resource_id = res.id), res.id))";

/// The identity's own sessions follow it. Everyone else's stay where they are.
const SESSIONS: &str =
    "UPDATE session SET acting_as = $1 WHERE identity_id = $2 AND acting_as = $3";

/// The guard flips here, last of the mutations.
const IDENTITY: &str = "UPDATE identity SET person_id = $1 WHERE id = $2 AND person_id = $3";

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'split', $2, $3, $4, $5, \
            json_object('event', 'split', 'identity', $6, 'person', $7) \
     WHERE changes() > 0";

/// The person this identity was attached to before its most recent link.
pub const FROM_PERSON_SQL: &str = "SELECT from_person_id AS n FROM person_link \
     WHERE identity_id = $1 AND from_person_id IS NOT NULL ORDER BY at DESC, id DESC LIMIT 1";

/// How many identities a person is made of.
const SIBLINGS_SQL: &str = "SELECT count(*) AS n FROM identity WHERE person_id = $1";

/// Detach an identity onto a person of its own.
pub async fn split<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, args: &Split) -> Outcome<Committed> {
    let (actor, acting_as, _) = member(&ctx.principal)?;
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
    let Ok(identity) = ids::decode::<Identity>(ctx.store.ids(), &args.identity) else {
        return decline();
    };
    let Some(person) = super::person_of(&ctx.store.reads(), identity).await? else {
        return decline();
    };

    // A person is theirs to split, or an operator's. Nobody else's.
    let method =
        if owned_identity(&ctx.store.reads(), acting_as, identity).await? && acting_as == person {
            LinkMethod::SelfLink
        } else if is_platform_operator(&ctx.store.reads(), acting_as).await? {
            LinkMethod::Operator
        } else {
            return decline();
        };

    // Splitting the only identity of a person would move it from one person to
    // an identical new one and abandon the first: a rename with extra steps.
    let siblings = ctx
        .store
        .query::<Count>(SIBLINGS_SQL, bind![person])
        .await?;
    if siblings.first().is_none_or(|row| row.0 < 2) {
        return decline();
    }

    let restore_from = ctx
        .store
        .query::<Count>(FROM_PERSON_SQL, bind![identity])
        .await?
        .first()
        .map(|row| Id::<Person>::new(row.0));
    let now = ctx.now();

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        let party = batch.one(PARTY, bind![now, person, identity]);
        let fresh = party.column("id");
        batch.one(
            LINK,
            vec![
                Value::from(identity),
                fresh.clone(),
                Value::from(method.as_str()),
                Value::from(evidence),
                Value::from(actor),
                Value::from(person),
                Value::from(now),
            ],
        );
        // Nothing to restore for an identity that was never merged: the
        // statements run against a person that does not exist and copy no rows.
        let source = Value::from(restore_from);
        for sql in [MEMBERSHIPS, RELATIONS_AS_SUBJECT, RELATIONS_AS_OBJECT] {
            batch.any(sql, vec![fresh.clone(), Value::from(now), source.clone()]);
        }
        batch.any(
            RESOURCES,
            vec![fresh.clone(), Value::from(person), source.clone()],
        );
        batch.any(DROP_OWNER, vec![Value::from(person), fresh.clone()]);
        batch.any(
            SESSIONS,
            vec![fresh.clone(), Value::from(identity), Value::from(person)],
        );
        batch.one(
            IDENTITY,
            vec![fresh.clone(), Value::from(identity), Value::from(person)],
        );
        batch.one(
            AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                Value::from(actor),
                Value::from(acting_as),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                Value::from(identity),
                fresh,
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(committed)
}
