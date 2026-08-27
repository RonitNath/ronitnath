//! Storage: the migrations, the engine seam, and the two handles a caller gets.
//!
//! [`Store`] reads and writes. [`ReadStore`] only reads — not by convention
//! but by construction: it has no `execute` and no `commit`, so "nothing
//! reachable from a GET writes" is a property of the type a query handler is
//! handed rather than a rule somebody has to remember. The test at the bottom
//! of this module proves the absence at compile time.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

mod replicated;
mod sql;
mod sqlite;
#[cfg(test)]
pub(crate) mod tests;
mod value;

pub use sql::{Expect, Sql, Stmt, StoreError};
pub use sqlite::Sqlite;
pub use value::{Count, Cursor, FromRow, OwnedRow, RowError, RowId, Value};

use crate::Timestamp;
use crate::ids::IdKey;

/// The migration files, embedded so a binary carries its own schema and a
/// node cannot boot against a directory that has drifted.
#[derive(rust_embed::Embed)]
#[folder = "migrations"]
pub struct Migrations;

/// Every migration's SQL, in application order.
///
/// hiqlite applies these itself through raft (`Client::migrate::<Migrations>`)
/// and records their hashes, so this is only for the in-process engine — but
/// it reads the same embedded files, which is what stops a test proving a
/// schema the cluster does not have.
pub fn migrations() -> Vec<String> {
    let mut files: Vec<(u32, String)> = Migrations::iter()
        .filter_map(|name| {
            let (id, _) = name.split_once('_')?;
            let sql = Migrations::get(name.as_ref())?;
            Some((
                id.parse().ok()?,
                String::from_utf8_lossy(&sql.data).into_owned(),
            ))
        })
        .collect();
    files.sort_by_key(|(id, _)| *id);
    files.into_iter().map(|(_, sql)| sql).collect()
}

/// Where "now" comes from.
///
/// Sessions expire and observations are throttled, so a test that could not
/// move time would have to sleep through it. `Clock::fixed` is not a fake
/// clock in the sense that matters — nothing about the logic under test is
/// replaced, only the reading it takes.
#[derive(Debug, Clone)]
pub enum Clock {
    /// The system clock, in unix seconds.
    System,
    /// A clock a test advances by hand.
    Fixed(Arc<AtomicI64>),
}

impl Clock {
    /// A clock starting at `at`, which a test moves with [`Self::advance`].
    pub fn fixed(at: Timestamp) -> Self {
        Self::Fixed(Arc::new(AtomicI64::new(at)))
    }

    /// The current unix second.
    pub fn now(&self) -> Timestamp {
        match self {
            Self::System => std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64),
            Self::Fixed(at) => at.load(Ordering::Relaxed),
        }
    }

    /// Move a fixed clock forward. A no-op on the system clock.
    pub fn advance(&self, seconds: i64) {
        if let Self::Fixed(at) = self {
            at.fetch_add(seconds, Ordering::Relaxed);
        }
    }
}

/// Read access to the database, and nothing else.
///
/// Both handles implement this; only [`Store`] adds anything to it.
pub trait Reads: Send + Sync {
    /// Run a query.
    fn query<T: FromRow + Send>(
        &self,
        sql: &'static str,
        params: Vec<Value>,
    ) -> impl Future<Output = Result<Vec<T>, StoreError>> + Send;

    /// The key public ids are derived under.
    fn ids(&self) -> &IdKey;

    /// The current unix second.
    fn now(&self) -> Timestamp;

    /// Run a query that must return exactly one row.
    fn query_one<T: FromRow + Send>(
        &self,
        sql: &'static str,
        params: Vec<Value>,
    ) -> impl Future<Output = Result<T, StoreError>> + Send {
        async move {
            self.query::<T>(sql, params)
                .await?
                .pop()
                .ok_or(StoreError::NotFound)
        }
    }

    /// Run a query that returns at most one row.
    fn query_opt<T: FromRow + Send>(
        &self,
        sql: &'static str,
        params: Vec<Value>,
    ) -> impl Future<Output = Result<Option<T>, StoreError>> + Send {
        async move { Ok(self.query::<T>(sql, params).await?.pop()) }
    }
}

/// Write access. Implemented only by [`Store`]; its whole job is to be
/// missing from [`ReadStore`], which the test below asserts.
pub trait Writes: Reads {}

/// The database, as a command sees it.
#[derive(Debug)]
pub struct Store<S: Sql> {
    sql: S,
    key: IdKey,
    clock: Clock,
}

impl<S: Sql> Store<S> {
    /// Wrap an engine.
    pub fn new(sql: S, key: IdKey, clock: Clock) -> Self {
        Self { sql, key, clock }
    }

    /// The engine, for the feed's own listen/notify calls.
    pub fn engine(&self) -> &S {
        &self.sql
    }

