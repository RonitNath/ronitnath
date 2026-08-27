//! `CreateDocument` — the first product kind, made the way every kind is.
//!
//! Three rows: the `resource` registration, the `document` that hangs off it
//! 1:1, and the owner's relation row. The third is what lets `check()` answer
//! ownership out of the same single query as every other question, and it is
//! written here rather than derived later so the two can never be out of step.

use rn_api::commands::CreateDocument;

use super::{Batch, Ctx, refs, run};
use crate::authority::{self, Want};
use crate::document::{self, BODY_LIMIT, FIRST_REV, TITLE_LIMIT};
use crate::domain::{DEFAULT_ZONE, Vocabulary as _};
use crate::error::{Invalid, Outcome};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{Id, Person};
use crate::org::MemberRole;
use crate::relation::{self, SubjectKind};
use crate::resource;
use crate::store::{Reads, Sql, Value};

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     VALUES ($1, 'create-document', $2, $3, $4, $5, \
             json_object('event', 'create-document', 'document', $6, 'owner', $7))";

/// Create a document in draft, owned by the acting person or by one of their
/// organizations.
pub async fn create_document<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &CreateDocument,
) -> Outcome<Committed> {
    let (identity, person, acting_as) = refs::actor(&ctx.principal)?;
    let title = args.title.trim();
    if title.is_empty() {
        return Err(Invalid::Missing("title").into());
    }
    if title.chars().count() > TITLE_LIMIT {
        return Err(Invalid::TooLong {
            field: "title",
            limit: TITLE_LIMIT,
        }
        .into());
    }
    if args.body.chars().count() > BODY_LIMIT {
        return Err(Invalid::TooLong {
            field: "body",
            limit: BODY_LIMIT,
        }
        .into());
    }

    let (owner, owner_kind): (Id<Person>, SubjectKind) = match &args.owner {
        Some(id) => {
            let org = refs::organization(ctx.store.ids(), id)?;
            let container: Id<Person> = Id::new(org.get());
            // Membership is enough to make something in an organization;
            // administering *who* is in it is the admin's job, not this one's.
            authority::require(
                ctx,
                Want::Role {
                    container,
                    role: MemberRole::Member,
                },
            )
            .await?;
            (container, SubjectKind::Organization)
        }
        None => (person, SubjectKind::Person),
    };
    let now = ctx.now();

    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        let row = batch.one(
            resource::INSERT_SQL,
            vec![
                Value::from(resource::kinds::DOCUMENT),
                Value::from(owner),
                Value::from(DEFAULT_ZONE),
                Value::from("draft"),
                Value::from(now),
            ],
        );
        batch.one(
            document::INSERT_SQL,
            vec![
                row.column("id"),
                Value::from(title),
                Value::from(args.body.as_str()),
                Value::from(FIRST_REV),
            ],
        );
        batch.one(
            relation::GRANT_SQL,
            vec![
                Value::from(resource::kinds::DOCUMENT),
                row.column("id"),
                Value::from(relation::Relation::Owner.as_str()),
                Value::from(owner_kind.as_str()),
                Value::from(owner),
                Value::from(identity),
                Value::from(now),
            ],
        );
        batch.one(
            AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                Value::from(identity),
                Value::from(acting_as),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                row.column("id"),
                Value::from(owner),
            ],
        );
        batch.push(crate::audit::object_stmt(
            ctx.key,
            crate::audit::object::PARTY,
            owner.get(),
        ));
        Ok(batch)
    })
    .await?;
    Ok(applied.committed)
}
