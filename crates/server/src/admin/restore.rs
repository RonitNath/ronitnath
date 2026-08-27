//! `rn-site admin restore <dir>` — the walk's inverse, onto an empty
//! formation.
//!
//! Restore is a **formation act, not a merge** (requirement F16.3), and the
//! three refusals below are all that sentence means in practice.
//!
//! * **A non-empty database is refused outright.** There is no "merge",
//!   because there is no rule that could decide what a colliding rowid means —
//!   and rowids are not incidental here: a public id *is* an encryption of the
//!   rowid, so restoring row 7 as row 12 renames it for everybody.
//! * **An id-key mismatch is refused.** The manifest carries a fingerprint of
//!   the key the source deployment derived its public ids under. Restoring
//!   under a different one produces a database whose rows are all correct and
//!   whose every public id is different — a link somebody saved, a URL
//!   somebody bookmarked, an `id_token`'s `sub`. That is exactly the failure
//!   the requirement says must be a refusal "rather than silently renaming
//!   every object".
//! * **A schema mismatch is refused.** The manifest names the migration
//!   history the rows came out of. A restore into a different one would be
//!   inserting a shape nobody has described.
//!
//! Rows go back in the order the manifest recorded — the walk's dependency
//! order — with their rowids intact, which is what makes the round trip's
//! acceptance ("every public id resolves to the same rows") true rather than
//! approximately true.

use std::path::Path;

use base64::Engine as _;
use hiqlite::{Param, Params};
use serde_json::{Value, json};

use super::AdminError;
use super::backup::{Kind, Manifest};
use crate::state::AppState;

/// How many rows go in one transaction. Big enough that a small deployment is
/// a handful of round trips, small enough that one batch is not the whole
/// table in memory twice.
const CHUNK: usize = 500;

/// Read `dir` and put it back.
///
/// # Errors
///
/// [`AdminError::Usage`] when the directory is not a backup,
/// [`AdminError::Refused`] for each of the three refusals above, and
/// [`AdminError::Store`] when an insert fails.
pub async fn read(state: &AppState, dir: &Path) -> Result<super::Report, AdminError> {
    let path = dir.join("manifest.json");
    let raw = std::fs::read_to_string(&path)
        .map_err(|error| AdminError::Usage(format!("cannot read {}: {error}", path.display())))?;
    let manifest: Manifest = serde_json::from_str(&raw).map_err(|error| {
        AdminError::Usage(format!(
            "{} is not a backup manifest: {error}",
            path.display()
        ))
    })?;
    if manifest.format != 1 {
        return Err(AdminError::Refused(format!(
            "this backup is format {} and this build writes and reads format 1",
            manifest.format
        )));
    }

    // The key first: it is the refusal that is invisible if it does not
    // happen, and the only one whose damage is not obvious afterwards.
    let here = super::backup::fingerprint(&state.config.require_id_key());
    if here != manifest.id_key {
        return Err(AdminError::Refused(format!(
            "this backup was taken under id key {} and this deployment's is {here}. Restoring it \
             would rename every object in it — every saved link, every bookmarked URL, every \
             token subject. Point RN_SITE__ID_KEY at the key this backup was taken under",
            manifest.id_key
        )));
    }

    let schema = super::backup::schema_digest(state).await?;
    if schema != manifest.schema {
        return Err(AdminError::Refused(format!(
            "this backup came out of schema {} and this deployment is on {schema}. A restore is \
             not a migration",
            manifest.schema
        )));
    }

    if let Some((table, rows)) = occupied(state, &manifest).await? {
        return Err(AdminError::Refused(format!(
            "this database is not empty — {table} already holds {rows} row(s). A restore is a \
             formation act, not a merge: wipe it first (`rn-site admin wipe`, or \
             `tools/ephemeral.sh reset`)"
        )));
    }

    let mut restored = 0u64;
    for table in &manifest.tables {
        restored += insert(state, dir, table).await?;
    }

    let head = super::backup::head(state).await?;
    let mut text = format!(
        "restored {restored} rows over {} tables from {}\n",
        manifest.tables.len(),
        dir.display()
    );
    text.push_str(&format!(
        "the feed head is {head}; the manifest was consistent at {}\n",
        manifest.offset
    ));
    if head != manifest.offset {
        text.push_str(
            "those differ, which means the audit table did not come back whole — read it before \
             trusting this instance\n",
        );
    }
    Ok(super::Report {
        text,
        body: json!({
            "restored": restored,
            "offset": manifest.offset,
            "head": head,
        }),
        offset: Some(head),
    })
}

/// The first table that already holds rows, if any does.
async fn occupied(
    state: &AppState,
    manifest: &Manifest,
) -> Result<Option<(String, i64)>, AdminError> {
    use hiqlite::params;
    for table in &manifest.tables {
        let name = super::backup::identifier(&table.name)?;
        let rows = state
            .db
            .query_raw(format!("SELECT count(*) AS n FROM \"{name}\""), params!())
            .await
            .map_err(AdminError::store)?;
        let count = match rows.into_iter().next() {
            Some(mut row) => row.try_get::<i64>("n").map_err(AdminError::store)?,
            // A table the manifest names and this schema does not have. The
            // schema digest already agreed, so this cannot happen; saying so
            // is cheaper than a panic if it ever does.
            None => continue,
        };
        if count > 0 {
            return Ok(Some((name, count)));
        }
    }
    Ok(None)
}

