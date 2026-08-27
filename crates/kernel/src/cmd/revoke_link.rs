//! `RevokeLink` — withdraw an invitation nobody has claimed.
//!
//! An invitation is two rows: the `link` that holds the secret, and the
//! relation that says what holding it is worth (`group:X #member @link:T`).
//! Revoking deletes the relation first, because the relation is the whole of
//! what the token *does* — a link row with no grant opens nothing, and if the
//! second statement somehow did not run, the failure mode is a dead row rather
//! than a live invitation.
//!
//! The link is named by its public id and never by its token. An admin
//! withdrawing somebody else's invitation has no business holding the secret
//! that would let them claim it, and the row keeps only a SHA-256 anyway.
//!
//! A claimed link is refused rather than quietly deleted. Withdrawing an
//! invitation somebody already used would say nothing about their membership,
//! which is what `Leave` and `RemoveMember` are for, and it would take the
//! `claimed_by_identity_id` that the merge lane reads as a signal.

use rn_api::commands::RevokeLink;

use super::{Applied, Batch, Ctx, is_platform_operator, refs, run};
use crate::bind;
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{self, Group, Id, Link};
use crate::org::{self, MemberRole};
use crate::store::{Cursor, FromRow, Reads, RowError, Sql};

/// The container an unclaimed link grants into, and nothing else about it.
///
/// Rides `relation_subject_idx` from the link, which is the same index the
/// claim page's lookup uses in the other direction.
const GRANT: &str = "SELECT rel.object_id AS container_id \
     FROM link l JOIN relation rel ON rel.subject_kind = 'link' AND rel.subject_id = l.id \
     WHERE l.id = $1 AND l.claimed_by_identity_id IS NULL \
       AND rel.object_kind IN ('group', 'organization')";

/// The grant goes first: it is what the token is worth.
const DROP_GRANT: &str = "DELETE FROM relation WHERE subject_kind = 'link' AND subject_id = $1 \
     AND EXISTS (SELECT 1 FROM link WHERE id = $1 AND claimed_by_identity_id IS NULL)";

/// Then the row that holds the secret.
const DROP_LINK: &str = "DELETE FROM link WHERE id = $1 AND claimed_by_identity_id IS NULL";

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'revoke-link', $2, $3, $4, $5, \
            json_object('event', 'revoke-link', 'container', $6, 'link', $7) \
     WHERE changes() > 0";

struct Container(Id<Group>);

impl FromRow for Container {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self(row.id("container_id")?))
    }
}

/// Withdraw an unclaimed invitation.
///
/// An admin or owner of the container it joins may, and so may a platform
/// operator. A member may not: minting invitations is administration, and so
/// is taking one back.
pub async fn revoke_link<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &RevokeLink,
) -> Outcome<Committed> {
    let (identity, person, acting_as) = refs::actor(&ctx.principal)?;
    let Ok(link) = ids::decode::<Link>(ctx.store.ids(), &args.link) else {
        return decline();
    };
    let Some(Container(container)) = ctx.store.query_opt::<Container>(GRANT, bind![link]).await?
    else {
        // Gone, claimed, or never there: one answer for all three.
        return decline();
    };
    let party: Id<crate::ids::Person> = Id::new(container.get());
    if !org::holds(ctx.store, party, person, MemberRole::Admin).await?
        && !is_platform_operator(ctx.store, person).await?
    {
        return decline();
    }
    let now = ctx.now();

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.one(DROP_GRANT, bind![link]);
        batch.one(DROP_LINK, bind![link]);
        batch.one(
            AUDIT,
            bind![
                ctx.key.to_string(),
                identity,
                acting_as,
                now,
                crate::audit::digest_of(args),
                container,
                link
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(committed)
}
