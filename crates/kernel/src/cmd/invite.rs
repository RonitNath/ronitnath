//! `Invite` — mint a bearer link that carries one role into one container.
//!
//! The link row holds a secret and an expiry and nothing else; what the token
//! is *worth* is a relation row, `group:X #member @link:T`. Both are written
//! in one batch, because a link with no grant is a token that opens nothing
//! and a grant with no link is a row that can never be claimed.
//!
//! The token exists in the clear once, in this command's reply. A replay of
//! the same idempotency key returns the event and no token — the row keeps
//! only a SHA-256, and reproducing the secret would mean storing it.

use rn_api::commands::Invite;

use super::{Batch, Ctx, Minted, refs, run};
use crate::authority::{self, Want};
use crate::domain::{Token, Vocabulary as _};
use crate::error::{Outcome, decline};
use crate::feed::Feed;
use crate::invite::{self, MAX_TTL};
use crate::org::MemberRole;
use crate::relation::{self, Object, SubjectKind};
use crate::store::{Reads, Sql, Value};

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     VALUES ($1, 'invite', $2, $3, $4, $5, \
             json_object('event', 'invite', 'container', $6, 'link', $7))";

/// Mint an invitation into a group or an organization.
///
/// `owner` is refused: ownership moves through `Transfer` and nowhere else,
/// and an invitation that conferred it would be a second way.
pub async fn invite<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, args: &Invite) -> Outcome<Minted> {
    let (identity, person, acting_as) = refs::actor(&ctx.principal)?;
    let container = refs::container(ctx.store.ids(), &args.group)?;
    let role = MemberRole::from(args.role);
    if role == MemberRole::Owner {
        return decline();
    }
    let _ = person;
    authority::require(
        ctx,
        Want::Role {
            container: crate::ids::Id::new(container.get()),
            role: MemberRole::Admin,
        },
    )
    .await?;

    let now = ctx.now();
    // The caller's expiry, capped. An invitation that never runs out is a
    // password with a friendly name.
    let expires_at = args.expires_at.min(now + MAX_TTL);
    if expires_at <= now {
        return decline();
    }

    // The container's kind decides which object the grant is on, and a
    // container that does not exist has no kind — so this read is also the
    // check that the id names something.
    let object =
        match crate::resource::of_party(ctx.store, crate::ids::Id::new(container.get())).await? {
            Some(row) if row.kind == crate::resource::kinds::ORGANIZATION => {
                Object::organization(crate::ids::Id::new(container.get()))
            }
            Some(row) if row.kind == crate::resource::kinds::GROUP => Object::group(container),
            _ => return decline(),
        };

    let token = Token::mint();
    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        let link = batch.one(
            invite::INSERT_SQL,
            vec![
                Value::from(token.digest().to_vec()),
                Value::from(expires_at),
                Value::from(now),
            ],
        );
        batch.one(
            relation::GRANT_SQL,
            vec![
                Value::from(object.kind),
                Value::from(object.id),
                Value::from(role.relation().as_str()),
                Value::from(SubjectKind::Link.as_str()),
                link.column("id"),
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
                Value::from(object.id),
                link.column("id"),
            ],
        );
        batch.push(crate::audit::object_stmt(
            ctx.key,
            crate::audit::object::PARTY,
            object.id,
        ));
        Ok(batch)
    })
    .await?;

    Ok(Minted {
        token: applied.fresh.then_some(token),
        committed: applied.committed,
    })
}
