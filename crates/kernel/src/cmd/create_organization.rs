//! `CreateOrganization` — a party, a resource, a join row and an owner, at once.
//!
//! An organization is not one row (`crate::org` spells the shape out), and the
//! four it is are useless apart: a `party` nobody owns cannot be transferred,
//! a `resource` with no party has nothing to grant, and an organization with
//! no owner is one nobody can administer. So they are one batch, and a
//! proptest injects a failing statement to prove the rollback.

use rn_api::commands::CreateOrganization;

use super::{Batch, Ctx, refs, run};
use crate::bind;
use crate::domain::{DEFAULT_ZONE, DISPLAY_NAME_LIMIT, Vocabulary as _};
use crate::error::{Invalid, Outcome};
use crate::event::Committed;
use crate::feed::Feed;
use crate::org::{self, MemberRole};
use crate::resource;
use crate::store::{Sql, Value};

const PARTY: &str = "INSERT INTO party (kind, display_name, status, created_at) \
                     VALUES ('organization', $1, 'active', $2) RETURNING id";

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     VALUES ($1, 'create-organization', $2, $3, $4, $5, \
             json_object('event', 'create-organization', 'organization', $6, \
                         'resource', $7, 'owner', $3))";

/// Found an organization. The acting person owns it and administers it, and
/// ownership moves from there only through `Transfer`.
pub async fn create_organization<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &CreateOrganization,
) -> Outcome<Committed> {
    let (identity, person) = refs::actor(&ctx.principal)?;
    let display_name = args.display_name.trim();
    if display_name.is_empty() {
        return Err(Invalid::Missing("display name").into());
    }
    if display_name.chars().count() > DISPLAY_NAME_LIMIT {
        return Err(Invalid::TooLong {
            field: "display name",
            limit: DISPLAY_NAME_LIMIT,
        }
        .into());
    }
    let now = ctx.now();

    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        let party = batch.one(PARTY, bind![display_name, now]);
        let row = batch.one(
            resource::INSERT_SQL,
            vec![
                Value::from(resource::kinds::ORGANIZATION),
                Value::from(person),
                Value::from(DEFAULT_ZONE),
                // An organization is live the moment it exists: no command
                // produces a draft one, so no draft one can exist.
                Value::from("published"),
                Value::from(now),
            ],
        );
        batch.one(
            resource::INSERT_PARTY_RESOURCE_SQL,
            vec![row.column("id"), party.column("id")],
        );
        batch.one(
            org::INSERT_SQL,
            vec![
                party.column("id"),
                Value::from(person),
                Value::from(MemberRole::Owner.as_str()),
                Value::from(now),
            ],
        );
        batch.one(
            AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                Value::from(identity),
                Value::from(person),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                party.column("id"),
                row.column("id"),
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(applied.committed)
}
