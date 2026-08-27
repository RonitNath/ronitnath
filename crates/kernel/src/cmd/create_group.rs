//! `CreateGroup` — a group in an organization, or one a person keeps.
//!
//! The difference between the two is one column: `resource.owner_party_id` is
//! the organization or the person. There is no "personal" flag, because a flag
//! and an owner column could disagree and then something would have to decide
//! which of them meant it.
//!
//! Creating a group inside an organization needs `admin` there. Membership of
//! an organization is not enough: a group is a grantable subject, so anyone who
//! can make one can make an audience, and that is an administrative act.

use rn_api::commands::CreateGroup;

use super::{Batch, Ctx, refs, run};
use crate::bind;
use crate::domain::{DEFAULT_ZONE, DISPLAY_NAME_LIMIT, Vocabulary as _};
use crate::error::{Invalid, Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{Id, Person};
use crate::org::{self, MemberRole};
use crate::resource;
use crate::store::{Reads, Sql, Value};

const PARTY: &str = "INSERT INTO party (kind, display_name, status, created_at) \
                     VALUES ('group', $1, 'active', $2) RETURNING id";

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     VALUES ($1, 'create-group', $2, $3, $4, $5, \
             json_object('event', 'create-group', 'group', $6, \
                         'resource', $7, 'owner', $8))";

/// Create a group. Its creator is its first owner either way; who *owns* it is
/// the organization when there is one and the creator when there is not.
pub async fn create_group<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &CreateGroup,
) -> Outcome<Committed> {
    let (identity, person, acting_as) = refs::actor(&ctx.principal)?;
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

    let owner: Id<Person> = match &args.organization {
        Some(id) => {
            let org = refs::organization(ctx.store.ids(), id)?;
            let container = Id::new(org.get());
            if !org::holds(ctx.store, container, person, MemberRole::Admin).await? {
                return decline();
            }
            container
        }
        None => person,
    };
    let now = ctx.now();

    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        let party = batch.one(PARTY, bind![display_name, now]);
        let row = batch.one(
            resource::INSERT_SQL,
            vec![
                Value::from(resource::kinds::GROUP),
                Value::from(owner),
                Value::from(DEFAULT_ZONE),
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
                Value::from(acting_as),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                party.column("id"),
                row.column("id"),
                Value::from(owner),
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(applied.committed)
}
