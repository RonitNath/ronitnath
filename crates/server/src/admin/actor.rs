//! `--as <email>` — who an admin action is accountable to.
//!
//! A kernel command wants a [`Principal::Member`], and a `Member` carries a
//! session: `cmd::refs::actor` reads the identity behind it and
//! `authority::fresh_enough` reads its `auth_time`. Neither front door of
//! [`crate::admin`] has one, so this module makes one, uses it for exactly one
//! command, and deletes it.
//!
//! Three things keep that honest, and each is written into the statements
//! below.
//!
//! * **The row cannot be presented.** `session.token_hash` holds the SHA-256
//!   of a secret that was handed to somebody once. This one holds 32 random
//!   bytes that are the digest of *nothing* — no string hashes to them, so
//!   there is no cookie that opens this session even in the seconds it exists.
//! * **It grants nothing.** The principal it builds names the person the
//!   address registered, and the command asks the database what that person
//!   may do. An address that names somebody who is not an operator gets the
//!   same refusal it would get in a browser.
//! * **It goes away.** [`Actor::close`] deletes the row whether the command
//!   succeeded or refused, and the expiry is short enough
//!   ([`ACTOR_SESSION_TTL`]) that a process killed between the two leaves a row
//!   that stops resolving in a minute rather than in a fortnight.
//!
//! What it does *not* do is prove anything about who typed the address. That
//! is not this module's job and it could not be: proof of being on the host is
//! having the data directory, or having reached loopback. `--as` answers a
//! different question — *whose name goes in the audit row* — and answering it
//! is the whole reason the CLI is not a hole in the record.

use rn_kernel::Principal;
use rn_kernel::bind;
use rn_kernel::ids::{Id, Identity, Person, Session};
use rn_kernel::store::{Count, Cursor, FromRow, Reads, RowError};

use super::AdminError;
use crate::state::AppState;

/// How long the actor session is valid for. One minute: long enough for the
/// command it was minted for, short enough that a killed process leaves
/// nothing usable behind.
pub const ACTOR_SESSION_TTL: i64 = 60;

/// The identity an address registered, and the person it resolved to.
const IDENTITY_OF_EMAIL: &str = "SELECT i.id AS identity_id, i.person_id, i.status \
     FROM factor f JOIN identity i ON i.id = f.identity_id \
     WHERE f.kind = 'email' AND f.value = $1";

/// An active identity of a person, for `--as` given a public id rather than an
/// address. The oldest one, so the choice is stable across calls.
const IDENTITY_OF_PERSON: &str = "SELECT i.id AS identity_id, i.person_id, i.status \
     FROM identity i WHERE i.person_id = $1 AND i.status = 'active' \
     ORDER BY i.id ASC LIMIT 1";

/// The same `WHERE EXISTS` guard every other session insert in the tree
/// carries: a disable that landed between the read and the write must win.
const MINT: &str = "INSERT INTO session \
     (identity_id, acting_as, token_hash, expires_at, created_at, last_seen_at, auth_time) \
     SELECT $1, $2, $3, $4, $5, $5, $5 \
     WHERE EXISTS (SELECT 1 FROM identity i \
                   LEFT JOIN party who ON who.id = i.person_id \
                   WHERE i.id = $1 AND i.status = 'active' \
                     AND coalesce(who.status, 'active') = 'active')";

/// The row just written, found by the one thing that is unique about it.
const FIND: &str = "SELECT id AS n FROM session WHERE token_hash = $1";

const DELETE: &str = "DELETE FROM session WHERE id = $1";

struct IdentityRow {
    identity: Id<Identity>,
    person: Option<Id<Person>>,
    status: String,
}

impl FromRow for IdentityRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            identity: row.id("identity_id")?,
            person: row.id_opt("person_id")?,
            status: row.text("status")?,
        })
    }
}

/// A principal that exists for the length of one command.
#[derive(Debug)]
pub struct Actor {
    /// Who the command runs as.
    pub principal: Principal,
    /// The row [`Self::close`] deletes.
    session: Id<Session>,
}

impl Actor {
    /// Mint an actor session for the address or public id `who` names.
    ///
    /// # Errors
    ///
    /// [`AdminError::NoSuchPerson`] when nothing is registered under it, and
    /// [`AdminError::NoActor`] when the identity exists and cannot hold a
    /// session — disabled, or resolved to no person, in which case there is
    /// nobody for `allows` to ask about.
    pub async fn open(state: &AppState, who: &str) -> Result<Self, AdminError> {
        let who = who.trim();
        let reads = state.store.reads();

        let found: Option<IdentityRow> =
            if let Ok(person) = rn_kernel::ids::parse::<Person>(state.ids(), who) {
                reads
                    .query_opt(IDENTITY_OF_PERSON, bind![person])
                    .await
                    .map_err(AdminError::store)?
            } else {
                reads
                    .query_opt(IDENTITY_OF_EMAIL, bind![who])
                    .await
                    .map_err(AdminError::store)?
            };
        let found = found.ok_or_else(|| AdminError::NoSuchPerson(who.to_owned()))?;

        if found.status != "active" {
            return Err(AdminError::NoActor(format!(
                "the registration at {who} is {}, so it cannot act",
                found.status
            )));
        }
        let person = found.person.ok_or_else(|| {
            AdminError::NoActor(format!(
                "{who} has registered but has not resolved to a person yet, and authority is the \
                 person's"
            ))
        })?;

        // Thirty-two bytes that are nobody's digest. See the module doc.
        let mut opaque = [0u8; 32];
        getrandom::fill(&mut opaque).map_err(|error| {
            AdminError::Store(format!(
                "the operating system provided no randomness: {error}"
            ))
        })?;

        let now = reads.now();
        let written = state
            .store
            .execute(
                MINT,
                bind![
                    found.identity,
                    person,
                    opaque.to_vec(),
                    now + ACTOR_SESSION_TTL,
                    now
                ],
            )
            .await
            .map_err(AdminError::store)?;
        if written == 0 {
            return Err(AdminError::NoActor(format!(
                "{who} cannot hold a session: the registration or the person it names is disabled"
            )));
        }

        let session: Count = reads
            .query_one(FIND, bind![opaque.to_vec()])
            .await
            .map_err(AdminError::store)?;

        Ok(Self {
            principal: Principal::Member {
                identity: found.identity,
                person: Some(person),
                acting_as: person,
                session: Id::new(session.0),
                // Never. An admin action is the operator acting as themselves;
                // an operator wearing somebody's hat is `SignInAs`, which is a
                // browser thing and is refused from a session like this one by
                // `authority::FORBIDDEN_WHILE_IMPERSONATING` anyway.
                impersonated_by: None,
            },
            session: Id::new(session.0),
        })
    }

    /// Delete the row. Best effort by construction: the process is about to
    /// exit either way, and the expiry is a minute.
    pub async fn close(self, state: &AppState) {
        if let Err(error) = state.store.execute(DELETE, bind![self.session]).await {
            tracing::warn!(%error, "the admin actor session could not be deleted; it expires in a minute");
        }
    }
}
