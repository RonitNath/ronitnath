//! `SignIn` — exchange a password factor for a session.
//!
//! Every way this can fail produces the same answer and, as near as matters,
//! the same latency. An unknown address, a disabled identity, a disabled
//! person, a wrong password: one [`Decline`](crate::Decline), and the
//! unknown-address path
//! spends an argon2 verification against a dummy hash so it does not answer
//! faster than the others and tell a caller which addresses are registered.
//! All three verifications go to the blocking pool, so the work that makes
//! them uniform does not stall the reactor serving everybody else.

use rn_api::commands::SignIn;

use super::{Applied, Batch, Ctx, Minted, run};
use crate::bind;
use crate::domain::{SESSION_TTL, Token, normalize_email};
use crate::error::{Outcome, decline};
use crate::feed::Feed;
use crate::ids::{Id, Identity, Person};
use crate::password;
use crate::store::{Cursor, FromRow, Reads, RowError, Sql, Value};

/// One indexed lookup answering "who is this address, and can they sign in".
///
/// The partial unique index on `factor(value) WHERE kind = 'email'` is what
/// makes this a seek; `explain_sign_in` asserts it.
pub const LOOKUP: &str = "SELECT i.id AS identity_id, i.person_id, \
                             CASE WHEN i.status = 'active' \
                                   AND coalesce(who.status, 'active') = 'active' \
                                  THEN 'active' ELSE 'disabled' END AS status, \
                             p.value AS phc \
                      FROM factor e \
                      JOIN identity i ON i.id = e.identity_id \
                      LEFT JOIN party who ON who.id = i.person_id \
                      LEFT JOIN factor p ON p.identity_id = i.id AND p.kind = 'password' \
                      WHERE e.kind = 'email' AND e.value = $1";

/// The guard is the whole rule, restated where it is transactional: the read
/// above decides, and this is what makes the decision hold against a `Disable`
/// that commits between the two.
const SESSION: &str = "INSERT INTO session \
                       (identity_id, acting_as, token_hash, expires_at, created_at, last_seen_at) \
                       SELECT $1, $2, $3, $4, $5, $5 \
                       WHERE EXISTS (SELECT 1 FROM identity i \
                                     LEFT JOIN party who ON who.id = i.person_id \
                                     WHERE i.id = $1 AND i.status = 'active' \
                                       AND coalesce(who.status, 'active') = 'active') \
                       RETURNING id";

const AUDIT: &str = "INSERT INTO audit \
                     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
                     SELECT $1, 'sign-in', $2, $3, $4, $5, \
                            json_object('event', 'sign-in', 'identity', $2, 'session', $6) \
                     WHERE changes() > 0";

struct Candidate {
    identity_id: Id<Identity>,
    person_id: Option<Id<Person>>,
    status: String,
    phc: Option<String>,
}

impl FromRow for Candidate {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            identity_id: row.id("identity_id")?,
            person_id: row.id_opt("person_id")?,
            status: row.text("status")?,
            phc: row.text_opt("phc")?,
        })
    }
}

/// What a successful sign-in produced.
pub type SignedIn = Minted;

/// Mint a session for an identity that proved its password.
pub async fn sign_in<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, args: &SignIn) -> Outcome<SignedIn> {
    let email = normalize_email(&args.email);
    let candidate = ctx
        .store
        .query_opt::<Candidate>(LOOKUP, bind![email.as_str()])
        .await?;

    // One shape for every refusal, and the same work done on each path.
    // `status` is the *pair*: `Disable` moves `party.status`, never
    // `identity.status`, so an identity that reads active on its own row can
    // still belong to a person this deployment has disabled — and a disable
    // that only deleted the open sessions would be an account that signs
    // straight back in.
    let Some(candidate) = candidate.filter(|c| c.status == "active") else {
        password::verify_dummy_blocking(&args.password).await;
        return decline();
    };
    let Some(phc) = candidate.phc.as_deref() else {
        password::verify_dummy_blocking(&args.password).await;
        return decline();
    };
    if !password::verify_blocking(&args.password, phc).await {
        return decline();
    }

    // A person always exists for a registered identity; an unresolved one
    // acts as itself until a merge says otherwise, which cannot happen in
    // this leg. Falling back keeps `acting_as` NOT NULL honest.
    let acting_as = candidate
        .person_id
        .unwrap_or(Id::<Person>::new(candidate.identity_id.get()));
    let token = Token::mint();
    let now = ctx.now();

    let Applied { committed, fresh } = run(ctx, args, async || {
        let mut batch = Batch::new();
        let session = batch.one(
            SESSION,
            vec![
                Value::from(candidate.identity_id),
                Value::from(acting_as),
                Value::from(token.digest()),
                Value::from(now + SESSION_TTL),
                Value::from(now),
            ],
        );
        batch.one(
            AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                Value::from(candidate.identity_id),
                Value::from(acting_as),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                session.column("id"),
            ],
        );
        Ok(batch)
    })
    .await?;

    Ok(Minted {
        committed,
        token: fresh.then_some(token),
    })
}
