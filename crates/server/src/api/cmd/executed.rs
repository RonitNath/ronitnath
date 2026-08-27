//! What a command produced, how it reaches the browser, and how it fails.
//!
//! Three things ride back from a command besides its event. The **offset** is
//! what a caller waits on to see its own write. The **result** is the public
//! ids it created, derived on the way out ([`result`](super::result)) — never
//! an address, never an internal id. And the **cookie**: `Register` and
//! `SignIn` mint a session, `SignOut` ends one, and every other command leaves
//! it alone.
//!
//! One secret does ride in a result, and only one: the token `Invite` mints.
//! A link has no public id — it *is* its token — so the reply is the only
//! place the clear text ever exists, exactly once, on the call that minted it.
//! It is carried in its own field rather than folded into the event mapping,
//! so [`result`](super::result) stays a function of the event alone and cannot
//! grow a second way to emit a secret.

use axum::Json;
use axum::response::{IntoResponse, Response};
use rn_api::CommandReply;
use rn_kernel::cmd::Minted;
use rn_kernel::domain::{SESSION_TTL, Token};
use rn_kernel::ids::IdKey;
use rn_kernel::{Committed, KernelError};

use rn_kernel::store::Reads;

use super::decline;
use crate::api::party;
use crate::api::result::{ambiguous_party, result_of};
use crate::auth::session;

/// What a command produced, plus the two session effects a reply carries.
#[derive(Debug)]
pub struct Executed {
    /// The event and the offset it landed at.
    pub committed: Committed,
    /// A freshly minted session secret. Present only on the call that minted
    /// it — a replay has nothing to hand back, because the row keeps only the
    /// digest.
    pub token: Option<Token>,
    /// A freshly minted *link* secret, on the same terms: `Invite` and nothing
    /// else, once, and never on a replay.
    pub link: Option<Token>,
    /// A freshly minted *client* secret, on the same terms again.
    ///
    /// It rides back for the reason a link token does: it has no other name.
    /// An operator who registers a relying party has to be handed its secret,
    /// there is nowhere else it exists, and the row keeps only its SHA-256. It
    /// is a *different field* from the link so that the reply cannot grow one
    /// way to emit either by accident.
    pub client_secret: Option<Token>,
    /// Whether the caller's own session is now gone.
    pub ended: bool,
}

impl Executed {
    pub(super) fn of(committed: Committed) -> Self {
        Self {
            committed,
            token: None,
            link: None,
            client_secret: None,
            ended: false,
        }
    }

    /// A command that registered or re-secreted a client.
    pub(super) fn secreting(registered: rn_kernel::cmd::Registered) -> Self {
        Self {
            committed: registered.committed,
            token: None,
            link: None,
            client_secret: registered.secret,
            ended: false,
        }
    }

    pub(super) fn minted(minted: Minted) -> Self {
        Self {
            committed: minted.committed,
            token: minted.token,
            link: None,
            client_secret: None,
            ended: false,
        }
    }

    /// A command that minted a bearer *link*. The secret goes in the reply
    /// body, never in a cookie: it is not this browser's session, it is the
    /// thing this browser is about to send somebody else.
    pub(super) fn linking(minted: Minted) -> Self {
        Self {
            committed: minted.committed,
            token: None,
            link: minted.token,
            client_secret: None,
            ended: false,
        }
    }

    pub(super) fn ending(committed: Committed) -> Self {
        Self {
            committed,
            token: None,
            link: None,
            client_secret: None,
            ended: true,
        }
    }

    /// Whether this command changed what the caller's own cookie resolves to.
    ///
    /// Signing out is the obvious one. `ActAs` is the other: it rewrites the
    /// session row, and the resolution of that cookie is cached — so without
    /// evicting it here, the very next request would still answer as the
    /// party the caller just stopped speaking as. Every *other* node learns
    /// from the feed a moment later (`sub::invalidate`); this is the caller's
    /// own, evicted before its own response leaves.
    #[must_use]
    pub fn rebinds_session(&self) -> bool {
        self.ended
            || matches!(
                self.committed.event,
                rn_kernel::Event::ActingAs { .. } | rn_kernel::Event::SessionRevoked { .. }
            )
    }

    /// The `Set-Cookie` this command implies, if any.
    #[must_use]
    pub fn cookie(&self, mode: crate::config::Mode) -> Option<String> {
        match (&self.token, self.ended) {
            (Some(token), _) => Some(session::set(token, SESSION_TTL, mode)),
            (None, true) => Some(session::clear(mode)),
            (None, false) => None,
        }
    }

    /// The command's reply body.
    ///
    /// The read is for the one party row an event may name without fixing its
    /// kind — see [`ambiguous_party`]. Most commands name none and pay
    /// nothing.
    pub async fn reply(&self, reads: &impl Reads, key: &IdKey) -> CommandReply {
        let party = match ambiguous_party(&self.committed.event) {
            Some(raw) => party::public_id(reads, raw, key).await,
            None => None,
        };
        let mut result = result_of(&self.committed.event, key, party);
        if let (Some(link), Some(fields)) = (&self.link, result.as_object_mut()) {
            fields.insert("token".to_owned(), link.expose().into());
        }
        if let (Some(secret), Some(fields)) = (&self.client_secret, result.as_object_mut()) {
            fields.insert("client_secret".to_owned(), secret.expose().into());
        }
        CommandReply {
            offset: self.committed.offset,
            result,
        }
    }
}

/// Why a command did not run.
#[derive(Debug)]
pub enum CommandError {
    /// The kernel refused it.
    Kernel(KernelError),
    /// No binding: either not a command at all, or one a later leg owns.
    Unbound,
    /// The body was not the envelope this command's arguments live in.
    Malformed(String),
}

impl From<KernelError> for CommandError {
    fn from(error: KernelError) -> Self {
        Self::Kernel(error)
    }
}

impl CommandError {
    /// The response this failure is allowed to be.
    #[must_use]
    pub fn response(&self) -> Response {
        match self {
            Self::Kernel(error) => decline::from_kernel(error),
            Self::Unbound => decline::not_found(),
            Self::Malformed(message) => (
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                Json(decline::Invalidated {
                    invalid: vec![decline::FieldError {
                        field: "body",
                        message: message.clone(),
                    }],
                }),
            )
                .into_response(),
        }
    }
}
