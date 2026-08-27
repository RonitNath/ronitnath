//! The pieces every rn-site bundle is built from.
//!
//! There are three bundles — `/app`, `/org`, `/platform` — and one of these:
//! the same shell, the same table, the same fields, the same client. A bundle
//! is a route table and its pages; everything else it does comes from here, so
//! a change to the chrome lands in all three at once and no surface keeps a
//! private copy of anything.
//!
//! The layers, outermost first:
//!
//! - [`shell`] — the tier chrome: rail, drawer, scope switcher, pinned footer.
//! - [`table`] and [`form`] — the two components an operator console is mostly
//!   made of. Tables sort, filter, page and elide by priority; fields sync
//!   themselves and there is no save button.
//! - [`live`] — a reconnecting subscription and the keyed row set it feeds.
//! - [`api`] — queries, commands and `whoami`, and nothing else.
//!
//! `tokens.css`, beside this file, is the one stylesheet: every bundle links
//! it and the askama pages embed it.

#![forbid(unsafe_code)]

pub mod api;
pub mod decline;
pub mod form;
pub mod live;
pub mod shell;
pub mod table;

pub use api::{ApiError, Committed, Invalid, command, invoke, query, whoami};
pub use decline::Decline;
pub use form::{Commit, SelectField, TextArea, TextField, ToggleField, sync_with};
pub use live::Live;
pub use shell::{NavItem, PageHead, Shell, use_whoami};
pub use table::{Column, Priority, RowAction, Table};

/// Wire this bundle up to the browser: panics get a stack trace in the
/// console instead of `unreachable executed`.
pub fn start() {
    console_error_panic_hook::set_once();
}
