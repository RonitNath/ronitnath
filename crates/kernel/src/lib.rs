//! The isoastra core model: ids, schema, store, principals, commands, audit
//! and the change feed.
//!
//! This crate is the whole authoritative half of rn-site. It holds no HTTP, no
//! templates and no rendering: `crates/server` turns requests into the calls
//! here, and nothing else in the workspace may write to the database.
//!
//! Three shapes carry the design (`docs/kernel/index.html`):
//!
//! * **Every change is a command.** [`cmd`] holds one module per command; each
//!   one authorises itself, writes its audit row inside its own transaction,
//!   and returns a typed [`Event`](event::Event). A status no command produces
//!   does not exist, which is why the status vocabularies are CHECK
//!   constraints rather than enums the code remembers to honour.
//! * **Read paths never write.** [`store::ReadStore`] is a type with no
//!   `execute`, so "nothing reachable from a GET writes" is a compile error
//!   rather than a review note. Observations — `last_seen`, match scanning —
//!   go on the async lane in [`observe`].
//! * **The audit row is the change feed.** `audit.id` is the offset a
//!   consumer resumes from; [`feed`] is the seam that rung 6 swaps for Zenoh
//!   without touching a caller.
//!
//! Ids that leave the process are encrypted ([`ids`]): the internal integer is
//! never serialised, so a forged or cross-type id fails to decrypt instead of
//! addressing the wrong row.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod audit;
pub mod cmd;
pub mod domain;
pub mod error;
pub mod event;
pub mod feed;
pub mod ids;
pub mod merge;
pub mod observe;
pub mod password;
pub mod principal;
pub mod store;
pub mod testing;

pub use error::{Decline, Invalid, KernelError, Outcome};
pub use event::{Committed, Event};
pub use ids::{Id, IdKey};
pub use principal::{Principal, SubjectSet};
pub use store::{Migrations, ReadStore, Sql, Store};

/// Unix seconds. Every instant in the schema is one of these.
pub type Timestamp = i64;

/// The change-feed offset: the `audit.id` of the command that produced it.
pub type Offset = u64;
