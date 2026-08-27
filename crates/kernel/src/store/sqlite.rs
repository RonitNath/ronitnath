//! The in-process engine: the same SQLite, without the raft.
//!
//! Tests and benches run against this. It is not a mock — a mock would be a
//! hand-written stand-in for SQLite's behaviour, and the copy would drift from
//! the original the first time a `CHECK` constraint or an index mattered.
//! This is `rusqlite` executing the same statements, against the same
//! `migrations/1_kernel.sql`, with foreign keys on.
//!
//! One piece *is* reimplemented here: hiqlite's `Param::StmtOutput`, the
//! reference from a statement in a batch to an earlier statement's returned
//! column. Both engines therefore agree on what a batch means, and the
//! integration test against a live node is what proves the two agreements are
//! the same one.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OpenFlags};

use super::Value;
use super::sql::{Sql, Stmt, StoreError};
use super::value::{FromRow, OwnedRow};

/// A SQLite connection behind a lock, satisfying [`Sql`].
///
/// The lock is not a performance decision: hiqlite serialises every write
/// through one raft leader, so a single writer is the shape production has
/// too, and a benchmark run against a pool would be measuring something the
/// real system never does.
#[derive(Debug)]
pub struct Sqlite {
    conn: Mutex<Connection>,
    fail_at: AtomicI64,
}

impl Sqlite {
    /// Open an in-memory database and apply every migration.
    pub fn memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory_with_flags(
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )
        .map_err(backend)?;
        Self::prepare(conn)
    }

    /// Open a file-backed database and apply every migration. Benches use this
    /// when the point is to measure a commit that reaches a disk.
    pub fn open(path: &std::path::Path) -> Result<Self, StoreError> {
        Self::prepare(Connection::open(path).map_err(backend)?)
    }

    fn prepare(conn: Connection) -> Result<Self, StoreError> {
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(backend)?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(backend)?;
        for sql in super::migrations() {
            conn.execute_batch(&sql).map_err(backend)?;
        }
        Ok(Self {
            conn: Mutex::new(conn),
            fail_at: AtomicI64::new(NO_INJECTION),
        })
    }

    /// Make the next batch fail at the given statement, once.
    ///
    /// Fault injection at the *edge*, which is the only place it belongs: the
    /// command under test runs unmodified and the database is what
    /// misbehaves. It is what lets a test prove that a half-applied
    /// registration rolls back, rather than trusting the word "transaction".
    pub fn fail_next_batch_at(&self, index: usize) {
        self.fail_at.store(index as i64, Ordering::Relaxed);
    }

    /// Stop injecting failures.
    pub fn stop_failing(&self) {
        self.fail_at.store(NO_INJECTION, Ordering::Relaxed);
    }

    /// Run `EXPLAIN QUERY PLAN` and return its `detail` lines.
    ///
    /// The "no full scan" gate reads these: a plan naming a `SCAN` of a table
    /// this crate expects to reach by index is a regression, and the assertion
    /// says which index it wanted.
    pub fn explain(&self, sql: &str, params: Vec<Value>) -> Result<Vec<String>, StoreError> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .map_err(backend)?;
        for (i, value) in params.into_iter().enumerate() {
            stmt.raw_bind_parameter(i + 1, into_sql(value)?)
                .map_err(backend)?;
        }
        let mut rows = stmt.raw_query();
        let mut plan = Vec::new();
        while let Some(row) = rows.next().map_err(backend)? {
            plan.push(row.get::<_, String>("detail").map_err(backend)?);
        }
        Ok(plan)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Sql for Sqlite {
    async fn execute(&self, sql: &'static str, params: Vec<Value>) -> Result<usize, StoreError> {
        let conn = self.lock();
        let mut stmt = conn.prepare_cached(sql).map_err(backend)?;
        bind(&mut stmt, params, None)?;
        stmt.raw_execute().map_err(classify)
    }

    async fn query<T: FromRow + Send>(
        &self,
        sql: &'static str,
        params: Vec<Value>,
    ) -> Result<Vec<T>, StoreError> {
        let conn = self.lock();
        let mut stmt = conn.prepare_cached(sql).map_err(backend)?;
        let columns: Vec<String> = stmt.column_names().into_iter().map(str::to_owned).collect();
        bind(&mut stmt, params, None)?;
        let mut rows = stmt.raw_query();
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(classify)? {
            out.push(T::from_row(&mut read_row(row, &columns)?)?);
        }
        Ok(out)
    }

    async fn batch(&self, stmts: Vec<Stmt>) -> Result<Vec<usize>, StoreError> {
        let mut conn = self.lock();
        let txn = conn.transaction().map_err(backend)?;
        let mut outputs: HashMap<usize, (Vec<String>, Vec<Value>)> = HashMap::new();
        let mut affected = Vec::with_capacity(stmts.len());

        for (index, item) in stmts.into_iter().enumerate() {
            if self.fail_at.load(Ordering::Relaxed) == index as i64 {
                self.stop_failing();
                // Dropping the transaction unwritten is the rollback.
                return Err(StoreError::Backend(format!(
                    "injected failure at batch statement {index}"
                )));
            }
            let mut stmt = txn.prepare_cached(item.sql).map_err(backend)?;
            let columns: Vec<String> = stmt.column_names().into_iter().map(str::to_owned).collect();
            bind(&mut stmt, item.params, Some(&outputs))?;

            if columns.is_empty() {
                affected.push(stmt.raw_execute().map_err(classify)?);
                continue;
            }

            // A statement with output columns — a `RETURNING` clause — is
            // observable: its first row is what later statements reference.
            let mut rows = stmt.raw_query();
            let mut count = 0usize;
            let mut first: Option<Vec<Value>> = None;
            while let Some(row) = rows.next().map_err(classify)? {
                if first.is_none() {
                    let mut values = Vec::with_capacity(columns.len());
                    for i in 0..columns.len() {
                        values.push(from_sql(row.get::<_, SqlValue>(i).map_err(backend)?));
                    }
                    first = Some(values);
                }
                count += 1;
            }
            if let Some(values) = first {
                outputs.insert(index, (columns, values));
            }
            affected.push(count);
        }

        txn.commit().map_err(classify)?;
        Ok(affected)
    }
}

