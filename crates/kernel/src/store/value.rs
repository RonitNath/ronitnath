//! The value and row vocabulary both engines speak.
//!
//! ## Parameters
//!
//! SQLite has five storage classes and so does [`Value`]. The sixth variant,
//! [`Value::StmtColumn`], is not a value at all but a *reference* to a column
//! of an earlier statement in the same transaction batch. It is what lets a
//! batch insert a party and then an identity pointing at it without a round
//! trip in between — see [`super::sql::Stmt`].
//!
//! ## Rows
//!
//! A row is read through [`Cursor`], not through a generic `get<T>`, because
//! the two engines cannot both be probed: hiqlite's owned row *removes* a
//! column as it converts it, so a failed attempt at `i64` would take the
//! column with it. Naming the type up front is therefore not ceremony — it is
//! the only way to read a row that works on both, and it makes each
//! [`FromRow`] impl a written statement of what its query returns.

use std::borrow::Cow;

use crate::ids::{Id, Table};

/// A bound parameter, or a reference to an earlier statement's output.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// SQL NULL.
    Null,
    /// An integer — every id, every unix-seconds instant, every boolean.
    Int(i64),
    /// A float. Only `match_candidate.score` uses one.
    Real(f64),
    /// Text.
    Text(String),
    /// Bytes. Token hashes are stored this way rather than as hex: 32 bytes
    /// instead of 64 characters, and no case to normalise.
    Blob(Vec<u8>),
    /// The named column of the first row returned by statement `index` of this
    /// transaction batch. Meaningless outside one, and refused there.
    StmtColumn {
        /// Zero-based position of the statement in the batch.
        index: usize,
        /// The column name, which that statement must have returned.
        column: &'static str,
    },
}

impl Value {
    /// Reference a column of an earlier statement in the same batch.
    pub const fn of_stmt(index: usize, column: &'static str) -> Self {
        Self::StmtColumn { index, column }
    }
}

macro_rules! from_int {
    ($($t:ty),+) => {$(
        impl From<$t> for Value {
            fn from(v: $t) -> Self {
                Self::Int(i64::from(v))
            }
        }
    )+};
}

from_int!(i8, i16, i32, i64, u8, u16, u32, bool);

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Self::Real(v)
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Self::Text(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Self::Text(v.to_owned())
    }
}

impl From<Cow<'_, str>> for Value {
    fn from(v: Cow<'_, str>) -> Self {
        Self::Text(v.into_owned())
    }
}

impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Self {
        Self::Blob(v)
    }
}

impl From<[u8; 32]> for Value {
    fn from(v: [u8; 32]) -> Self {
        Self::Blob(v.to_vec())
    }
}

impl<T: Table> From<Id<T>> for Value {
    fn from(v: Id<T>) -> Self {
        Self::Int(v.get())
    }
}

impl<T> From<Option<T>> for Value
where
    Value: From<T>,
{
    fn from(v: Option<T>) -> Self {
        v.map_or(Self::Null, Value::from)
    }
}

/// Bind a list of values into a parameter vector.
#[macro_export]
macro_rules! bind {
    () => { ::std::vec::Vec::<$crate::store::Value>::new() };
    ($($v:expr),+ $(,)?) => {
        ::std::vec![$($crate::store::Value::from($v)),+]
    };
}

/// Why a column could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RowError {
    /// The query did not return a column by that name, or not as that type.
    #[error("column {column} is missing or is not a {wanted}")]
    Column {
        /// The column that disagreed.
        column: String,
        /// The Rust type that was asked for.
        wanted: &'static str,
    },
}

impl RowError {
    fn at(column: &str, wanted: &'static str) -> Self {
        Self::Column {
            column: column.to_owned(),
            wanted,
        }
    }
}

/// One row of a result set, read column by column and type by type.
pub trait Cursor {
    /// Read an `INTEGER NOT NULL` column.
    fn int(&mut self, column: &str) -> Result<i64, RowError>;
    /// Read an `INTEGER` column that may be NULL.
    fn int_opt(&mut self, column: &str) -> Result<Option<i64>, RowError>;
    /// Read a `TEXT NOT NULL` column.
    fn text(&mut self, column: &str) -> Result<String, RowError>;
    /// Read a `TEXT` column that may be NULL.
    fn text_opt(&mut self, column: &str) -> Result<Option<String>, RowError>;
    /// Read a `BLOB NOT NULL` column.
    fn blob(&mut self, column: &str) -> Result<Vec<u8>, RowError>;
    /// Read a `REAL NOT NULL` column.
    fn real(&mut self, column: &str) -> Result<f64, RowError>;

