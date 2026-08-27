//! `POST /api/cmd/<name>` — the dispatch table, and the only place a command
//! is executed.
//!
//! The vocabulary is `rn_api::commands::ALL_COMMAND_NAMES`: one list, shared by
//! the kernel's event names, the browser's client and this router. The table
//! below binds a name to a kernel function, and it binds all of them:
//! `unbound()` is empty and
//! `every_command_in_the_contract_is_bound` keeps it that way, so a command
//! added to the contract without a binding fails a test rather than answering
//! a uniform decline nobody can tell from a broken route.
//!
//! Nothing here authorises anything. The commands do that themselves, inside
//! their own transactions (`kernel::cmd`), which is why a route that forgot
//! would be refused by the kernel rather than by a check this file could omit.

pub mod confirmed;
mod executed;

pub use executed::{CommandError, Executed};

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::post};
use rn_api::commands::{
    ALL_COMMAND_NAMES, ActAs, AddFactor, ClaimLink, ConfirmMatch, CreateDocument, CreateGroup,
    CreateOrganization, Disable, EditDocument, Enable, Invite, Leave, ProposeMatch,
    PublishDocument, Register, RemoveFactor, RemoveMember, Revoke, RevokeLink, RevokeSession,
    RuleMatch, SetRole, Share, SignIn, SignOut, Split, Transfer, VerifyEmail,
};
use rn_api::{Command, CommandEnvelope};
use rn_kernel::Principal;
use rn_kernel::cmd::{self, Ctx};
use uuid::Uuid;

use super::decline;
use super::origin::SameOrigin;
use crate::auth::session;
use crate::state::AppState;
use axum::http::HeaderValue;
use rn_kernel::Offset;

/// Bind command names to kernel functions.
///
/// The kind says what the function returns and therefore what the reply does
/// with the secret it minted: `plain` nothing, `minting` a new session cookie,
/// `ending` the caller's own, `linking` a bearer link handed back in the reply
/// body because a link has no other name.
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
    (@run linking $ctx:ident, $run:path, $args:expr) => {
        $run(&$ctx, &$args).await.map(Executed::linking).map_err(CommandError::Kernel)
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
    plain ActAs => cmd::act_as,
    plain AddFactor => cmd::add_factor,
    plain RemoveFactor => cmd::remove_factor,
    plain VerifyEmail => cmd::verify_email,
    plain CreateOrganization => cmd::create_organization,
    plain CreateGroup => cmd::create_group,
    linking Invite => cmd::invite,
    plain ClaimLink => cmd::claim_link,
    plain RevokeLink => cmd::revoke_link,
    plain SetRole => cmd::set_role,
    plain RemoveMember => cmd::remove_member,
    plain Leave => cmd::leave,
    plain Share => cmd::share,
    plain Revoke => cmd::revoke,
    plain Transfer => cmd::transfer,
    plain CreateDocument => cmd::create_document,
    plain EditDocument => cmd::edit_document,
    plain PublishDocument => cmd::publish_document,
    plain ProposeMatch => cmd::propose_match,
    plain ConfirmMatch => cmd::confirm_match,
    plain RuleMatch => cmd::rule_match,
    plain Split => cmd::split,
    plain Disable => cmd::disable,
    plain Enable => cmd::enable,
}

/// Run a command, on this handler's own task.
///
/// There is no blocking-pool wrapper here any more and that is deliberate.
/// [`rn_kernel::cmd::run`] bounds its plan future `Send`, so a command future
/// is `Send` and can simply be awaited inside an axum handler; the argon2id
/// verification that used to justify the wrapper now goes to the blocking pool
/// where it happens, inside `Register` and `SignIn`, rather than dragging the
/// whole command — its transaction, its feed notify — off the reactor with it.
async fn dispatch(
    state: &AppState,
    principal: Principal,
    name: String,
    body: String,
) -> Result<Executed, CommandError> {
    dispatch_inner(state, principal, &name, &body).await
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
        // A form post's caller waits by the header, not by the envelope: this
        // path is reached with arguments already typed, and the wait has
        // happened by the time one is built.
        after: None,
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
///
/// The session is resolved here rather than by an extractor, and the order is
/// the reason: a caller that presents an `after` is telling this node it has
/// already been shown a world that includes its own `register` — and the
/// session that `register` minted is one of the rows a stale node has not
/// applied yet. Waiting first means the cookie is looked up in the world the
/// caller was promised, so the answer to "who is this" cannot be "nobody"
/// either ([`confirmed`]).
async fn command(
    State(state): State<AppState>,
    _: SameOrigin,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Response {
    // Only for a caller that presents a cookie. The reason the barrier comes
    // before the session lookup is that the cookie may name a session this
    // node has not applied yet — which presupposes there is a cookie. Without
    // one the command is declined below whatever this node's offset is, so
    // waiting for an offset first is [`confirmed::WAIT`] seconds of this node
    // spent on a request that was never going to be authorised.
    if session::cookie(&headers, session::COOKIE).is_some()
        && let Some(after) = confirmed::requested(&body, &headers)
    {
        confirmed::catch_up(&state, after).await;
    }
    let principal = match session::resolve(&state, &headers).await.principal {
        principal @ rn_kernel::Principal::Member { .. } => principal,
        _ => return decline::forbidden(),
    };
    match dispatch(&state, principal, name, body).await {
        Ok(executed) => {
            if executed.rebinds_session() {
                session::forget(&state, &headers);
            }
            let offset = executed.committed.offset;
            let reply = executed.reply(&state.store.reads(), state.ids()).await;
            let mut response = Json(reply).into_response();
            if let Some(value) = executed
                .cookie(state.config.mode)
                .and_then(|cookie| cookie.parse().ok())
            {
                response
                    .headers_mut()
                    .insert(axum::http::header::SET_COOKIE, value);
            }
            stamp_offset(&mut response, offset);
            response
        }
        Err(error) => error.response(),
    }
}

/// Say where a command landed, on the response itself.
///
/// The JSON reply carries the offset too, and this is for the caller that
/// cannot read it: an `/auth` form post is answered with a redirect, and the
/// browser or harness that follows it still has to know what to send back as
/// `after` on its next command.
pub fn stamp_offset(response: &mut Response, offset: Offset) {
    if let Ok(value) = HeaderValue::from_str(&offset.to_string()) {
        response
            .headers_mut()
            .insert(confirmed::OFFSET_HEADER, value);
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
    fn every_command_in_the_contract_is_bound() {
        let unbound = unbound();
        assert!(
            unbound.is_empty(),
            "the contract declares commands this build declines: {unbound:?}"
        );
        assert_eq!(BOUND.len(), ALL_COMMAND_NAMES.len());
    }

    #[test]
    fn no_name_is_bound_twice() {
        let mut sorted = BOUND.to_vec();
        sorted.sort_unstable();
        let count = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), count, "a command name is bound twice");
    }
}
