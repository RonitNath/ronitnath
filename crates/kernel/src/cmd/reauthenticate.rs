//! `ReAuthenticate` — present the password again, without minting anything.
//!
//! The session is already signed in and stays exactly as it was: same row,
//! same cookie, same expiry. What moves is `session.auth_time`, which is the
//! only thing the sensitive commands measure their window against
//! ([`crate::authority::fresh_enough`]).
//!
//! It is deliberately not a second `SignIn`. Minting a new session would mean
//! a new secret in a reply, a `Set-Cookie` on a request that was not a
//! sign-in, and — the part that matters — an operator's *old* session left
//! alive beside the new one, which is one more fourteen-day cookie than
//! anybody asked for.
//!
//! Every refusal is the uniform decline and every path does the same argon2
//! work, for the reason [`crate::cmd::sign_in`]'s do: a caller that could tell
//! "wrong password" from "no password factor" by timing would be learning
//! about the account rather than about their own typing.
//!
//! Refused from an impersonated session. An operator wearing somebody's hat
//! who could re-authenticate *as them* would need their password, which they
//! do not have — but the interesting case is the other one: the command would
//! otherwise refresh the target's window using the operator's freshness, and a
//! hat is not a proof of anything about the head under it.

use rn_api::commands::ReAuthenticate;

use super::{Applied, Batch, Ctx, member, refs, run};
use crate::audit::{self, object};
use crate::authority;
use crate::bind;
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::password;
use crate::store::{Cursor, FromRow, Reads, RowError, Sql};

/// The acting identity's password hash. One seek on `factor_identity_idx`.
const PASSWORD: &str =
    "SELECT value AS phc FROM factor WHERE identity_id = $1 AND kind = 'password'";

/// The guard is the identity the session was read as holding: a session that
/// changed hands between the read and the write is not this caller's.
const TOUCH: &str = "UPDATE session SET auth_time = $1 WHERE id = $2 AND identity_id = $3";

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 're-authenticate', $2, $3, $4, $5, \
            json_object('event', 're-authenticate', 'identity', $6, 'session', $7) \
     WHERE changes() > 0";

struct Phc(String);

impl FromRow for Phc {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self(row.text("phc")?))
    }
}

/// What the arguments digest is taken over.
///
/// **Not the command's own arguments.** [`run`] digests whatever it is handed
/// into `audit.request_digest`, and the only thing this command carries is a
/// password — so handing it the real arguments would put a SHA-256 of
/// somebody's password in a table that is also the change feed and has no
/// retention. The identity and the session are what make two calls under one
/// idempotency key the same call, and neither is a secret.
///
/// `crate::dev::DevSignIn` does the same for the same reason.
#[derive(Debug, serde::Serialize)]
struct Redacted {
    identity: i64,
    session: i64,
}

/// Prove the password again on the session that is already signed in.
pub async fn reauthenticate<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &ReAuthenticate,
) -> Outcome<Committed> {
    authority::not_impersonating(&ctx.principal)?;
    let (identity, _, session) = member(&ctx.principal)?;
    let (actor, _, acting_as) = refs::actor(&ctx.principal)?;

    let phc = ctx
        .store
        .query_opt::<Phc>(PASSWORD, bind![identity])
        .await?;
    let Some(Phc(phc)) = phc else {
        // An identity with no password factor — an imported one, or one whose
        // only factor is an address. The same work and the same answer as a
        // wrong password.
        password::verify_dummy_blocking(&args.password).await;
        return decline();
    };
    if !password::verify_blocking(&args.password, &phc).await {
        return decline();
    }
    let now = ctx.now();
    let redacted = Redacted {
        identity: identity.get(),
        session: session.get(),
    };

    let Applied { committed, .. } = run(ctx, &redacted, async || {
        let mut batch = Batch::new();
        batch.one(TOUCH, bind![now, session, identity]);
        batch.one(
            AUDIT,
            bind![
                ctx.key.to_string(),
                actor,
                acting_as,
                now,
                audit::digest_of(&redacted),
                identity,
                session
            ],
        );
        batch.push(audit::object_stmt(ctx.key, object::SESSION, session.get()));
        Ok(batch)
    })
    .await?;
    Ok(committed)
}
