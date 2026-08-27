//! `RotateSigningKey` — mint a key, and start retiring the one before it.
//!
//! Rotation is a non-event for a relying party, and the `retiring` status is
//! why: the previous key stops signing and keeps verifying, and stays in the
//! JWKS, so an RP holding a cached document and a token signed a minute ago
//! finds the key it needs. Retiring it for good is a second, later decision —
//! `retiring` → `retired` — taken when nothing signed under it is still alive.

use rn_api::commands::RotateSigningKey;

use super::operator;
use crate::cmd::{Applied, Batch, Ctx, run};
use crate::error::Outcome;
use crate::event::Committed;
use crate::feed::Feed;
use crate::oidc::key;
use crate::store::{Sql, Value};

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     VALUES ($1, 'rotate-signing-key', $2, $3, $4, $5, \
             json_object('event', 'rotate-signing-key', 'kid', $6))";

/// Mint the key new tokens will be signed under.
pub async fn rotate_signing_key<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &RotateSigningKey,
) -> Outcome<Committed> {
    let (identity, _, acting_as) = operator(ctx).await?;
    let seal = ctx.provider.seal().clone();
    // RSA key generation is tens to hundreds of milliseconds of arithmetic.
    // It goes to the blocking pool for the reason `argon2` does: the reactor
    // is serving everybody else, and this is the one command that is not I/O
    // bound at all.
    let minted = tokio::task::spawn_blocking(move || key::mint(&seal))
        .await
        .map_err(|err| crate::KernelError::Invariant(format!("key generation: {err}")))??;
    let now = ctx.now();

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        // Before the insert, in the same transaction, so there is never a
        // moment at which two keys are active.
        batch.any(key::RETIRE_ACTIVE_SQL, vec![]);
        batch.one(
            key::INSERT_SQL,
            vec![
                Value::from(minted.kid.as_str()),
                Value::from(minted.modulus.as_str()),
                Value::from(minted.exponent.as_str()),
                Value::from(minted.sealed.clone()),
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
                Value::from(minted.kid.as_str()),
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(committed)
}
