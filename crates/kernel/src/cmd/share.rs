//! `Share` — one relation, one subject, one row.
//!
//! Two gates, and they answer different questions.
//!
//! **May this principal share it at all?** `editor` or better on the object.
//! Not "may they view it": a viewer who could share would make every grant
//! transitive, and the person who granted the view would have no way to know.
//! `owner` covers `editor`, so an owner passes without a second rule. A
//! *person* is the exception with a rule of its own: your contact details are
//! shared by you and by nobody else, so the gate there is being that person.
//!
//! **Does this kind accept this?** The kind's vocabulary says which relations
//! it admits and which subjects each of those accepts. A patient record that
//! refuses `@public` refuses it here, once, rather than in every command that
//! might write a row — which is what makes "products plug in" a registration
//! and not a code review.

use rn_api::commands::Share;

use super::{Batch, Ctx, refs, run};
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::principal::expand;
use crate::relation::{self, Object, Relation, Vocabulary};
use crate::resource::{self, ResourceStatus};
use crate::store::{Reads, Sql, Value};

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     VALUES ($1, 'share', $2, $3, $4, $5, \
             json_object('event', 'share', 'object_kind', $6, 'object_id', $7, \
                         'relation', $8))";

/// The relation a caller must hold to hand one out.
pub const SHARER: Relation = Relation::Editor;

/// Turn a wire role into a relation. The wire cannot name `owner`, and this is
/// where that stays true.
pub(super) const fn relation_of(role: rn_api::whoami::DocRole) -> Relation {
    match role {
        rn_api::whoami::DocRole::Viewer => Relation::Viewer,
        rn_api::whoami::DocRole::Commenter => Relation::Commenter,
        rn_api::whoami::DocRole::Editor => Relation::Editor,
        rn_api::whoami::DocRole::Contact => Relation::Contact,
    }
}

/// Resolve the object a sharing command names: a live resource of a
/// registered, resource-backed kind, or a person.
pub(super) async fn object_of<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    id: &rn_api::ids::PublicId,
) -> Outcome<Object> {
    // A person is an object as much as a document is — `person:P #contact
    // @group:X` is a row in the same table — and their public id is what says
    // so, since a person has no resource row to name.
    if let Ok(who) = crate::ids::decode::<crate::ids::Person>(ctx.store.ids(), id) {
        return Ok(Object::person(who));
    }
    let resource_id = refs::resource(ctx.store.ids(), id)?;
    let Some(row) = resource::load(ctx.store, resource_id).await? else {
        return decline();
    };
    if row.status == ResourceStatus::Deleted {
        return decline();
    }
    match Vocabulary::KERNEL.kind(&row.kind) {
        // The party-backed kinds address their `party` row, not their
        // `resource` row, so a resource id is not how they are shared.
        Some(kind) if kind.is_resource => Ok(Object::resource(kind.name, resource_id)),
        _ => decline(),
    }
}

/// Grant a relation on a resource.
pub async fn share<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, args: &Share) -> Outcome<Committed> {
    let (identity, person, acting_as) = refs::actor(&ctx.principal)?;
    let object = object_of(ctx, &args.resource).await?;
    let subject = refs::subject(ctx.store.ids(), &args.subject)?;
    let relation = relation_of(args.relation);

    if !Vocabulary::KERNEL.admits(object.kind, relation, subject.kind) {
        return decline();
    }
    if !may_share(ctx, object, person).await? {
        return decline();
    }
    let now = ctx.now();

    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.push(relation::grant_stmt(
            object,
            relation,
            subject,
            Some(identity),
            now,
        ));
        batch.one(
            AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                Value::from(identity),
                Value::from(acting_as),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                Value::from(object.kind),
                Value::from(object.id),
                Value::from(relation.as_str()),
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(applied.committed)
}

/// Whether this principal may hand out a relation on this object.
///
/// Two rules, because there are two kinds of thing being shared. A resource is
/// shared by somebody who may edit it. A person is shared by that person: no
/// relation on a person makes you able to hand their details on, which is what
/// stops a contact list becoming a directory.
pub(super) async fn may_share<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    object: Object,
    person: crate::ids::Id<crate::ids::Person>,
) -> Outcome<bool> {
    if object.kind == "person" {
        return Ok(object.id == person.get());
    }
    let subjects = expand(ctx.store, &ctx.principal).await?;
    relation::check(ctx.store, &subjects, SHARER, object).await
}
