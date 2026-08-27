//! `RetireKey` — take a signing key out of the JWKS for good.
//!
//! Rotation gives a key three lives and only two of them were reachable.
//! `RotateSigningKey` moves the active key to `retiring`, which is the overlap
//! that makes a rotation a non-event for a relying party: the key stops
//! signing and keeps verifying, and stays published. Nothing ever wrote
//! `retired`, so `retiring` was terminal in practice and the JWKS grew by one
//! key per rotation forever.
//!
//! This is the second decision, and it is a decision rather than a cleanup:
//! retiring a key is what makes every token signed under it unverifiable. So
//! the command refuses while any are still alive, and the override says so out
//! loud — `force: true` with a reason that lands in the audit row, because
//! "we broke every session issued under that key on purpose" is exactly the
//! sentence somebody will want to find afterwards.
//!
//! ## What "signed under this key" means
//!
//! A token does not record its `kid`. What it records is when it was issued,
//! and exactly one key is `active` at a time — so a token was signed under
//! this key if it was created after this key and before the key that replaced
//! it. That is a range over `oidc_token.created_at`, bounded by two reads of
//! `oidc_key`, and it is a count of rows the deployment holds rather than an
//! estimate.

use rn_api::commands::RetireKey;

use crate::audit::{self, object};
use crate::bind;
use crate::cmd::{Applied, Batch, Ctx, run};
use crate::error::{Invalid, Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{Id, OidcKey};
use crate::merge::rule::EVIDENCE_LIMIT;
use crate::oidc::key;
use crate::store::{Count, Reads, Sql, Value};

use super::operator;

/// The key, if it is one this command can act on. `retiring` only: an `active`
/// key is what everything is being signed under right now, and a `retired` one
/// is already gone.
const RETIRING: &str = "SELECT id AS n FROM oidc_key WHERE kid = $1 AND status = 'retiring'";

/// How many live tokens were signed under a key.
///
/// The lower bound is this key's own `created_at`; the upper is the next key's,
/// or the end of time if it is somehow the newest. `expires_at > $2` is what
/// puts this on `oidc_token_expires_idx` — a range seek rather than a pass
/// over every token the deployment has ever issued, which is what the count
/// would otherwise be on the one screen an operator opens to decide something.
pub const ALIVE_SQL: &str = "SELECT count(*) AS n FROM oidc_token t \
     WHERE t.created_at >= (SELECT created_at FROM oidc_key WHERE id = $1) \
       AND t.created_at < coalesce( \
             (SELECT min(created_at) FROM oidc_key \
              WHERE created_at > (SELECT created_at FROM oidc_key WHERE id = $1)), \
             9223372036854775807) \
       AND t.expires_at > $2 AND t.revoked_at IS NULL";

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'retire-key', $2, $3, $4, $5, \
            json_object('event', 'retire-key', 'kid', $6, 'forced', json($7), \
                        'alive', $8, 'reason', $9) \
     WHERE changes() > 0";

/// Move a `retiring` key to `retired`.
pub async fn retire_key<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &RetireKey,
) -> Outcome<Committed> {
    crate::authority::not_impersonating(&ctx.principal)?;
    let (identity, _, acting_as) = operator(ctx).await?;
    crate::authority::fresh_enough(ctx).await?;
    let reason = reason(&args.reason)?;

    let Some(row) = ctx
        .store
        .reads()
        .query_opt::<Count>(RETIRING, bind![args.kid.as_str()])
        .await?
    else {
        // Unknown, already retired, or still the active one: a caller learns
        // which keys exist from the JWKS, and learns nothing more here.
        return decline();
    };
    let id: Id<OidcKey> = Id::new(row.0);
    let now = ctx.now();
    let alive = ctx
        .store
        .reads()
        .query::<Count>(ALIVE_SQL, bind![id, now])
        .await?
        .first()
        .map_or(0, |count| count.0);
    if alive > 0 && !args.force {
        return decline();
    }

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.one(key::RETIRE_SQL, bind![now, args.kid.as_str()]);
        batch.one(
            AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                Value::from(identity),
                Value::from(acting_as),
                Value::from(now),
                Value::from(audit::digest_of(args)),
                Value::from(args.kid.as_str()),
                // `json($7)` so the payload carries a boolean and not the
                // string "true": a consumer reading it back with
                // `as_bool` would find `None` and quietly call a forced
                // retire an ordinary one.
                Value::from(if args.force { "true" } else { "false" }),
                Value::from(alive),
                Value::from(reason),
            ],
        );
        batch.push(audit::object_stmt(ctx.key, object::KEY, id.get()));
        Ok(batch)
    })
    .await?;
    Ok(committed)
}

/// The stated reason, bounded like `RuleMatch`'s evidence.
fn reason(raw: &str) -> Outcome<&str> {
    let reason = raw.trim();
    if reason.is_empty() {
        return Err(Invalid::Missing("reason").into());
    }
    if reason.chars().count() > EVIDENCE_LIMIT {
        return Err(Invalid::TooLong {
            field: "reason",
            limit: EVIDENCE_LIMIT,
        }
        .into());
    }
    Ok(reason)
}
