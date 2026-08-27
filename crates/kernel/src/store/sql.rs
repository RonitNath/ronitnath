//! The narrow database seam: three operations, two engines, one set of
//! statements.
//!
//! ## Why a transaction here is a batch
//!
//! hiqlite commits through raft, so a transaction is *submitted* — a list of
//! statements the leader applies together — and there is no way to read a row,
//! think about it, and write inside the same transaction. Everything the
//! kernel does therefore takes this shape:
//!
//! 1. **Read** what the decision needs, outside any transaction.
//! 2. **Submit** one batch whose every statement carries the same guard in its
//!    `WHERE` clause, together with the number of rows each is expected to
//!    affect.
//! 3. **Check** the affected counts. All-as-expected means the world had not
//!    moved between (1) and (2). All-zero means it had: nothing was written,
//!    because every statement was guarded, so the command retries the read
//!    once and gives up after that.
//!
//! Step 2's discipline is what makes step 3 safe: a batch whose statements do
//! *not* all carry the guard can apply half of itself, and
//! [`Store::commit`](super::Store::commit) reports that as an invariant
//! failure rather than pretending it was a decline.
//!
//! ## Why the SQL is `&'static str`
//!
//! Every statement in this crate is a literal. A `String` parameter here would
//! be the one place a command could interpolate a value into SQL, so the type
//! removes the option.

use std::future::Future;

use super::Value;
use super::value::{FromRow, RowError};

/// How many rows a statement in a batch must affect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    /// Exactly this many. The usual case: one insert, one row.
    Rows(usize),
    /// This many or more — a fan-out whose width the caller does not know.
    AtLeast(usize),
    /// Any number. For a statement whose count carries no information, such as
    /// a delete that is allowed to have already happened.
    Any,
}

impl Expect {
    /// Whether an observed count satisfies this expectation.
    pub const fn satisfied_by(self, rows: usize) -> bool {
        match self {
            Self::Rows(n) => rows == n,
            Self::AtLeast(n) => rows >= n,
            Self::Any => true,
        }
    }

    /// Whether an observed count means the statement's guard did not hold —
    /// which for a guarded statement is always "it changed nothing".
    pub const fn guard_missed(self, rows: usize) -> bool {
        match self {
            Self::Rows(n) | Self::AtLeast(n) => n > 0 && rows == 0,
            Self::Any => false,
        }
    }
}

/// One statement of a transaction batch, with the count it must affect.
#[derive(Debug, Clone)]
pub struct Stmt {
    /// The statement text. A literal, always.
    pub sql: &'static str,
    /// Its bound parameters, which may reference earlier statements' output.
    pub params: Vec<Value>,
    /// How many rows it must affect for the batch to have meant what the
    /// command intended.
    pub expect: Expect,
}

impl Stmt {
    /// A statement that must affect exactly one row — an insert, or an update
    /// of a row the command has already read.
    pub fn one(sql: &'static str, params: Vec<Value>) -> Self {
        Self {
            sql,
            params,
            expect: Expect::Rows(1),
        }
    }

    /// A statement whose affected count carries no information.
    pub fn any(sql: &'static str, params: Vec<Value>) -> Self {
        Self {
            sql,
            params,
            expect: Expect::Any,
        }
    }

    /// A statement that must affect exactly `rows` rows.
    pub fn rows(sql: &'static str, params: Vec<Value>, rows: usize) -> Self {
        Self {
            sql,
            params,
            expect: Expect::Rows(rows),
        }
    }
}

/// What the database refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    /// A UNIQUE or PRIMARY KEY constraint. Told apart from other failures
    /// because it is how idempotency keys and duplicate emails announce
    /// themselves, and both are ordinary outcomes rather than faults.
    #[error("constraint violation: {0}")]
    Constraint(String),
    /// A query that had to return a row returned none.
    #[error("no such row")]
    NotFound,
    /// A row came back in a shape the caller could not read.
    #[error("row: {0}")]
    Row(#[from] RowError),
    /// A batch applied, but not the rows the command expected — and not
    /// uniformly zero either, so some statement was missing its guard. This is
    /// a bug in the command, not a decline.
    #[error("batch statement {index} affected {rows} rows, expected {expected:?}")]
    BatchMismatch {
        /// Which statement disagreed.
        index: usize,
        /// What it actually affected.
        rows: usize,
        /// What it was supposed to affect.
        expected: Expect,
    },
    /// The guard did not hold: nothing was written and the caller may retry.
    #[error("guard did not hold")]
    GuardMissed,
    /// Anything else the engine said.
    #[error("database: {0}")]
    Backend(String),
}

/// The database, as the kernel uses it.
///
/// Implemented for `hiqlite::Client` (production, raft-replicated) and for an
/// in-process `rusqlite` connection (tests and benches). The second is not a
/// mock: it is the same SQLite running the same statements, so a test proves
/// the SQL, not a description of it. What it does not prove is replication,
/// which is why one integration test runs a real hiqlite node.
pub trait Sql: Send + Sync {
    /// Run one statement, returning the rows it affected.
    fn execute(
        &self,
        sql: &'static str,
        params: Vec<Value>,
    ) -> impl Future<Output = Result<usize, StoreError>> + Send;

    /// Run one query, mapping each row.
    fn query<T: FromRow + Send>(
        &self,
        sql: &'static str,
        params: Vec<Value>,
    ) -> impl Future<Output = Result<Vec<T>, StoreError>> + Send;

    /// Apply a batch as one transaction, returning each statement's affected
    /// count in order. The batch rolls back if any statement errors; the
    /// affected counts are checked by the caller.
    fn batch(
        &self,
        stmts: Vec<Stmt>,
    ) -> impl Future<Output = Result<Vec<usize>, StoreError>> + Send;
}
