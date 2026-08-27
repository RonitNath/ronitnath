//! Who holds `platform:* #operator`, and who granted it to them.
//!
//! Platform administration is a relation and not a column, so this is a read
//! of `relation` rather than of a flag: one seek on `relation_object_idx`,
//! which is `(object_kind, object_id)` and `platform:0` is one pair. The list
//! is therefore as short as the deployment's operator count and costs one
//! indexed range however many parties exist.
//!
//! `granted_by` is NULL on exactly one row and it is not missing data. The
//! bootstrap makes the *first* operator out of nothing — it fires only on a
//! deployment that has none, and refuses the moment one exists — so there was
//! nobody to attribute it to. `RN_SITE__BOOTSTRAP_OPERATOR_EMAIL` named them
//! before anybody could, which is why the screen says *granted by
//! configuration* rather than leaving a dash where a name goes: a dash is what
//! a page prints when it does not know, and this page does.

use rn_kernel::bind;
use rn_kernel::domain::{PartyKind, Vocabulary as _};
use rn_kernel::store::{Cursor, FromRow, Reads, RowError};
use rn_kernel::{Outcome, Timestamp};
use serde_json::json;

use super::{Params, Row};
use crate::api::party::public_of;

/// Every operator relation, oldest first — which is the order they were
/// granted in, and the first row is the bootstrap.
pub(super) const OPERATORS: &str = "SELECT r.subject_id, r.at, r.granted_by, \
       p.kind, p.display_name, p.handle, p.status, \
       g.display_name AS granted_display \
     FROM relation r \
     JOIN party p ON p.id = r.subject_id \
     LEFT JOIN identity gi ON gi.id = r.granted_by \
     LEFT JOIN party g ON g.id = gi.person_id \
     WHERE r.object_kind = 'platform' AND r.object_id = 0 \
       AND r.relation = 'operator' AND r.subject_kind = 'person' \
     ORDER BY r.id";

struct OperatorRow {
    subject_id: i64,
    kind: PartyKind,
    display_name: String,
    handle: Option<String>,
    status: String,
    at: Timestamp,
    granted_by: Option<i64>,
    granted_display: Option<String>,
}

impl FromRow for OperatorRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let raw = row.text("kind")?;
        Ok(Self {
            subject_id: row.int("subject_id")?,
            kind: PartyKind::parse(&raw).ok_or(RowError::Column {
                column: "kind".to_owned(),
                wanted: "PartyKind",
            })?,
            display_name: row.text("display_name")?,
            handle: row.text_opt("handle")?,
            status: row.text("status")?,
            at: row.int("at")?,
            granted_by: row.int_opt("granted_by")?,
            granted_display: row.text_opt("granted_display")?,
        })
    }
}

/// `/api/q/platform-operators`.
pub(super) async fn list(reads: &impl Reads, _params: &Params) -> Outcome<Vec<Row>> {
    let key = reads.ids();
    Ok(reads
        .query::<OperatorRow>(OPERATORS, bind![])
        .await?
        .into_iter()
        .map(|row| {
            let public = public_of(row.kind, row.subject_id, key);
            Row {
                key: public.as_str().to_owned(),
                value: json!({
                    "public_id": public,
                    "display": row.display_name,
                    "handle": row.handle,
                    "status": row.status,
                    "at": row.at,
                    // Absent means the bootstrap: nobody had authority to
                    // delegate yet, so the configuration is what granted it.
                    "granted_by": row.granted_by.is_some(),
                    "granted_display": row.granted_display,
                }),
            }
        })
        .collect())
}