    /// The clock, so a test can move it.
    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// A read-only view, for query handlers and for `check()`.
    pub fn reads(&self) -> ReadStore<'_, S> {
        ReadStore(self)
    }

    /// Run one statement outside a transaction. For the observation lane and
    /// for the expiry sweep — never for anything an authorisation reads.
    pub async fn execute(
        &self,
        sql: &'static str,
        params: Vec<Value>,
    ) -> Result<usize, StoreError> {
        self.sql.execute(sql, params).await
    }

    /// Apply a guarded batch as one transaction, and check that it meant what
    /// the command intended.
    ///
    /// Every statement in `batch` must carry the command's guard in its
    /// `WHERE` clause. Given that, the three outcomes are exhaustive:
    ///
    /// * every count as expected — the world had not moved; the command holds.
    /// * every guarded count zero — the world had moved; *nothing* was
    ///   written, and the caller re-reads and tries once more.
    /// * anything else — some statement was missing its guard and the batch
    ///   applied in part. That is a bug, and it is reported as one rather than
    ///   dressed up as a decline.
    pub async fn commit(&self, batch: Vec<Stmt>) -> Result<Vec<usize>, StoreError> {
        let expected: Vec<Expect> = batch.iter().map(|stmt| stmt.expect).collect();
        let affected = self.sql.batch(batch).await?;

        let mut guarded = 0usize;
        let mut missed = 0usize;
        for (expect, rows) in expected.iter().zip(&affected) {
            if !matches!(expect, Expect::Any) {
                guarded += 1;
                if expect.guard_missed(*rows) {
                    missed += 1;
                }
            }
        }
        if guarded > 0 && missed == guarded {
            return Err(StoreError::GuardMissed);
        }

        for (index, (expect, rows)) in expected.into_iter().zip(&affected).enumerate() {
            if !expect.satisfied_by(*rows) {
                return Err(StoreError::BatchMismatch {
                    index,
                    rows: *rows,
                    expected: expect,
                });
            }
        }
        Ok(affected)
    }
}

impl<S: Sql> Reads for Store<S> {
    fn query<T: FromRow + Send>(
        &self,
        sql: &'static str,
        params: Vec<Value>,
    ) -> impl Future<Output = Result<Vec<T>, StoreError>> + Send {
        self.sql.query(sql, params)
    }

    fn ids(&self) -> &IdKey {
        &self.key
    }

    fn now(&self) -> Timestamp {
        self.clock.now()
    }
}

impl<S: Sql> Writes for Store<S> {}

/// A read-only view of a [`Store`].
///
/// It borrows rather than owns, so handing one out costs nothing and cannot
/// outlive the store it came from. What it does *not* have is any way to
/// write, and that is the point — a query handler holds one of these, so
/// "nothing reachable from a GET writes" is checked by the compiler:
///
/// ```compile_fail
/// # use rn_kernel::store::{Clock, Reads, Sqlite, Store};
/// # use rn_kernel::ids::IdKey;
/// # async fn f() {
/// let store = Store::new(
///     Sqlite::memory().expect("opens"),
///     IdKey::from_hex("0f0e0d0c0b0a09080706050403020100").expect("fake test key"),
///     Clock::System,
/// );
/// let read = store.reads();
/// read.execute("DELETE FROM session", vec![]).await.expect("no such method");
/// # }
/// ```
///
/// ```compile_fail
/// # use rn_kernel::store::{Clock, Reads, Sqlite, Store, Stmt};
/// # use rn_kernel::ids::IdKey;
/// # async fn f() {
/// # let store = Store::new(
/// #     Sqlite::memory().expect("opens"),
/// #     IdKey::from_hex("0f0e0d0c0b0a09080706050403020100").expect("fake test key"),
/// #     Clock::System,
/// # );
/// let read = store.reads();
/// read.commit(vec![Stmt::any("DELETE FROM session", vec![])]).await.expect("no such method");
/// # }
/// ```
///
/// Reading, of course, works:
///
/// ```
/// # use rn_kernel::store::{Clock, Count, Reads, Sqlite, Store};
/// # use rn_kernel::ids::IdKey;
/// # tokio_test_block(async {
/// let store = Store::new(
///     Sqlite::memory().expect("opens"),
///     IdKey::from_hex("0f0e0d0c0b0a09080706050403020100").expect("fake test key"),
///     Clock::System,
/// );
/// let rows: Vec<Count> = store
///     .reads()
///     .query("SELECT count(*) AS n FROM party", vec![])
///     .await
///     .expect("query runs");
/// assert_eq!(rows[0].0, 0);
/// # });
/// # fn tokio_test_block<F: std::future::Future>(f: F) -> F::Output {
/// #     tokio::runtime::Builder::new_current_thread().build().expect("runtime").block_on(f)
/// # }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ReadStore<'a, S: Sql>(&'a Store<S>);

impl<S: Sql> Reads for ReadStore<'_, S> {
    fn query<T: FromRow + Send>(
        &self,
        sql: &'static str,
        params: Vec<Value>,
    ) -> impl Future<Output = Result<Vec<T>, StoreError>> + Send {
        self.0.query(sql, params)
    }

    fn ids(&self) -> &IdKey {
        self.0.ids()
    }

    fn now(&self) -> Timestamp {
        self.0.now()
    }
}
