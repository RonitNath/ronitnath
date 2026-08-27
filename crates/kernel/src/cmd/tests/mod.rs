//! Every command's contract, against the real schema.
//!
//! Split by what is being exercised rather than by which module the code lives
//! in, because a command's contract is a story about several tables and the
//! test that tells it belongs beside the others about the same story.

mod factors;
mod identity;
mod sessions;
mod status;

/// What every file here needs. A prelude rather than eight repeated import
/// blocks: these tests all reach for the same dozen names.
mod prelude {
    pub(super) use rn_api::commands::{
        AddFactor, Disable, Enable, FactorKind, Register, RemoveFactor, RevokeSession, SignIn,
        SignOut, VerifyEmail,
    };
    pub(super) use uuid::Uuid;

    pub(super) use crate::bind;
    pub(super) use crate::cmd::*;
    pub(super) use crate::domain::{PASSWORD_MIN, SESSION_TTL};
    pub(super) use crate::error::Invalid;
    pub(super) use crate::event::Event;
    pub(super) use crate::ids;
    pub(super) use crate::principal::Principal;
    pub(super) use crate::store::{Count, Reads};
    pub(super) use crate::testing::{Local, TEST_PASSWORD};

    /// A registration with the harness password, for the cases that need the
    /// arguments rather than the result.
    pub(super) fn registration(email: &str) -> Register {
        Register {
            display_name: "Ronit".into(),
            email: email.into(),
            password: TEST_PASSWORD.into(),
        }
    }

    /// Run a `SELECT count(*) AS n` and return the number.
    pub(super) async fn count(harness: &Local, sql: &'static str) -> i64 {
        harness
            .store()
            .query::<Count>(sql, bind![])
            .await
            .expect("query runs")[0]
            .0
    }
}
