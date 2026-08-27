//! `PublishDocument` — the draft that is there now becomes the published one.
//!
//! Publishing is `published_rev := draft_rev`, which makes "is there an
//! unpublished change" a comparison rather than a flag somebody has to
//! remember to clear. The resource's status moves with it the first time and
//! is already `published` after that, so re-publishing a newer draft is the
//! same command with nothing left to do to the status.
//!
//! The guard is the revision the caller read. Publishing a draft that moved
//! under you is refused, because "publish" means "publish *this*".

use rn_api::commands::PublishDocument;

use super::share::SHARER;
use super::{Batch, Ctx, refs, run};
use crate::bind;
use crate::document;
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::principal::expand;
use crate::relation::{self, Object};
use crate::store::{Reads, Sql};

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'publish-document', $2, $3, $4, $5, \
            json_object('event', 'publish-document', 'document', $6, 'rev', $7) \
     WHERE changes() > 0";

/// Publish a document's current draft.
pub async fn publish_document<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &PublishDocument,
) -> Outcome<Committed> {
    let (identity, _person, acting_as) = refs::actor(&ctx.principal)?;
    let document_id = refs::resource(ctx.store.ids(), &args.document)?;

    let subjects = expand(ctx.store, &ctx.principal).await?;
    let object = Object::document(document_id);
    if !relation::check(ctx.store, &subjects, SHARER, object).await? {
        return decline();
    }
    let Some(row) = document::load(ctx.store, document_id).await? else {
        return decline();
    };
    let rev = row.draft_rev;
    let now = ctx.now();

    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.one(document::PUBLISH_SQL, bind![document_id, rev]);
        batch.one(
            AUDIT,
            bind![
                ctx.key.to_string(),
                identity,
                acting_as,
                now,
                crate::audit::digest_of(args),
                document_id,
                rev
            ],
        );
        // Already `published` after the first time, and a resource that was
        // deleted is not brought back — hence the status in the guard.
        batch.any(document::PUBLISH_RESOURCE_SQL, bind![document_id]);
        Ok(batch)
    })
    .await?;
    Ok(applied.committed)
}
