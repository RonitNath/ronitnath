//! `rn-site admin backup <dir>` — the logical walk, and the manifest that
//! makes it a backup rather than a pile of rows.
//!
//! Why a walk and not hiqlite's own snapshot is `docs/ops-backup.md`, which is
//! this leg's first commit and the reason the rest of it exists. The short
//! version: hiqlite 0.14's backup surface is not compiled into this build, and
//! with it compiled in it is a copy of the state-machine file — which cannot
//! say what feed offset it is consistent at and carries no fingerprint of the
//! key its public ids were derived under.
//!
//! ## What "consistent at one offset" means here, exactly
//!
//! The walk runs against a **stopped** node. Nothing is writing, so "the
//! offset it is consistent at" is simply the audit head — and rather than
//! assert that, this reads the head before the walk and again after it and
//! refuses if it moved. A backup that cannot name its offset is not one
//! (F16.2), and one that names an offset it was not actually consistent at is
//! worse.
//!
//! ## What the manifest carries
//!
//! | field | why |
//! |---|---|
//! | `offset` | the audit head the walk was consistent at |
//! | `schema` | a digest over the applied migration history |
//! | `id_key` | a **fingerprint** of the id key — never the key |
//! | `tables` | the order the walk took, and each table's row count |
//!
//! The id key is never written, never printed and never logged. Public ids are
//! derived from it on read and stored in no column, so a backup directory that
//! contained it would be a backup directory that could rename every object in
//! any deployment it was carried to. The fingerprint is what lets a restore
//! *refuse* a mismatch — requirement F16.3's "rather than silently renaming
//! every object".

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use base64::Engine as _;
use hiqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::AdminError;
use crate::state::AppState;

/// The tables hiqlite owns. They describe the state machine, not the
/// deployment, and a restore onto a fresh formation has its own.
pub const NOT_OURS: &[&str] = &["_migrations", "_metadata"];

/// The manifest, as `manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// The format this file is written in. One so far.
    pub format: u32,
    /// The audit head the walk was consistent at.
    pub offset: u64,
    /// A digest over `_migrations`: id, name and hash of every applied step.
    pub schema: String,
    /// A fingerprint of the id key. **Not the key.**
    pub id_key: String,
    /// Which node wrote it.
    pub node: String,
    /// Which build wrote it.
    pub version: String,
    /// When, in unix seconds.
    pub at: i64,
    /// The tables, in the order the walk took them.
    pub tables: Vec<TableEntry>,
}

/// One table's place in the walk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableEntry {
    /// Its name, which is also its file's stem.
    pub name: String,
    /// The columns, in the order the rows are written.
    pub columns: Vec<Column>,
    /// How many rows. This is the acceptance test: it must equal
    /// `SELECT count(*)`.
    pub rows: u64,
}

/// One column, and how to read and write its values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    /// The column name.
    pub name: String,
    /// Its storage class, from the declared type.
    pub kind: Kind,
}

/// What a column holds, which is what says how to encode it as JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// `INTEGER` — every id, every unix second, every boolean.
    Int,
    /// `REAL`.
    Real,
    /// `TEXT`.
    Text,
    /// `BLOB` — written as `{"b64": "…"}`, because JSON has no bytes.
    Blob,
}

impl Kind {
    /// SQLite's declared-type rules, which are prefix rules and not a
    /// vocabulary: "INTEGER", "INT" and "BIGINT" are all integers.
    fn of(declared: &str) -> Option<Self> {
        let upper = declared.to_ascii_uppercase();
        if upper.contains("INT") {
            return Some(Self::Int);
        }
        if upper.contains("CHAR") || upper.contains("CLOB") || upper.contains("TEXT") {
            return Some(Self::Text);
        }
        if upper.contains("BLOB") {
            return Some(Self::Blob);
        }
        if upper.contains("REAL") || upper.contains("FLOA") || upper.contains("DOUB") {
            return Some(Self::Real);
        }
        None
    }
}

