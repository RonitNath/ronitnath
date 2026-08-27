//! `Share` — one relation, one subject, one row.
//!
//! Two gates, and they answer different questions.
//!
//! **May this principal share it at all?** `editor` or better on the object.
//! Not "may they view it": a viewer who could share would make every grant
//! transitive, and the person who granted the view would have no way to know.
//! `owner` covers `editor`, so an owner passes without a second rule.
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
             json_object('event', 'share', 'resource', $6, 'relation', $7))";

/// The relation a caller must hold to hand one out.
pub const SHARER: Relation = Relation::Editor;

/// Turn a wire role into a relation. The wire cannot name `owner`, and this is
/// where that stays true.
pub(super) const fn relation_of(role: rn_api::whoami::DocRole) -> Relation {
    match role {
        rn_api::whoami::DocRole::Viewer => Relation::Viewer,
        rn_api::whoami::DocRole::Commenter => Relation::Commenter,
        rn_api::whoami::DocRole::Editor => Relation::Editor,
    }
}

/// Resolve the object a sharing command names, refusing anything that is not a
/// live resource of a registered, resource-backed kind.
pub(super) async fn object_of<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    id: &rn_api::ids::PublicId,
) -> Outcome<Object> {
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
    let (identity, person) = refs::actor(&ctx.principal)?;
    let object = object_of(ctx, &args.resource).await?;
    let subject = refs::subject(ctx.store.ids(), &args.subject)?;
    let relation = relation_of(args.relation);

    if !Vocabulary::KERNEL.admits(object.kind, relation, subject.kind) {
        return decline();
    }
    let subjects = expand(ctx.store, &ctx.principal).await?;
    if !relation::check(ctx.store, &subjects, SHARER, object).await? {
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
                Value::from(person),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                Value::from(object.id),
                Value::from(relation.as_str()),
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(applied.committed)
}
