//! `VerifyEmail` — prove an address with the token that was mailed to it.
//!
//! Holding the token *is* the authorisation, which is why this command accepts
//! an anonymous principal: the person clicking the link in their mail client
//! may well not be signed in, and requiring a session would make the link
//! useless in exactly the case it exists for. What the token proves is
//! delivery to that address, and that is all it is allowed to do — the link
//! carries no session and mints none.

use rn_api::commands::VerifyEmail;

use super::{Applied, Batch, Ctx, run};
use crate::bind;
use crate::domain::Token;
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{Factor, Id, Identity, Link};
use crate::store::{Cursor, FromRow, Reads, RowError, Sql};

const LOOKUP: &str = "SELECT l.id AS link_id, l.expires_at, l.claimed_by_identity_id, \
                             f.id AS factor_id, f.identity_id \
                      FROM link l JOIN factor f ON f.id = l.verifies_factor_id \
                      WHERE l.token_hash = $1";

/// Both mutations carry the same guard — the link is unclaimed and unexpired —
/// so a token used twice changes nothing rather than half of something.
const FACTOR: &str = "UPDATE factor SET verified_at = $1 WHERE id = $2 \
                      AND EXISTS (SELECT 1 FROM link WHERE id = $3 \
                                  AND claimed_by_identity_id IS NULL AND expires_at > $1)";

const LINK: &str = "UPDATE link SET claimed_by_identity_id = $1, claimed_at = $2 \
                    WHERE id = $3 AND claimed_by_identity_id IS NULL AND expires_at > $2";

const AUDIT: &str = "INSERT INTO audit \
                     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
                     SELECT $1, 'verify-email', $2, NULL, $3, $4, \
                            json_object('event', 'verify-email', 'identity', $2, 'factor', $5) \
                     WHERE changes() > 0";

struct Claim {
    link_id: Id<Link>,
    factor_id: Id<Factor>,
    identity_id: Id<Identity>,
    expires_at: crate::Timestamp,
    claimed: bool,
}

impl FromRow for Claim {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            link_id: row.id("link_id")?,
            expires_at: row.int("expires_at")?,
            claimed: row.int_opt("claimed_by_identity_id")?.is_some(),
            factor_id: row.id("factor_id")?,
            identity_id: row.id("identity_id")?,
        })
    }
}

/// Mark an email factor verified, and mark its link claimed.
pub async fn verify_email<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &VerifyEmail,
) -> Outcome<Committed> {
    let token = Token::from_wire(&args.token);
    let now = ctx.now();
    let claim = ctx
        .store
        .query_opt::<Claim>(LOOKUP, bind![token.digest().to_vec()])
        .await?;
    // Unknown, expired and already claimed are one answer: a token that does
    // not work now. Telling them apart would say whether an address is
    // registered here.
    let Some(claim) = claim.filter(|c| !c.claimed && c.expires_at > now) else {
        return decline();
    };

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.one(FACTOR, bind![now, claim.factor_id, claim.link_id]);
        batch.one(LINK, bind![claim.identity_id, now, claim.link_id]);
        batch.one(
            AUDIT,
            bind![
                ctx.key.to_string(),
                claim.identity_id,
                now,
                crate::audit::digest_of(args),
                claim.factor_id
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(committed)
}

/// Mint a verification link for an email factor.
///
/// Not a command: nothing about it is a decision, and it is the mailer's
/// trigger rather than a person's action. It is here because the token it
/// returns is the one [`verify_email`] consumes, and the two belong in one
/// file for the same reason a lock and its key do.
pub async fn mint_verification<S: Sql>(
    store: &crate::store::Store<S>,
    factor: Id<Factor>,
) -> Outcome<Token> {
    let token = Token::mint();
    let now = store.clock().now();
    store
        .execute(
            "INSERT INTO link (token_hash, expires_at, verifies_factor_id, created_at) \
             VALUES ($1, $2, $3, $4)",
            bind![
                token.digest().to_vec(),
                now + crate::domain::VERIFY_TTL,
                factor,
                now
            ],
        )
        .await?;
    Ok(token)
}
