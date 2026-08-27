//! The platform operator's own commands: delegation, the shorter session, the
//! re-authentication window, impersonation, the disable cascade and the keys.
//!
//! The sweep in [`super::operator`] proves that an operator *reaches* every
//! command. These are about the things that are true only of an operator, and
//! each one is a rule that would be invisible in a functional test: a session
//! that is too long, a password that was presented too long ago, a hat that
//! outlived the head wearing it, an invitation that should have gone to sleep
//! with the person who minted it.
//!
//! One file per requirement group, and the helpers they share are here.

mod cascade;
mod delegation;
mod impersonation;
mod keys;
mod sessions;

use super::prelude::*;
use crate::store::Value;

/// A ctx whose deployment forbids impersonation, for requirement C11.3.
fn without_impersonation(
    harness: &Local,
    principal: Principal,
) -> crate::cmd::Ctx<'_, crate::store::Sqlite, crate::feed::LocalFeed<crate::store::Sqlite>> {
    let mut ctx = harness.ctx(principal);
    ctx.impersonation = false;
    ctx
}

async fn expires_at(harness: &Local, session: crate::ids::Id<crate::ids::Session>) -> i64 {
    count_of(
        harness,
        "SELECT expires_at AS n FROM session WHERE id = $1",
        bind![session],
    )
    .await
}

async fn auth_time(harness: &Local, session: crate::ids::Id<crate::ids::Session>) -> i64 {
    count_of(
        harness,
        "SELECT auth_time AS n FROM session WHERE id = $1",
        bind![session],
    )
    .await
}

/// [`super::prelude::count`], for a statement with parameters.
async fn count_of(harness: &Local, sql: &'static str, params: Vec<Value>) -> i64 {
    harness
        .store()
        .query::<Count>(sql, params)
        .await
        .expect("the query runs")[0]
        .0
}