/// Put one table's rows back.
async fn insert(
    state: &AppState,
    dir: &Path,
    table: &super::backup::TableEntry,
) -> Result<u64, AdminError> {
    let name = super::backup::identifier(&table.name)?;
    let path = dir.join("tables").join(format!("{name}.jsonl"));
    let file = std::fs::read_to_string(&path)
        .map_err(|error| AdminError::Usage(format!("cannot read {}: {error}", path.display())))?;

    let columns = table
        .columns
        .iter()
        .map(|column| format!("\"{}\"", column.name))
        .collect::<Vec<_>>()
        .join(", ");
    let holders = (1..=table.columns.len())
        .map(|n| format!("${n}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!("INSERT INTO \"{name}\" ({columns}) VALUES ({holders})");

    let mut batch: Vec<(String, Params)> = Vec::with_capacity(CHUNK);
    let mut written = 0u64;
    for (line_number, line) in file.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let values: Vec<Value> = serde_json::from_str(line).map_err(|error| {
            AdminError::Usage(format!("{}:{}: {error}", path.display(), line_number + 1))
        })?;
        if values.len() != table.columns.len() {
            return Err(AdminError::Usage(format!(
                "{}:{}: {} values for {} columns",
                path.display(),
                line_number + 1,
                values.len(),
                table.columns.len()
            )));
        }
        let mut params = Params::with_capacity(values.len());
        for (value, column) in values.into_iter().zip(&table.columns) {
            params.push(param(value, column.kind).map_err(|what| {
                AdminError::Usage(format!(
                    "{}:{}: {}: {what}",
                    path.display(),
                    line_number + 1,
                    column.name
                ))
            })?);
        }
        batch.push((sql.clone(), params));
        if batch.len() == CHUNK {
            written += commit(state, std::mem::take(&mut batch)).await?;
        }
    }
    if !batch.is_empty() {
        written += commit(state, batch).await?;
    }

    if written != table.rows {
        return Err(AdminError::Store(format!(
            "{name}: the manifest counted {} rows and {written} went back",
            table.rows
        )));
    }
    Ok(written)
}

async fn commit(state: &AppState, batch: Vec<(String, Params)>) -> Result<u64, AdminError> {
    let size = batch.len() as u64;
    for result in state.db.txn(batch).await.map_err(AdminError::store)? {
        result.map_err(AdminError::store)?;
    }
    Ok(size)
}

/// One JSON value back into the storage class its column declares.
fn param(value: Value, kind: Kind) -> Result<Param, String> {
    if value.is_null() {
        return Ok(Param::Null);
    }
    match kind {
        Kind::Int => value
            .as_i64()
            .map(Param::Integer)
            .ok_or_else(|| format!("{value} is not an integer")),
        Kind::Real => value
            .as_f64()
            .map(Param::Real)
            .ok_or_else(|| format!("{value} is not a number")),
        Kind::Text => match value {
            Value::String(text) => Ok(Param::Text(text)),
            other => Err(format!("{other} is not text")),
        },
        Kind::Blob => value
            .get("b64")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{value} is not a {{\"b64\": …}} blob"))
            .and_then(|encoded| {
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|error| format!("the blob does not decode: {error}"))
            })
            .map(Param::Blob),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every storage class survives the round trip through JSON, and a blob
    /// survives it as bytes rather than as whatever text they happened to be.
    #[test]
    fn values_come_back_as_the_class_their_column_declares() {
        assert!(matches!(param(json!(7), Kind::Int), Ok(Param::Integer(7))));
        assert!(matches!(param(json!(null), Kind::Int), Ok(Param::Null)));
        assert!(matches!(param(json!(null), Kind::Blob), Ok(Param::Null)));
        assert!(matches!(param(json!("x"), Kind::Text), Ok(Param::Text(_))));
        assert!(matches!(param(json!(1.5), Kind::Real), Ok(Param::Real(_))));

        let bytes = vec![0u8, 255, 128, 7];
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        match param(json!({ "b64": encoded }), Kind::Blob) {
            Ok(Param::Blob(back)) => assert_eq!(back, bytes),
            other => panic!("{other:?}"),
        }
    }

    /// A value of the wrong shape is an error naming the line, never a silent
    /// coercion: a token hash restored as the *text* of its base64 is a
    /// session nobody can present and nothing would have said so.
    #[test]
    fn a_value_of_the_wrong_shape_is_refused() {
        assert!(param(json!("7"), Kind::Int).is_err());
        assert!(param(json!(7), Kind::Text).is_err());
        assert!(param(json!("not-an-object"), Kind::Blob).is_err());
        assert!(param(json!({ "b64": "not base64!!" }), Kind::Blob).is_err());
    }
}