/// Walk the whole database into `dir`.
///
/// # Errors
///
/// [`AdminError::Store`] when the database refuses, [`AdminError::Refused`]
/// when the head moved under the walk, and [`AdminError::Usage`] when `dir`
/// cannot be written or is already a backup.
pub async fn write(state: &AppState, dir: &Path) -> Result<super::Report, AdminError> {
    if dir.join("manifest.json").exists() {
        return Err(AdminError::Usage(format!(
            "{} already holds a backup; name a directory that does not",
            dir.display()
        )));
    }
    let tables_dir = dir.join("tables");
    std::fs::create_dir_all(&tables_dir).map_err(|error| {
        AdminError::Usage(format!("cannot write {}: {error}", tables_dir.display()))
    })?;

    let opened = head(state).await?;
    let order = walk_order(state).await?;

    let mut entries = Vec::with_capacity(order.len());
    for name in &order {
        let columns = columns_of(state, name).await?;
        let rows = dump(state, name, &columns, &tables_dir).await?;
        entries.push(TableEntry {
            name: name.clone(),
            columns,
            rows,
        });
    }

    let closed = head(state).await?;
    if closed != opened {
        return Err(AdminError::Refused(format!(
            "the feed head moved from {opened} to {closed} while the walk ran, so this backup is \
             consistent at no offset at all. Stop the node and run it again"
        )));
    }

    let manifest = Manifest {
        format: 1,
        offset: opened,
        schema: schema_digest(state).await?,
        id_key: fingerprint(&state.config.require_id_key()),
        node: state.node.to_string(),
        version: state.version.to_string(),
        at: {
            use rn_kernel::store::Reads as _;
            state.store.reads().now()
        },
        tables: entries,
    };
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|error| AdminError::Store(error.to_string()))?;
    std::fs::write(dir.join("manifest.json"), format!("{json}\n"))
        .map_err(|error| AdminError::Usage(format!("cannot write the manifest: {error}")))?;

    let total: u64 = manifest.tables.iter().map(|table| table.rows).sum();
    let mut text = format!(
        "backup at offset {} — {total} rows over {} tables, into {}\n",
        manifest.offset,
        manifest.tables.len(),
        dir.display()
    );
    for table in &manifest.tables {
        if table.rows > 0 {
            text.push_str(&format!("  {:<20} {}\n", table.name, table.rows));
        }
    }
    Ok(super::Report {
        text,
        body: json!({ "backup": serde_json::to_value(&manifest).unwrap_or(Value::Null) }),
        offset: Some(manifest.offset),
    })
}

/// The audit head — the offset every command's `Committed` reports.
pub(crate) async fn head(state: &AppState) -> Result<u64, AdminError> {
    use rn_kernel::feed::Feed as _;
    state.feed.head().await.map_err(AdminError::store)
}

/// A digest over the applied migration history.
///
/// Not the embedded history: what a restore has to agree with is the shape the
/// *database* is in, and `_migrations` is where that is recorded. `digest_of`
/// is the kernel's sha-256-over-canonical-JSON, borrowed rather than a second
/// hashing scheme introduced here.
pub(crate) async fn schema_digest(state: &AppState) -> Result<String, AdminError> {
    let rows = state
        .db
        .query_raw(
            "SELECT id, name, hash FROM _migrations ORDER BY id ASC",
            params!(),
        )
        .await
        .map_err(AdminError::store)?;
    let mut steps = Vec::with_capacity(rows.len());
    for mut row in rows {
        let id: i64 = row.try_get("id").map_err(AdminError::store)?;
        let name: String = row.try_get("name").map_err(AdminError::store)?;
        let hash: String = row.try_get("hash").map_err(AdminError::store)?;
        steps.push(format!("{id}_{name}:{hash}"));
    }
    Ok(rn_kernel::audit::digest_of(&steps))
}

/// A fingerprint of the id key, and never the key.
///
/// Sixteen characters of the kernel's own digest over a domain-separated
/// string. Sixteen base64 characters is 96 bits, which is not a number anybody
/// collides by accident and is far too little to be worth attacking: the
/// fingerprint's whole job is to answer "is this the same key", and it never
/// leaves a backup directory the operator already holds.
#[must_use]
pub fn fingerprint(id_key: &str) -> String {
    let mut digest =
        rn_kernel::audit::digest_of(&format!("rn-site id-key fingerprint v1:{id_key}"));
    digest.truncate(16);
    digest
}

// ------------------------------------------------------------------- walk ---

