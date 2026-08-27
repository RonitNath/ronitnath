//! The harness tests, benches and the integration test share.
//!
//! It is public because a benchmark is a separate crate and cannot reach into
//! `#[cfg(test)]`, and because an integration test that built its own fixtures
//! would be proving a second registration path rather than the real one. Every
//! constant here is obviously fake and says so.

mod node;

pub use node::{KernelCache, Node};

use std::sync::Arc;

use uuid::Uuid;

use crate::cmd::{Ctx, Minted};
use crate::error::Outcome;
use crate::feed::{Feed, LocalFeed};
use crate::ids::IdKey;
use crate::principal::Principal;
use crate::store::{Clock, Sql, Sqlite, Store};

/// An obviously fake id key. Not a deployment's, and never will be.
pub const TEST_ID_KEY: &str = "0f0e0d0c0b0a09080706050403020100";

/// A fixed instant every fixture starts at, so a failure reads the same twice.
pub const TEST_EPOCH: crate::Timestamp = 1_800_000_000;

/// A password that is long enough to be accepted and obvious enough that
/// nobody mistakes it for a real one.
pub const TEST_PASSWORD: &str = "an obviously fake test password";

/// A store, a feed and a clock a test can move.
pub struct Harness<S: Sql> {
    /// The change feed, which also owns the store.
    pub feed: LocalFeed<S>,
    /// The issuer and sealing key the OpenID Provider's commands run under.
    /// A fixture's, never a deployment's: the seal is minted per harness.
    pub provider: crate::oidc::Provider,
}

/// The in-process harness: real SQLite, real migrations, no raft.
pub type Local = Harness<Sqlite>;

impl Local {
    /// A fresh in-memory database with the schema applied.
    ///
    /// # Panics
    ///
    /// If SQLite will not open in memory, which means the test cannot run.
    pub fn new() -> Self {
        let store = Store::new(
            Sqlite::memory().expect("an in-memory database opens"),
            IdKey::from_hex(TEST_ID_KEY).expect("the test key is well formed"),
            Clock::fixed(TEST_EPOCH),
        );
        Self::over(store)
    }
}

impl Default for Local {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Sql> Harness<S> {
    /// Wrap a store that is already open and migrated.
    pub fn over(store: Store<S>) -> Self {
        Self {
            feed: LocalFeed::new(Arc::new(store)),
            provider: crate::oidc::Provider::dev(),
        }
    }

    /// The store.
    pub fn store(&self) -> &Store<S> {
        self.feed.store().as_ref()
    }

    /// Move the clock, for expiry and throttle cases.
    pub fn advance(&self, seconds: i64) {
        self.store().clock().advance(seconds);
    }

    /// A context for a principal, with a fresh idempotency key.
    pub fn ctx(&self, principal: Principal) -> Ctx<'_, S, LocalFeed<S>> {
        self.ctx_keyed(principal, Uuid::new_v4())
    }

    /// A context with the key spelled out, for replay and conflict cases.
    pub fn ctx_keyed(&self, principal: Principal, key: Uuid) -> Ctx<'_, S, LocalFeed<S>> {
        Ctx {
            store: self.store(),
            feed: &self.feed,
            provider: &self.provider,
            principal,
            key,
            // A fixture is a dev process: the commands that are gated on it
            // have tests of their own for the off case.
            impersonation: true,
        }
    }

    /// Register somebody, and hand back the principal their session resolves
    /// to along with the token.
    ///
    /// Goes through the real command, so a fixture cannot drift from what
    /// registration actually writes.
    pub async fn register(&self, display: &str, email: &str) -> Outcome<Registered>
    where
        LocalFeed<S>: Feed,
    {
        let minted = crate::cmd::register(
            &self.ctx(Principal::Anonymous),
            &rn_api::commands::Register {
                display_name: display.to_owned(),
                // Derived from the whole address, so two fixtures never
                // collide on a handle the way two local parts would.
                handle: crate::oidc::handle::fixture(email),
                email: email.to_owned(),
                password: TEST_PASSWORD.to_owned(),
            },
        )
        .await?;
        self.resolved(minted).await
    }

    /// Sign somebody in with the harness password.
    pub async fn sign_in(&self, email: &str) -> Outcome<Registered>
    where
        LocalFeed<S>: Feed,
    {
        let minted = crate::cmd::sign_in(
            &self.ctx(Principal::Anonymous),
            &rn_api::commands::SignIn {
                email: email.to_owned(),
                password: TEST_PASSWORD.to_owned(),
            },
        )
        .await?;
        self.resolved(minted).await
    }

    async fn resolved(&self, minted: Minted) -> Outcome<Registered> {
        let token = minted.token.expect("a fresh mint returns its token");
        let resolved = crate::principal::resolve(self.store(), &token).await?;
        Ok(Registered {
            principal: resolved.principal,
            token,
            offset: minted.committed.offset,
        })
    }
}

/// Somebody who exists, with a live session.
#[derive(Debug)]
pub struct Registered {
    /// What their cookie resolves to.
    pub principal: Principal,
    /// Their session token.
    pub token: crate::domain::Token,
    /// The offset their registration landed at.
    pub offset: crate::Offset,
}
