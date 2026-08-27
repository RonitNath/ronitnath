//! `EditDocument` — a guarded write against the revision the client read.
//!
//! The client sends `expected_rev`: the `draft_rev` it was looking at when the
//! person started typing. The update carries it in its `WHERE`, so two people
//! editing the same draft produce one write and one decline rather than one
//! write that silently ate the other. There is no merge here and there is not
//! meant to be — rung 12's Loro documents are what a real concurrent editor
//! looks like, and pretending in the meantime would be worse than refusing.
//!
//! Sending neither a title nor a body is refused rather than treated as a
//! no-op: it would still bump the revision, and a revision that means nothing
//! is a revision every other client has to fetch to discover that.

use rn_api::commands::EditDocument;

use super::share::SHARER;
use super::{Batch, Ctx, refs, run};
use crate::authority::{self, Want};
use crate::bind;
use crate::document::{self, BODY_LIMIT, TITLE_LIMIT};
use crate::error::{Invalid, Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::relation::Object;
use crate::store::{Reads, Sql, Value};

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'edit-document', $2, $3, $4, $5, \
            json_object('event', 'edit-document', 'document', $6, 'rev', $7) \
     WHERE changes() > 0";

/// Edit a document's draft.
pub async fn edit_document<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &EditDocument,
) -> Outcome<Committed> {
    let (identity, _person, acting_as) = refs::actor(&ctx.principal)?;
    let document_id = refs::resource(ctx.store.ids(), &args.document)?;
    if args.title.is_none() && args.body.is_none() {
        return Err(Invalid::Missing("a title or a body").into());
    }
    if let Some(title) = &args.title {
        if title.trim().is_empty() {
            return Err(Invalid::Missing("title").into());
        }
        if title.chars().count() > TITLE_LIMIT {
            return Err(Invalid::TooLong {
                field: "title",
                limit: TITLE_LIMIT,
            }
            .into());
        }
    }
    if let Some(body) = &args.body
        && body.chars().count() > BODY_LIMIT
    {
        return Err(Invalid::TooLong {
            field: "body",
            limit: BODY_LIMIT,
        }
        .into());
    }

    let object = Object::document(document_id);
    authority::require(
        ctx,
        Want::On {
            relation: SHARER,
            object,
        },
    )
    .await?;
    // A document row that is not there is the same answer as one that is not
    // yours, and the revision guard would decline it anyway — reading it here
    // is what makes the event carry the revision the edit produced.
    let Some(row) = document::load(ctx.store, document_id).await? else {
        return decline();
    };
    if row.draft_rev != args.expected_rev {
        return decline();
    }
    let now = ctx.now();
    let next = args.expected_rev + 1;

    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.one(
            document::EDIT_SQL,
            vec![
                Value::from(args.title.as_deref().map(str::trim)),
                Value::from(args.body.as_deref()),
                Value::from(document_id),
                Value::from(args.expected_rev),
            ],
        );
        batch.one(
            AUDIT,
            bind![
                ctx.key.to_string(),
                identity,
                acting_as,
                now,
                crate::audit::digest_of(args),
                document_id,
                next
            ],
        );
        batch.push(crate::audit::object_stmt(
            ctx.key,
            crate::audit::object::RESOURCE,
            document_id.get(),
        ));
        Ok(batch)
    })
    .await?;
    Ok(applied.committed)
}
