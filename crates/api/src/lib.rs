//! The rn-site wire vocabulary: every type that crosses the boundary between
//! the server and a browser bundle, and nothing else.
//!
//! This crate is compiled to wasm as part of each bundle, so it holds no
//! runtime, no clock and no I/O: instants are `i64` unix seconds, not
//! `SystemTime`. It also holds no behaviour beyond parsing and validating the
//! shapes themselves — authorisation, storage and command execution live in
//! `rn-kernel`, which is never sent to a browser.
//!
//! The shapes here are binding: `docs/rebuild/plan.md` §API is the contract
//! and this crate is its executable form.

#![forbid(unsafe_code)]

pub mod cluster;
pub mod command;
pub mod commands;
pub mod ids;
pub mod sub;
pub mod whoami;

#[cfg(test)]
mod testing;

pub use cluster::{ClusterView, RaftView, RaftViews};
pub use command::{Command, CommandEnvelope, CommandReply, Decline};
pub use ids::{IdKind, PublicId, PublicIdError};
pub use sub::{DiffOp, QueryRef, SubMessage, SubRequest};
pub use whoami::{
    DocRole, IdentityRef, MemberRole, OrganizationRef, PartyKind, PartyRef, PersonRef, Tier, Whoami,
};
