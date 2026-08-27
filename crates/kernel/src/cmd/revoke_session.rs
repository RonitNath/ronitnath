//! `RevokeSession` — end a session that is not the one you are using.
//!
//! Two principals may run it, and they are not the same rule:
//!
//! * **the person whose session it is.** The device list, and the lost laptop.
//! * **a platform operator.** Someone has to be able to end a session on a
//!   compromised account without owning it, and platform administration is a
//!   relation (`platform:* #operator @person`) rather than a column — the same
//!   row `Disable` and `Enable` ask about.
//!
//! Whichever it is, the delete is guarded on the identity the row was *read*
//! as holding rather than on the caller's, so a session that changed hands
//! between the read and the write matches nothing and the caller is declined.
//! And the audit row names the identity the session belonged to, not the one
//! that ended it — an operator's revocation that recorded the operator's own
//! identity would be an audit log that cannot answer "who was signed out".

use rn_api::commands::RevokeSession;

use super::{Applied, Batch, Ctx, refs, run};
use crate::authority::{self, Want};
use crate::bind;
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids;
use crate::ids::{Id, Identity, Session};
use crate::oidc::token as oidc;
use crate::store::{Cursor, FromRow, Reads, RowError, Sql};

/// Whose session it is. One indexed read by primary key, and also the check
/// that the id names a live row at all.
const OWNER: &str = "SELECT identity_id FROM session WHERE id = $1";

const DELETE: &str = "DELETE FROM session WHERE id = $1 AND identity_id = $2";

const AUDIT: &str = "INSERT INTO audit \
                     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
                     SELECT $1, 'revoke-session', $2, $3, $4, $5, \
                            json_object('event', 'revoke-session', 'identity', $6, \
                                        'session', $7, 'clients', json($8)) \
                     WHERE changes() > 0";

struct Owner(Id<Identity>);

impl FromRow for Owner {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self(row.id("identity_id")?))
    }
}

/// End a session: your own device, or anybody's if you operate the platform.
pub async fn revoke_session<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &RevokeSession,
) -> Outcome<Committed> {
    let (identity, person, acting_as) = refs::actor(&ctx.principal)?;
    let Ok(target) = ids::decode::<Session>(ctx.store.ids(), &args.session) else {
        // A forged or foreign id is refused without a query, and refused the
        // same way a real id belonging to somebody else would be.
        return decline();
    };
    let Some(Owner(whose)) = ctx.store.query_opt::<Owner>(OWNER, bind![target]).await? else {
        return decline();
    };
    let _ = person;
    authority::require(ctx, Want::Settled(whose == identity)).await?;
    let now = ctx.now();
    let targets = oidc::logout_targets(&ctx.store.reads(), target).await?;
    let clients = oidc::target_ids_json(&targets);

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.any(oidc::DELETE_SESSION_TOKENS_SQL, bind![target]);
        batch.any(oidc::DELETE_SESSION_CODES_SQL, bind![target]);
        batch.one(DELETE, bind![target, whose]);
        batch.one(
            AUDIT,
            bind![
                ctx.key.to_string(),
                identity,
                acting_as,
                now,
                crate::audit::digest_of(args),
                whose,
                target,
                clients.as_str()
            ],
        );
        batch.push(crate::audit::object_stmt(
            ctx.key,
            crate::audit::object::SESSION,
            target.get(),
        ));
        batch.push(crate::audit::object_stmt(
            ctx.key,
            crate::audit::object::IDENTITY,
            whose.get(),
        ));
        Ok(batch)
    })
    .await?;
    Ok(committed)
}
