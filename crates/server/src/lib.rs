//! `rn-site` — the server: the kernel's API, the auth and claim pages, the
//! tier shells, ops routes, static assets and the public presence.
//!
//! Composition lives in [`router`]; every feature module owns its own
//! handlers, its templates and its tests. The surface is default-deny by
//! construction rather than by middleware: a route names the extractor it
//! needs, and the extractor is the authorisation. There is no request-level
//! guard that a new route could be added without.
//!
//! The trust boundaries (`docs/rebuild/plan.md` §Product contract) are three,
//! and each is a router:
//!
//! * **public** — [`presence`], [`assets`], [`ops`], `GET /auth`, and the
//!   [`product`] routes this deployment has turned on.
//! * **browser-session** — [`api`], [`sub`], [`shell`] and the `/auth` posts;
//!   the cookie, resolved to a [`Principal`](rn_kernel::Principal).
//! * **recipient/embed** — [`links`], and nothing else. It has its own router
//!   state so that its bearer extractor cannot be used anywhere but there.
//!
//! * **the OpenID Provider** — [`oidc`], which is its own boundary and its own
//!   router, mountable by a downstream deployment as it stands. The cookie
//!   reaches its two decision pages; a bearer access token reaches its
//!   `userinfo` and `revoke` and nothing under `/api/*`; a client credential
//!   reaches its token endpoint.
//!
//! The `service` party kind now has exactly one route that accepts it:
//! `client_credentials` at `/oidc/token`, which mints a token whose principal
//! is a client's own service party. What that party may *do* is whatever
//! relations somebody granted it.

pub mod api;
pub mod assets;
pub mod auth;
pub mod bootstrap;
pub mod config;
pub mod db;
pub mod http;
pub mod links;
pub mod observe;
pub mod oidc;
pub mod ops;
pub mod presence;
pub mod product;
pub mod shell;
pub mod shutdown;
pub mod state;
pub mod sub;
pub mod telemetry;

use axum::Router;

pub use state::AppState;

/// The whole HTTP surface. Feature routers merge as peers; the freshness and
/// deadline layers wrap all of them so no route can be added later that
/// forgets the version header, picks its own cache policy, or waits forever
/// for a quorum that is not coming.
pub fn router(state: AppState) -> Router {
    let session_surface = Router::new()
        .merge(ops::router())
        .merge(presence::router())
        .merge(assets::router())
        .merge(auth::router())
        .merge(shell::router())
        .merge(oidc::router())
        .merge(api::router())
        // Every product's routes, mounted always and answered only while the
        // product is on. The router is built once, here, at boot — which is
        // exactly why enabling a product cannot add a route and must instead
        // change what a route already mounted answers (`product`).
        .merge(product::router(&state))
        .with_state(state.clone());

    // The bearer surface carries its own state, which is what keeps its
    // extractor off every router above (`links`).
    let bearer_surface = links::router().with_state(links::LinkState(state.clone()));

    session_surface
        .merge(bearer_surface)
        // Inside the freshness layer, so a refusal still carries the version
        // header and the cache policy every other answer does.
        .layer(axum::middleware::from_fn(http::deadline))
        .layer(axum::middleware::from_fn_with_state(state, http::freshness))
        .layer(tower_http::trace::TraceLayer::new_for_http())
}