    /// Read an id column.
    fn id<T: Table>(&mut self, column: &str) -> Result<Id<T>, RowError> {
        self.int(column).map(Id::new)
    }

    /// Read an id column that may be NULL.
    fn id_opt<T: Table>(&mut self, column: &str) -> Result<Option<Id<T>>, RowError> {
        Ok(self.int_opt(column)?.map(Id::new))
    }
}

/// A type that can be built from one row.
///
/// Written by hand for each row struct rather than derived: the column names
/// are the query's contract, and writing them out is where a rename gets
/// noticed.
pub trait FromRow: Sized {
    /// Read the row.
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError>;
}

/// A single-column count, for `SELECT count(*) AS n`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Count(pub i64);

impl FromRow for Count {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self(row.int("n")?))
    }
}

/// A single id column, for `INSERT … RETURNING id` and `SELECT id …`.
#[derive(Debug)]
pub struct RowId<T: Table>(pub Id<T>);

impl<T: Table> FromRow for RowId<T> {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self(row.id("id")?))
    }
}

/// A row held in memory, as the in-process engine produces it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OwnedRow {
    columns: Vec<(String, Value)>,
}

impl OwnedRow {
    /// Build a row from its columns.
    pub fn new(columns: Vec<(String, Value)>) -> Self {
        Self { columns }
    }

    fn take(&mut self, column: &str) -> Option<Value> {
        let index = self.columns.iter().position(|(name, _)| name == column)?;
        Some(self.columns.swap_remove(index).1)
    }
}

impl Cursor for OwnedRow {
    fn int(&mut self, column: &str) -> Result<i64, RowError> {
        match self.take(column) {
            Some(Value::Int(v)) => Ok(v),
            _ => Err(RowError::at(column, "i64")),
        }
    }

    fn int_opt(&mut self, column: &str) -> Result<Option<i64>, RowError> {
        match self.take(column) {
            Some(Value::Int(v)) => Ok(Some(v)),
            Some(Value::Null) => Ok(None),
            _ => Err(RowError::at(column, "Option<i64>")),
        }
    }

    fn text(&mut self, column: &str) -> Result<String, RowError> {
        match self.take(column) {
            Some(Value::Text(v)) => Ok(v),
            _ => Err(RowError::at(column, "String")),
        }
    }

    fn text_opt(&mut self, column: &str) -> Result<Option<String>, RowError> {
        match self.take(column) {
            Some(Value::Text(v)) => Ok(Some(v)),
            Some(Value::Null) => Ok(None),
            _ => Err(RowError::at(column, "Option<String>")),
        }
    }

    fn blob(&mut self, column: &str) -> Result<Vec<u8>, RowError> {
        match self.take(column) {
            Some(Value::Blob(v)) => Ok(v),
            _ => Err(RowError::at(column, "Vec<u8>")),
        }
    }

    fn real(&mut self, column: &str) -> Result<f64, RowError> {
        match self.take(column) {
            Some(Value::Real(v)) => Ok(v),
            Some(Value::Int(v)) => Ok(v as f64),
            _ => Err(RowError::at(column, "f64")),
        }
    }
}

impl Cursor for hiqlite::Row<'_> {
    fn int(&mut self, column: &str) -> Result<i64, RowError> {
        self.try_get(column)
            .map_err(|_| RowError::at(column, "i64"))
    }

    fn int_opt(&mut self, column: &str) -> Result<Option<i64>, RowError> {
        self.try_get(column)
            .map_err(|_| RowError::at(column, "Option<i64>"))
    }

    fn text(&mut self, column: &str) -> Result<String, RowError> {
        self.try_get(column)
            .map_err(|_| RowError::at(column, "String"))
    }

    fn text_opt(&mut self, column: &str) -> Result<Option<String>, RowError> {
        self.try_get(column)
            .map_err(|_| RowError::at(column, "Option<String>"))
    }

    fn blob(&mut self, column: &str) -> Result<Vec<u8>, RowError> {
        self.try_get(column)
            .map_err(|_| RowError::at(column, "Vec<u8>"))
    }

    fn real(&mut self, column: &str) -> Result<f64, RowError> {
        self.try_get(column)
            .map_err(|_| RowError::at(column, "f64"))
    }
}
