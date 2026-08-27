//! The production engine: hiqlite, which is SQLite behind raft.
//!
//! Every method here is a translation and nothing more. The interesting
//! decisions — what a batch means, when a guard has missed — live in
//! [`super::sql`] and apply to both engines; this file only carries values
//! across the boundary.

use hiqlite::{Client, Error as HiqliteError, Param, Params};

use super::Value;
use super::sql::{Sql, Stmt, StoreError};
use super::value::FromRow;

impl Sql for Client {
    async fn execute(&self, sql: &'static str, params: Vec<Value>) -> Result<usize, StoreError> {
        Client::execute(self, sql, into_params(params))
            .await
            .map_err(classify)
    }

    async fn query<T: FromRow + Send>(
        &self,
        sql: &'static str,
        params: Vec<Value>,
    ) -> Result<Vec<T>, StoreError> {
        let rows = Client::query_raw(self, sql, into_params(params))
            .await
            .map_err(classify)?;
        rows.into_iter()
            .map(|mut row| T::from_row(&mut row).map_err(StoreError::from))
            .collect()
    }

    async fn batch(&self, stmts: Vec<Stmt>) -> Result<Vec<usize>, StoreError> {
        let batch: Vec<(&'static str, Params)> = stmts
            .into_iter()
            .map(|stmt| (stmt.sql, into_params(stmt.params)))
            .collect();
        let results = Client::txn(self, batch).await.map_err(classify)?;
        results
            .into_iter()
            .map(|res| res.map_err(classify))
            .collect()
    }
}

fn into_params(values: Vec<Value>) -> Params {
    values
        .into_iter()
        .map(|value| match value {
            Value::Null => Param::Null,
            Value::Int(v) => Param::Integer(v),
            Value::Real(v) => Param::Real(v),
            Value::Text(v) => Param::Text(v),
            Value::Blob(v) => Param::Blob(v),
            Value::StmtColumn { index, column } => {
                Param::StmtOutputNamed(index, std::borrow::Cow::Borrowed(column))
            }
        })
        .collect()
}

fn classify(err: HiqliteError) -> StoreError {
    match err {
        HiqliteError::ConstraintViolation(msg) => StoreError::Constraint(msg),
        HiqliteError::QueryReturnedNoRows(_) => StoreError::NotFound,
        // hiqlite passes most rusqlite errors through as `Sqlite`, so the
        // constraint case has to be recognised by what SQLite said. Missing it
        // would turn a replayed idempotency key into a fault instead of the
        // replay it is, which is why this is matched rather than assumed.
        other => {
            let msg = other.to_string();
            if msg.contains("constraint failed") || msg.contains("ConstraintViolation") {
                StoreError::Constraint(msg)
            } else {
                StoreError::Backend(msg)
            }
        }
    }
}
