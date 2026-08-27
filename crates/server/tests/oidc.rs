//! The OpenID Provider, proved end to end.
//!
//! Two suites, in one binary because they share one node. [`rp`] is a relying
//! party: discovery, authorize, code, token, id-token validation against the
//! published JWKS, userinfo, refresh, revoke, and sign-out with a back-channel
//! receipt. [`negative`] is everything that must *not* work.

mod harness;

#[path = "oidc/mod.rs"]
mod oidc;
