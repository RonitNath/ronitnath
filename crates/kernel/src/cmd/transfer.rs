//! `Transfer` — the only *command* that changes who owns a resource.
//!
//! That claim is worth stating precisely, because it is the invariant a
//! proptest asserts over random command sequences: no other command writes
//! that column, and the schema's trigger refuses a group as its value on
//! insert *and* on update, so "groups are granted, never owners" survives this
//! command too.
//!
//! The one other statement in the crate that writes `owner_party_id` is
//! [`merge`](crate::merge), and it is not an exception to the rule so much as
//! a different question: a merge does not change the owner, it changes which
//! party *is* that person. `Split` moves the column back the same way. Both
//! move the `#owner` relation row with it, exactly as this command does.
//!
//! Four statements, one guard. The resource's current owner is the guard, so a
//! transfer that raced another transfer writes nothing rather than overwriting
//! the winner, and every other statement carries the same condition by asking
//! whether the new owner is now on the row.
//!
//! For the party-backed kinds it also moves the owner *membership*: the new
//! owner becomes `owner` and the old one is demoted to `admin` rather than
//! removed, because somebody who hands their organization on has not left it.
//!
//! Two principals may run it: the owner, and a platform operator. The second
//! is what a deployment needs when an owner is gone and the thing they owned
//! is not — and it is audited under the operator's own identity, so a
//! reassignment is always attributable afterwards.

use rn_api::commands::Transfer;

use super::{Batch, Ctx, is_platform_operator, refs, run};
use crate::domain::Vocabulary as _;
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{Id, Identity, Person, Resource};
use crate::org::{self, MemberRole};
use crate::relation::{Object, SubjectKind, Vocabulary};
use crate::resource;
use crate::store::{Cursor, FromRow, Reads, RowError, Sql, Value};
use crate::{Timestamp, bind};

// Every statement after the ownership update asks the same question — is the
// new owner on the row now? — spelled out in each, because the SQL in this
// crate is `&'static str` and there is nothing to interpolate it with.
const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'transfer', $2, $3, $4, $5, \
            json_object('event', 'transfer', 'resource', $6, 'to', $7) \
     WHERE changes() > 0";

/// Withdraw the previous owner's grant, once the column has moved.
const DROP_OWNER: &str = "DELETE FROM relation \
     WHERE object_kind = $1 AND object_id = $2 AND relation = 'owner' \
       AND EXISTS (SELECT 1 FROM resource WHERE id = $3 AND owner_party_id = $4)";

/// Write the new owner's grant, once the column has moved.
const ADD_OWNER: &str = "INSERT OR IGNORE INTO relation \
     (object_kind, object_id, relation, subject_kind, subject_id, granted_by, at) \
     SELECT $1, $2, 'owner', $3, $4, $5, $6 \
     WHERE EXISTS (SELECT 1 FROM resource WHERE id = $7 AND owner_party_id = $4)";

/// The party a party-backed resource stands for.
struct PartyOf(Id<Person>);

impl FromRow for PartyOf {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self(row.id("party_id")?))
    }
}

/// Move a resource's ownership to a person or an organization.
pub async fn transfer<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, args: &Transfer) -> Outcome<Committed> {
    let (identity, person, acting_as) = refs::actor(&ctx.principal)?;
    let resource_id = refs::resource(ctx.store.ids(), &args.resource)?;
    let (to, to_kind) = refs::owner(ctx.store.ids(), &args.to)?;

    let Some(row) = resource::load(ctx.store, resource_id).await? else {
        return decline();
    };
    // Only the owner transfers, and "the owner" is the party on the row —
    // either this person or an organization they belong to.
    let subjects = crate::principal::expand(ctx.store, &ctx.principal).await?;
    let mut owners: Vec<Id<Person>> = vec![person];
    owners.extend(subjects.organizations.iter().map(|o| Id::new(o.get())));
    if !row.owned_by_any(&owners) && !is_platform_operator(&ctx.store.reads(), person).await? {
        return decline();
    }
    if row.owner_party_id == to {
        // The world the caller asked for is the world that exists, but saying
        // so would need an audit row describing a change nobody made.
        return decline();
    }
    let Some(kind) = Vocabulary::KERNEL.kind(&row.kind) else {
        return decline();
    };

    // A party-backed resource is addressed by its party, so the relation and
    // membership work needs that id rather than the resource's.
    let party = if kind.is_container {
        ctx.store
            .query_opt::<PartyOf>(
                "SELECT party_id FROM party_resource WHERE resource_id = $1",
                bind![resource_id],
            )
            .await?
            .map(|found| found.0)
    } else {
        None
    };
    let object = party.map_or_else(
        || Object::resource(kind.name, resource_id),
        |id| Object {
            kind: kind.name,
            id: id.get(),
        },
    );
    let from = row.owner_party_id;
    let now = ctx.now();

    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.one(resource::TRANSFER_SQL, bind![to, resource_id, from]);
        batch.one(
            AUDIT,
            bind![
                ctx.key.to_string(),
                identity,
                acting_as,
                now,
                crate::audit::digest_of(args),
                resource_id,
                to
            ],
        );
        ownership(&mut batch, object, resource_id, to, to_kind, identity, now);
        if party.is_some() {
            memberships(&mut batch, object.id, from, to, now);
        }
        Ok(batch)
    })
    .await?;
    Ok(applied.committed)
}

/// The two relation rows that follow the column: the old owner's grant goes,
/// the new owner's arrives, and both are conditional on the column having
/// actually moved.
fn ownership(
    batch: &mut Batch,
    object: Object,
    resource_id: Id<Resource>,
    to: Id<Person>,
    to_kind: SubjectKind,
    granted_by: Id<Identity>,
    at: Timestamp,
) {
    batch.any(DROP_OWNER, bind![object.kind, object.id, resource_id, to]);
    batch.any(
        ADD_OWNER,
        vec![
            Value::from(object.kind),
            Value::from(object.id),
            Value::from(to_kind.as_str()),
            Value::from(to),
            Value::from(granted_by),
            Value::from(at),
            Value::from(resource_id),
        ],
    );
}

/// Ownership and administration, kept in step for a container: the new owner
/// holds the owner role, and the party it came from stays as an admin.
fn memberships(batch: &mut Batch, container: i64, from: Id<Person>, to: Id<Person>, at: Timestamp) {
    batch.any(
        org::UPSERT_ROLE_SQL,
        bind![container, to, MemberRole::Owner.as_str(), at],
    );
    batch.any(
        org::FORCE_ROLE_SQL,
        bind![MemberRole::Admin.as_str(), container, from],
    );
}
