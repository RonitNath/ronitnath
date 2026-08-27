//! `/oidc/*` and `/.well-known/*` — this deployment as an OpenID Provider.
//!
//! One router, mounted whole ([`router`]), and one configuration struct
//! ([`crate::config::AppConfig`]'s `public_origin`, `public_name` and
//! `oidc_key`). Nothing in here knows which deployment it is running on: the
//! issuer, the deployment's own name, the sealing key, the client registry and
//! the sector a client's subjects are pairwise under all come from
//! configuration or from the store. A downstream binary that depends on these
//! crates mounts this router, sets three variables, and is an OpenID Provider
//! — see `docs/oidc.md` §What a downstream deployment needs.
//!
//! ## The trust boundary
//!
//! This is a *fourth* surface beside the three `docs/rebuild/plan.md` §Product
//! contract names, and it is bounded on both sides.
//!
//! * The **session cookie** reaches `/oidc/authorize` and `/oidc/end_session`
//!   and nothing else here: those are the two routes where a person decides
//!   something, and a person is who a cookie names.
//! * A **bearer access token** reaches `/oidc/userinfo` and `/oidc/revoke` and
//!   nothing else anywhere — in particular nothing under `/api/*`, where no
//!   code reads one at all.
//! * A **client credential** reaches `/oidc/token` and `/oidc/revoke`, and is
//!   checked by the kernel against the method the *registration* names.
//!
//! Every write is a command. This module builds arguments, reads a reply and
//! shapes an answer; it authorises nothing, because the commands do that
//! inside their own transactions.

pub mod authorize;
pub mod discovery;
pub mod end_session;
pub mod error;
pub mod limits;
pub mod logout;
pub mod page;
pub mod request;
pub mod revoke;
pub mod token;
pub mod url;
pub mod userinfo;

use axum::Router;
use axum::routing::{get, post};
use rn_kernel::oidc::{jwt, key, paths};
use rn_kernel::{Principal, cmd::Ctx};

pub use end_session::Hint;
pub use limits::Limiter;

use crate::state::AppState;

/// The whole Provider, as one router.
///
/// Merged as a peer of the other feature routers ([`crate::router`]). A
/// downstream deployment mounts exactly this.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(paths::DISCOVERY, get(discovery::metadata))
        .route(paths::OAUTH_METADATA, get(discovery::metadata))
        .route(paths::JWKS, get(discovery::jwks))
        .route(paths::AUTHORIZE, get(authorize::get).post(authorize::post))
        .route(paths::TOKEN, post(token::post))
        .route(paths::USERINFO, get(userinfo::any).post(userinfo::any))
        .route(paths::REVOKE, post(revoke::post))
        .route(
            paths::END_SESSION,
            get(end_session::get).post(end_session::post),
        )
}

/// A command context, for the endpoints that run one.
///
/// Every endpoint here calls the kernel directly rather than going through
/// `/api/cmd/<name>`, and the reason is the secret: `authorize` mints a code
/// and the token endpoint mints a token pair, and those ride on the command's
/// own return type. The `/api/cmd` binding of the same commands deliberately
/// drops them.
pub fn context(
    state: &AppState,
    principal: Principal,
) -> Ctx<'_, hiqlite::Client, rn_kernel::feed::ClusterFeed> {
    Ctx {
        store: state.store.as_ref(),
        feed: state.feed.as_ref(),
        provider: state.provider.as_ref(),
        principal,
        key: uuid::Uuid::new_v4(),
    }
}

/// Validate an `id_token_hint` as one this deployment issued, and read the two
/// claims a hint is worth.
///
/// Written once and shared by the authorization endpoint and the sign-out one,
/// because they ask the same question and an answer that differed between them
/// would be a hole in whichever was looser.
///
/// **Expiry is not checked**, and that is the specification's instruction
/// (RP-Initiated Logout §2): the token is a *hint* about which session the
/// relying party believes is current, not a credential for it. Everything that
/// makes it trustworthy is checked — the signature, under a key this
/// deployment published, and the issuer.
pub async fn verified_hint(state: &AppState, hint: &str) -> Option<Hint> {
    let parts = jwt::split(hint)?;
    let kid = parts.kid()?;
    let row = key::by_kid(&state.store.reads(), kid).await.ok()??;
    let public = key::public_of(&row).ok()?;
    if !jwt::verify(&public, &parts) {
        return None;
    }
    if jwt::claim(&parts.claims, "iss") != Some(state.provider.issuer()) {
        return None;
    }
    Some(Hint {
        sub: jwt::claim(&parts.claims, "sub")?.to_owned(),
        audience: jwt::claim(&parts.claims, "aud")?.to_owned(),
    })
}