/// No statement index, so no injection.
const NO_INJECTION: i64 = -1;

type Outputs = HashMap<usize, (Vec<String>, Vec<Value>)>;

fn bind(
    stmt: &mut rusqlite::CachedStatement<'_>,
    params: Vec<Value>,
    outputs: Option<&Outputs>,
) -> Result<(), StoreError> {
    for (i, value) in params.into_iter().enumerate() {
        let value = match value {
            Value::StmtColumn { index, column } => {
                let outputs = outputs.ok_or_else(|| {
                    StoreError::Backend("a statement-output reference outside a batch".to_owned())
                })?;
                let (columns, values) = outputs.get(&index).ok_or_else(|| {
                    StoreError::Backend(format!("statement {index} returned no row to reference"))
                })?;
                let at = columns.iter().position(|c| c == column).ok_or_else(|| {
                    StoreError::Backend(format!("statement {index} has no column {column}"))
                })?;
                values[at].clone()
            }
            other => other,
        };
        stmt.raw_bind_parameter(i + 1, into_sql(value)?)
            .map_err(backend)?;
    }
    Ok(())
}

fn read_row(row: &rusqlite::Row<'_>, columns: &[String]) -> Result<OwnedRow, StoreError> {
    let mut out = Vec::with_capacity(columns.len());
    for (i, name) in columns.iter().enumerate() {
        out.push((
            name.clone(),
            from_sql(row.get::<_, SqlValue>(i).map_err(backend)?),
        ));
    }
    Ok(OwnedRow::new(out))
}

fn into_sql(value: Value) -> Result<SqlValue, StoreError> {
    Ok(match value {
        Value::Null => SqlValue::Null,
        Value::Int(v) => SqlValue::Integer(v),
        Value::Real(v) => SqlValue::Real(v),
        Value::Text(v) => SqlValue::Text(v),
        Value::Blob(v) => SqlValue::Blob(v),
        Value::StmtColumn { index, column } => {
            return Err(StoreError::Backend(format!(
                "unresolved reference to statement {index} column {column}"
            )));
        }
    })
}

fn from_sql(value: SqlValue) -> Value {
    match value {
        SqlValue::Null => Value::Null,
        SqlValue::Integer(v) => Value::Int(v),
        SqlValue::Real(v) => Value::Real(v),
        SqlValue::Text(v) => Value::Text(v),
        SqlValue::Blob(v) => Value::Blob(v),
    }
}

fn backend(err: rusqlite::Error) -> StoreError {
    StoreError::Backend(err.to_string())
}

/// Tell a constraint violation from a fault. Idempotency keys and duplicate
/// email addresses both arrive here, and both are ordinary answers.
fn classify(err: rusqlite::Error) -> StoreError {
    if let rusqlite::Error::SqliteFailure(inner, _) = &err
        && inner.code == rusqlite::ErrorCode::ConstraintViolation
    {
        return StoreError::Constraint(err.to_string());
    }
    StoreError::Backend(err.to_string())
}
