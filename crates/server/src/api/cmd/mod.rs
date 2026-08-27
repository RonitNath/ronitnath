//! `POST /api/cmd/<name>` — the dispatch table, and the only place a command
//! is executed.
//!
//! The vocabulary is `rn_api::commands::ALL_COMMAND_NAMES`: one list, shared by
//! the kernel's event names, the browser's client and this router. The table
//! below binds a name to a kernel function; a name in the vocabulary with no
//! binding — every command K2 and K3 own — answers with the uniform decline,
//! because a route that exists and does nothing is indistinguishable from a
//! broken one. Wiring one up later is a line in [`bindings!`].
//!
//! Nothing here authorises anything. The commands do that themselves, inside
//! their own transactions (`kernel::cmd`), which is why a route that forgot
//! would be refused by the kernel rather than by a check this file could omit.

mod executed;

pub use executed::{CommandError, Executed};

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::post};
use rn_api::commands::{
    ALL_COMMAND_NAMES, AddFactor, Disable, Enable, Register, RemoveFactor, RevokeSession, SignIn,
    SignOut, VerifyEmail,
};
use rn_api::{Command, CommandEnvelope};
use rn_kernel::cmd::{self, Ctx};
use rn_kernel::{KernelError, Principal};
use uuid::Uuid;

use super::decline;
use super::origin::SameOrigin;
use crate::auth::session;
use crate::state::AppState;

/// Bind command names to kernel functions.
///
/// The kind says what the function returns and therefore what the reply does
/// to the session cookie: `plain` nothing, `minting` a new cookie, `ending`
/// the caller's own.
macro_rules! bindings {
    ($($kind:ident $args:ty => $run:path),+ $(,)?) => {
        /// The command names this build executes. Everything else in
        /// `ALL_COMMAND_NAMES` declines until its leg lands.
        pub const BOUND: &[&str] = &[$(<$args as Command>::NAME),+];

        async fn dispatch_inner(
            state: &AppState,
            principal: Principal,
            name: &str,
            body: &str,
        ) -> Result<Executed, CommandError> {
            match name {
                $(<$args as Command>::NAME => {
                    let envelope: CommandEnvelope<$args> = serde_json::from_str(body)
                        .map_err(|error| CommandError::Malformed(error.to_string()))?;
                    let ctx = Ctx {
                        store: state.store.as_ref(),
                        feed: state.feed.as_ref(),
                        principal,
                        key: envelope.key,
                    };
                    bindings!(@run $kind ctx, $run, envelope.args)
                })+
                _ => Err(CommandError::Unbound),
            }
        }
    };
    (@run plain $ctx:ident, $run:path, $args:expr) => {
        $run(&$ctx, &$args).await.map(Executed::of).map_err(CommandError::Kernel)
    };
    (@run minting $ctx:ident, $run:path, $args:expr) => {
        $run(&$ctx, &$args).await.map(Executed::minted).map_err(CommandError::Kernel)
    };
    (@run ending $ctx:ident, $run:path, $args:expr) => {
        $run(&$ctx, &$args).await.map(Executed::ending).map_err(CommandError::Kernel)
    };
}

bindings! {
    minting Register => cmd::register,
    minting SignIn => cmd::sign_in,
    ending SignOut => cmd::sign_out,
    plain RevokeSession => cmd::revoke_session,
    plain AddFactor => cmd::add_factor,
    plain RemoveFactor => cmd::remove_factor,
    plain VerifyEmail => cmd::verify_email,
    plain Disable => cmd::disable,
    plain Enable => cmd::enable,
}

/// Run a command on the blocking pool.
///
/// Two reasons, and either would be enough on its own.
///
/// **The KDF.** `Register` and `SignIn` do an argon2id verification — tens of
/// milliseconds of deliberate CPU. On a runtime worker that is the reactor
/// stalled for every other connection this node is serving, which is not a
/// tuning question but a correctness one for the heartbeat on `/api/sub`.
///
/// **The `Send` bound.** `rn_kernel::cmd::run` takes its plan as an
/// `AsyncFn`, whose returned future carries no `Send` bound, so a command
/// future is not provably `Send` for every lifetime and cannot be awaited
/// inside an axum handler. Driving it to completion on one blocking thread
/// means it never crosses threads and never needs to be. Listed as a kernel
/// gap in this leg's report; when the bound lands, this wrapper is the only
/// thing that has to go.
async fn dispatch(
    state: &AppState,
    principal: Principal,
    name: String,
    body: String,
) -> Result<Executed, CommandError> {
    let state = state.clone();
    let handle = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        handle.block_on(dispatch_inner(&state, principal, &name, &body))
    })
    .await
    .unwrap_or_else(|joined| {
        // A panicking command is a bug in this process, never the caller's
        // fault and never useful to them: it is logged and declined.
        tracing::error!(%joined, "a command panicked");
        Err(CommandError::Kernel(KernelError::Invariant(
            "a command panicked".to_owned(),
        )))
    })
}

/// Command names the product contract declares that this build declines.
#[must_use]
pub fn unbound() -> Vec<&'static str> {
    ALL_COMMAND_NAMES
        .iter()
        .copied()
        .filter(|name| !BOUND.contains(name))
        .collect()
}

/// Run a command whose arguments are already typed — the auth forms' path.
///
/// It goes through [`dispatch`] rather than calling the kernel directly, so a
/// form post and a bundle's `POST /api/cmd/<name>` are one execution path and
/// cannot drift.
pub async fn invoke<A: Command + serde::Serialize>(
    state: &AppState,
    principal: Principal,
    args: &A,
) -> Result<Executed, CommandError> {
    let envelope = CommandEnvelope {
        key: Uuid::new_v4(),
        args,
    };
    let body = serde_json::to_string(&envelope)
        .map_err(|error| CommandError::Malformed(error.to_string()))?;
    dispatch(state, principal, A::NAME.to_owned(), body).await
}

pub fn router() -> Router<AppState> {
    Router::new().route("/api/cmd/{name}", post(command))
}

/// The route. A cookie, a same-origin browser, a body, a command.
async fn command(
    State(state): State<AppState>,
    _: SameOrigin,
    session: session::Session,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Response {
    match dispatch(&state, session.principal, name, body).await {
        Ok(executed) => {
            if executed.ended {
                session::forget(&state, &headers);
            }
            let mut response = Json(executed.reply(state.ids())).into_response();
            if let Some(value) = executed
                .cookie(state.config.mode)
                .and_then(|cookie| cookie.parse().ok())
            {
                response
                    .headers_mut()
                    .insert(axum::http::header::SET_COOKIE, value);
            }
            response
        }
        Err(error) => error.response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bound_name_is_in_the_product_contract() {
        for name in BOUND {
            assert!(
                ALL_COMMAND_NAMES.contains(name),
                "{name} is not a declared command"
            );
        }
    }

    #[test]
    fn the_commands_later_legs_own_are_declared_and_unbound() {
        let unbound = unbound();
        for name in [
            "create-organization",
            "invite",
            "claim-link",
            "share",
            "propose-match",
            "split",
        ] {
            assert!(unbound.contains(&name), "{name} should still decline");
        }
        assert_eq!(BOUND.len() + unbound.len(), ALL_COMMAND_NAMES.len());
    }

    #[test]
    fn the_identity_half_of_the_contract_is_wired() {
        for name in [
            "register",
            "sign-in",
            "sign-out",
            "revoke-session",
            "add-factor",
            "remove-factor",
            "verify-email",
            "disable",
            "enable",
        ] {
            assert!(BOUND.contains(&name), "{name} is not wired");
        }
    }
}
