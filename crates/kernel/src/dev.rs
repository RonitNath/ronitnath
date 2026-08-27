//! The developer sign-in, and nothing else. Debug builds only.
//!
//! `crates/server/src/auth/dev.rs` serves a button that signs the person at
//! the keyboard in as this deployment's platform operator without a password.
//! That is a bypass of the one control the whole model rests on, so it is
//! written here — inside the kernel, as a command-shaped transaction — rather
//! than as a session row a handler inserts on its own.
//!
//! Three things make it honest:
//!
//! * **The module does not exist in a release build.** `lib.rs` declares it
//!   under `#[cfg(debug_assertions)]`, so the statements below are not in the
//!   binary a deployment runs and no configuration can reach them.
//! * **It writes an audit row of its own name.** `dev-sign-in`, not
//!   `sign-in`: a session minted without a password must be distinguishable
//!   from one minted with a password for as long as the row exists, and a
//!   reader of the tail should never have to infer which it was.
//! * **It proves nothing about the identity except that it is usable.** The
//!   same `WHERE EXISTS` guard [`crate::cmd::sign_in`] commits under, for the
//!   same reason: a disable that lands between the read and the write must
//!   win.
//!
//! The payload is `SignedIn`-shaped, because what happened *is* a sign-in as
//! far as every consumer of the feed is concerned — the invalidator, the
//! subscriptions and the presence machinery all have to see it, and a private
//! event tag would be a session they never heard about.

use serde::Serialize;

use crate::bind;
use crate::cmd::{Applied, Batch, Ctx, Minted, run};
use crate::domain::{SESSION_TTL, Token};
use crate::error::{Outcome, decline};
use crate::feed::Feed;
use crate::ids::{Id, Identity, Person};
use crate::store::{Cursor, FromRow, Reads, RowError, Sql, Value};

/// The identity, and the person it acts as, if it can hold a session at all.
const USABLE: &str = "SELECT i.id AS identity_id, i.person_id \
                      FROM identity i \
                      LEFT JOIN party who ON who.id = i.person_id \
                      WHERE i.id = $1 AND i.status = 'active' \
                        AND coalesce(who.status, 'active') = 'active'";

/// [`crate::cmd::sign_in`]'s statement, verbatim: the guard is the whole rule,
/// and a copy that relaxed it would be a session the disable did not stop.
/// The `auth_time` a bypass writes is the moment it ran, and that is the
/// honest answer: no password was presented, and a column saying one was five
/// minutes ago would be a lie in the same row as the `dev-sign-in` that says
/// otherwise. What keeps it defensible is that this module is not in a release
/// binary at all.
const SESSION: &str = "INSERT INTO session \
                       (identity_id, acting_as, token_hash, expires_at, created_at, \
                        last_seen_at, auth_time) \
                       SELECT $1, $2, $3, $4, $5, $5, $5 \
                       WHERE EXISTS (SELECT 1 FROM identity i \
                                     LEFT JOIN party who ON who.id = i.person_id \
                                     WHERE i.id = $1 AND i.status = 'active' \
                                       AND coalesce(who.status, 'active') = 'active') \
                       RETURNING id";

/// `command` is `dev-sign-in`; the payload's `event` is `sign-in`.
///
/// Those are two different questions and they get two different answers. The
/// command column is the record of what was done, and what was done was a
/// bypass. The payload is what the feed carries, and every consumer of it
/// needs the session it just learned about to look like the session it is.
const AUDIT: &str = "INSERT INTO audit \
                     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
                     SELECT $1, 'dev-sign-in', $2, $3, $4, $5, \
                            json_object('event', 'sign-in', 'identity', $2, 'session', $6) \
                     WHERE changes() > 0";

/// The name the audit row carries. Stated once, so a test and the statement
/// above cannot drift.
pub const ACTION: &str = "dev-sign-in";

/// What the arguments digest is taken over. There are no arguments a caller
/// sends — the identity is resolved by the server — so this is what makes two
/// calls under one idempotency key the same call.
#[derive(Debug, Serialize)]
struct DevSignIn {
    identity: i64,
}

struct Usable {
    identity_id: Id<Identity>,
    person_id: Option<Id<Person>>,
}

impl FromRow for Usable {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            identity_id: row.id("identity_id")?,
            person_id: row.id_opt("person_id")?,
        })
    }
}

/// Mint a session for an identity, with no proof asked of it.
///
/// # Errors
///
/// The uniform decline when the identity does not exist, is disabled, or
/// belongs to a person this deployment has disabled — the three cases
/// [`crate::cmd::sign_in`] refuses, refused here for the same reasons.
pub async fn sign_in_as<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    identity: Id<Identity>,
) -> Outcome<Minted> {
    let Some(usable) = ctx
        .store
        .query_opt::<Usable>(USABLE, bind![identity])
        .await?
    else {
        return decline();
    };

    // As in `sign_in`: an identity whose person has not resolved acts as
    // itself, which is what keeps `acting_as` NOT NULL honest.
    let acting_as = usable
        .person_id
        .unwrap_or_else(|| Id::<Person>::new(usable.identity_id.get()));
    let token = Token::mint();
    let now = ctx.now();
    let args = DevSignIn {
        identity: usable.identity_id.get(),
    };

    let Applied { committed, fresh } = run(ctx, &args, async || {
        let mut batch = Batch::new();
        let session = batch.one(
            SESSION,
            vec![
                Value::from(usable.identity_id),
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
                Value::from(usable.identity_id),
                Value::from(acting_as),
                Value::from(now),
                Value::from(crate::audit::digest_of(&args)),
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

#[cfg(test)]
mod tests;
