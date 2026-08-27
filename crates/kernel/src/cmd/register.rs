//! `Register` — a person, a registration, its factors and its session, at once.
//!
//! Atomic is not a nice-to-have here. A registration that left a party row
//! without an identity, or an identity without a password, would be a human
//! who cannot sign in and cannot register again, because the address is taken.
//! The pre-rebuild code did this as five statements outside a transaction and
//! said so in a comment; this is one batch, and the proptest injects a failing
//! statement to prove the rollback rather than trusting the word "transaction".

use rn_api::commands::Register;

use super::{Batch, Ctx, Minted, run};
use crate::bind;
use crate::domain::{
    DEFAULT_SOURCE, DEFAULT_ZONE, DISPLAY_NAME_LIMIT, EMAIL_LIMIT, PASSWORD_MIN, SESSION_TTL,
    Token, looks_like_email, normalize_email,
};
use crate::error::{Invalid, Outcome};
use crate::feed::Feed;
use crate::password;
use crate::store::{Sql, Value};

/// The handle is claimed here, and the two ways it can be taken are both
/// refused by the database rather than by a read before the batch. A read
/// would be a precondition outside the transaction — and, worse, it would run
/// before [`run`] gets to answer a replayed idempotency key, which is how a
/// retried registration turns into a refusal.
///
/// A live handle collides on `party_handle_unique_idx`; one a merge or a
/// rename filed away is refused by this `WHERE NOT EXISTS`, which makes the
/// whole batch write nothing and reads as the uniform decline.
const PARTY: &str = "INSERT INTO party (kind, display_name, handle, status, created_at) \
                     SELECT 'person', $1, $2, 'active', $3 \
                     WHERE NOT EXISTS (SELECT 1 FROM party_handle_alias WHERE handle = $2) \
                     RETURNING id";

const IDENTITY: &str = "INSERT INTO identity (source, person_id, home_zone, status, created_at) \
                        VALUES ($1, $2, $3, 'active', $4) RETURNING id";

const FACTOR: &str = "INSERT INTO factor (identity_id, kind, value, created_at) \
                      VALUES ($1, $2, $3, $4)";

const SESSION: &str = "INSERT INTO session \
                       (identity_id, acting_as, token_hash, expires_at, created_at, \
                        last_seen_at, auth_time) \
                       VALUES ($1, $2, $3, $4, $5, $5, $5) RETURNING id";

const AUDIT: &str = "INSERT INTO audit \
                     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
                     VALUES ($1, 'register', $2, $3, $4, $5, \
                     json_object('event', 'register', 'identity', $6, 'person', $7, \
                                 'session', $8))";

/// Create a person, an identity, its email and password factors, and a
/// session — in one transaction.
///
/// An address that is already registered declines exactly like a bad password
/// does. Saying "that email is taken" would turn the registration form into a
/// list of who is registered here.
pub async fn register<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, args: &Register) -> Outcome<Minted> {
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
    let email = normalize_email(&args.email);
    if !looks_like_email(&email) {
        return Err(Invalid::NotAnEmail.into());
    }
    if email.len() > EMAIL_LIMIT {
        return Err(Invalid::TooLong {
            field: "email",
            limit: EMAIL_LIMIT,
        }
        .into());
    }
    if args.password.chars().count() < PASSWORD_MIN {
        return Err(Invalid::PasswordTooShort(PASSWORD_MIN).into());
    }
    // The handle is chosen here and changeable by `SetHandle`. Its *shape* is
    // an `Invalid` — the caller typed it — and a handle somebody already holds
    // declines through the UNIQUE index in the batch, exactly as a registered
    // address does: two different reasons for "no" that read the same.
    let handle = crate::oidc::handle::validate(&args.handle)?;

    // Hashed once, outside the batch and off the reactor: argon2 is tens of
    // milliseconds, a retry must not pay for it twice, and no transaction is
    // held open for it — see `password`'s note on the blocking variants.
    let phc = password::hash_blocking(&args.password)
        .await
        .map_err(|err| crate::KernelError::Invariant(err.to_string()))?;
    let token = Token::mint();
    let now = ctx.now();

    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        let party = batch.one(PARTY, bind![display_name, handle.as_str(), now]);
        let identity = batch.one(
            IDENTITY,
            vec![
                Value::from(DEFAULT_SOURCE),
                party.column("id"),
                Value::from(DEFAULT_ZONE),
                Value::from(now),
            ],
        );
        batch.one(
            FACTOR,
            vec![
                identity.column("id"),
                Value::from("email"),
                Value::from(email.as_str()),
                Value::from(now),
            ],
        );
        batch.one(
            FACTOR,
            vec![
                identity.column("id"),
                Value::from("password"),
                Value::from(phc.as_str()),
                Value::from(now),
            ],
        );
        let session = batch.one(
            SESSION,
            vec![
                identity.column("id"),
                party.column("id"),
                Value::from(token.digest()),
                Value::from(now + SESSION_TTL),
                Value::from(now),
            ],
        );
        batch.one(
            AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                identity.column("id"),
                party.column("id"),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                identity.column("id"),
                party.column("id"),
                session.column("id"),
            ],
        );
        Ok(batch)
    })
    .await?;

    Ok(Minted {
        token: applied.fresh.then_some(token),
        committed: applied.committed,
    })
}