/// Every table this deployment owns, in an order that inserts cleanly.
///
/// Kahn's algorithm over the foreign-key graph, alphabetical inside each
/// layer so two runs of the same schema produce the same manifest. A
/// self-reference is not a dependency — `party` referencing `party` is one row
/// pointing at another, and there is no order that helps — and a genuine cycle
/// between two tables is appended in name order rather than refused, because
/// SQLite does not enforce foreign keys unless somebody turned them on and a
/// refusal would be this walk inventing a constraint the database does not
/// have.
pub(crate) async fn walk_order(state: &AppState) -> Result<Vec<String>, AdminError> {
    let mut names = tables(state).await?;
    names.sort();

    let known: BTreeSet<String> = names.iter().cloned().collect();
    let mut parents: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for name in &names {
        let mut of = BTreeSet::new();
        for parent in foreign_keys(state, name).await? {
            if parent != *name && known.contains(&parent) {
                of.insert(parent);
            }
        }
        parents.insert(name.clone(), of);
    }

    let mut order: Vec<String> = Vec::with_capacity(names.len());
    let mut placed: BTreeSet<String> = BTreeSet::new();
    while order.len() < names.len() {
        let ready: Vec<String> = names
            .iter()
            .filter(|name| !placed.contains(*name))
            .filter(|name| parents[*name].iter().all(|parent| placed.contains(parent)))
            .cloned()
            .collect();
        if ready.is_empty() {
            // A cycle. Take the rest in name order; see the doc above.
            for name in &names {
                if !placed.contains(name) {
                    order.push(name.clone());
                }
            }
            break;
        }
        for name in ready {
            placed.insert(name.clone());
            order.push(name);
        }
    }
    Ok(order)
}

/// Every table in the schema that is this deployment's rather than hiqlite's.
async fn tables(state: &AppState) -> Result<Vec<String>, AdminError> {
    let rows = state
        .db
        .query_raw(
            "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name ASC",
            params!(),
        )
        .await
        .map_err(AdminError::store)?;
    let mut names = Vec::with_capacity(rows.len());
    for mut row in rows {
        let name: String = row.try_get("name").map_err(AdminError::store)?;
        if name.starts_with("sqlite_") || NOT_OURS.contains(&name.as_str()) {
            continue;
        }
        names.push(identifier(&name)?);
    }
    Ok(names)
}

/// The tables one table points at.
async fn foreign_keys(state: &AppState, table: &str) -> Result<Vec<String>, AdminError> {
    let rows = state
        .db
        .query_raw(
            format!("SELECT \"table\" AS parent FROM pragma_foreign_key_list('{table}')"),
            params!(),
        )
        .await
        .map_err(AdminError::store)?;
    let mut parents = Vec::with_capacity(rows.len());
    for mut row in rows {
        parents.push(row.try_get::<String>("parent").map_err(AdminError::store)?);
    }
    Ok(parents)
}

/// One table's columns, in declaration order, with the storage class each one
/// is read and written as.
pub(crate) async fn columns_of(state: &AppState, table: &str) -> Result<Vec<Column>, AdminError> {
    let rows = state
        .db
        .query_raw(
            format!("SELECT cid, name, type FROM pragma_table_info('{table}') ORDER BY cid ASC"),
            params!(),
        )
        .await
        .map_err(AdminError::store)?;
    let mut columns = Vec::with_capacity(rows.len());
    for mut row in rows {
        let name: String = row.try_get("name").map_err(AdminError::store)?;
        let declared: String = row.try_get("type").unwrap_or_default();
        let kind = Kind::of(&declared).ok_or_else(|| {
            AdminError::Refused(format!(
                "{table}.{name} declares the type {declared:?}, which is not one of SQLite's five \
                 storage classes. The walk will not guess at how to read it"
            ))
        })?;
        columns.push(Column {
            name: identifier(&name)?,
            kind,
        });
    }
    if columns.is_empty() {
        return Err(AdminError::Refused(format!("{table} has no columns")));
    }
    Ok(columns)
}

