//! What a command produced, how it reaches the browser, and how it fails.
//!
//! Three things ride back from a command besides its event. The **offset** is
//! what a caller waits on to see its own write. The **result** is the public
//! ids it created, derived on the way out — never a token, never an address,
//! never an internal id. And the **cookie**: `Register` and `SignIn` mint a
//! session, `SignOut` ends one, and every other command leaves it alone.

use axum::Json;
use axum::response::{IntoResponse, Response};
use rn_api::CommandReply;
use rn_kernel::cmd::Minted;
use rn_kernel::domain::{SESSION_TTL, Token};
use rn_kernel::ids::IdKey;
use rn_kernel::{Committed, Event, KernelError};
use serde_json::{Value, json};

use super::decline;
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
    /// Whether the caller's own session is now gone.
    pub ended: bool,
}

impl Executed {
    pub(super) fn of(committed: Committed) -> Self {
        Self {
            committed,
            token: None,
            ended: false,
        }
    }

    pub(super) fn minted(minted: Minted) -> Self {
        Self {
            committed: minted.committed,
            token: minted.token,
            ended: false,
        }
    }

    pub(super) fn ending(committed: Committed) -> Self {
        Self {
            committed,
            token: None,
            ended: true,
        }
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
    #[must_use]
    pub fn reply(&self, key: &IdKey) -> CommandReply {
        CommandReply {
            offset: self.committed.offset,
            result: result_of(&self.committed.event, key),
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

/// The public ids a command produced, derived on the way out. Never a token,
/// never an address, never an internal id.
fn result_of(event: &Event, key: &IdKey) -> Value {
    match event {
        Event::Registered {
            identity, person, ..
        } => json!({ "identity": identity.public(key), "person": person.public(key) }),
        Event::SignedIn { identity, .. } => json!({ "identity": identity.public(key) }),
        Event::SignedOut { session, .. } | Event::SessionRevoked { session, .. } => {
            json!({ "session": session.public(key) })
        }
        Event::FactorAdded { factor, .. }
        | Event::FactorRemoved { factor, .. }
        | Event::EmailVerified { factor, .. } => json!({ "factor": factor.public(key) }),
        Event::PartyDisabled { party } | Event::PartyEnabled { party } => {
            json!({ "party": party.public(key) })
        }
    }
}