/// Stream one table into `<dir>/<table>.jsonl`, one JSON array per row.
///
/// An array rather than an object: the column names are in the manifest once
/// each rather than on every row, which for `audit` is the difference between
/// a file and a file six times the size.
async fn dump(
    state: &AppState,
    table: &str,
    columns: &[Column],
    dir: &Path,
) -> Result<u64, AdminError> {
    use std::io::Write as _;

    let select = columns
        .iter()
        .map(|column| format!("\"{}\"", column.name))
        .collect::<Vec<_>>()
        .join(", ");
    let rows = state
        .db
        .query_raw(format!("SELECT {select} FROM \"{table}\""), params!())
        .await
        .map_err(AdminError::store)?;

    let path = dir.join(format!("{table}.jsonl"));
    let file = std::fs::File::create(&path)
        .map_err(|error| AdminError::Usage(format!("cannot write {}: {error}", path.display())))?;
    let mut out = std::io::BufWriter::new(file);

    let mut written = 0u64;
    for mut row in rows {
        let mut values = Vec::with_capacity(columns.len());
        for column in columns {
            values.push(
                read(&mut row, column).map_err(|error| {
                    AdminError::Store(format!("{table}.{}: {error}", column.name))
                })?,
            );
        }
        let line = serde_json::to_string(&Value::Array(values))
            .map_err(|error| AdminError::Store(error.to_string()))?;
        writeln!(out, "{line}").map_err(|error| AdminError::Store(error.to_string()))?;
        written += 1;
    }
    out.flush()
        .map_err(|error| AdminError::Store(error.to_string()))?;
    Ok(written)
}

/// One column of one row, as JSON.
fn read(row: &mut hiqlite::Row<'_>, column: &Column) -> Result<Value, hiqlite::Error> {
    Ok(match column.kind {
        Kind::Int => row
            .try_get::<Option<i64>>(&column.name)?
            .map_or(Value::Null, Value::from),
        Kind::Real => row
            .try_get::<Option<f64>>(&column.name)?
            .and_then(serde_json::Number::from_f64)
            .map_or(Value::Null, Value::Number),
        Kind::Text => row
            .try_get::<Option<String>>(&column.name)?
            .map_or(Value::Null, Value::from),
        Kind::Blob => row.try_get::<Option<Vec<u8>>>(&column.name)?.map_or(
            Value::Null,
            |bytes| json!({ "b64": base64::engine::general_purpose::STANDARD.encode(bytes) }),
        ),
    })
}

/// A name out of the schema, checked before it is interpolated into SQL.
///
/// The names come from `sqlite_master`, so they are already the database's own
/// — but they are interpolated rather than bound, because an identifier cannot
/// be a parameter, and "it came from a trusted place" is exactly the sentence
/// that precedes every injection. This is the check that makes the
/// interpolation a fact rather than a hope.
pub(crate) fn identifier(name: &str) -> Result<String, AdminError> {
    let ok = !name.is_empty()
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok {
        Ok(name.to_owned())
    } else {
        Err(AdminError::Refused(format!(
            "{name:?} is not a name this walk will put into a statement"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_types_map_to_storage_classes_the_way_sqlite_does() {
        assert_eq!(Kind::of("INTEGER"), Some(Kind::Int));
        assert_eq!(Kind::of("INT"), Some(Kind::Int));
        assert_eq!(Kind::of("TEXT"), Some(Kind::Text));
        assert_eq!(Kind::of("VARCHAR(40)"), Some(Kind::Text));
        assert_eq!(Kind::of("BLOB"), Some(Kind::Blob));
        assert_eq!(Kind::of("REAL"), Some(Kind::Real));
        assert_eq!(Kind::of("DOUBLE"), Some(Kind::Real));
        assert_eq!(Kind::of(""), None, "an undeclared column is not guessed at");
    }

    #[test]
    fn only_a_plain_identifier_reaches_a_statement() {
        for good in ["party", "_migrations", "audit_object", "a1"] {
            identifier(good).unwrap_or_else(|error| panic!("{good}: {error}"));
        }
        for bad in ["", "1table", "drop table party", "party;--", "pärty", "a-b"] {
            assert!(identifier(bad).is_err(), "{bad} was let through");
        }
    }

    /// The fingerprint identifies a key and does not carry it. Both halves are
    /// the requirement: F16.3 needs a restore to be able to *refuse* a
    /// mismatch, and the rail says the manifest never holds the key.
    #[test]
    fn the_fingerprint_names_a_key_without_being_one() {
        let key = "000102030405060708090a0b0c0d0e0f";
        let print = fingerprint(key);
        assert_eq!(print, fingerprint(key), "it is a function of the key");
        assert_ne!(print, fingerprint("0f0e0d0c0b0a09080706050403020100"));
        assert_eq!(print.len(), 16);
        assert!(!key.contains(&print) && !print.contains(key));
    }
}
