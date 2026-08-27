//! Every live session on the deployment.
//!
//! A session row *is* the session — there is no status, because ending one is
//! deleting it — so this list is exactly what is live right now, and its
//! length is the honest answer to "how many people are signed in". The bundle
//! folds that number into the page's title rather than captioning it.
//!
//! A session binds an *identity*, never a person, which is why the identity is
//! the column beside it and `acting_as` is a separate one: the same
//! registration can be speaking as itself or as an organization it belongs to.

use rn_kernel::bind;
use rn_kernel::ids::{Id, IdKey, Identity, Session};
use rn_kernel::store::{Cursor, FromRow, Reads, RowError};
use rn_kernel::{Outcome, Timestamp};
use serde_json::json;

use super::platform::page;
use super::platform_parties::object_id;
use super::{Params, Row};

/// Newest first, by rowid rather than by `created_at`: ids are handed out in
/// creation order, and the rowid is the primary key — so the order this list
/// wants is one the table is already in, and there is no sort over every
/// session on the deployment. `explain_the_platform_lists` asserts it.
pub(super) const SESSIONS: &str = "SELECT s.id, s.identity_id, s.acting_as, s.created_at, s.last_seen_at, \
                               s.expires_at, a.kind AS acting_kind, \
                               a.display_name AS acting_display, \
                               p.display_name AS person_display \
                        FROM session s \
                        JOIN party a ON a.id = s.acting_as \
                        JOIN identity i ON i.id = s.identity_id \
                        LEFT JOIN party p ON p.id = i.person_id \
                        ORDER BY s.id DESC LIMIT $1";

struct Live {
    id: Id<Session>,
    identity_id: Id<Identity>,
    acting_as: i64,
    acting_kind: String,
    acting_display: String,
    person_display: Option<String>,
    created_at: Timestamp,
    last_seen_at: Timestamp,
    expires_at: Timestamp,
}

impl FromRow for Live {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            identity_id: row.id("identity_id")?,
            acting_as: row.int("acting_as")?,
            acting_kind: row.text("acting_kind")?,
            acting_display: row.text("acting_display")?,
            person_display: row.text_opt("person_display")?,
            created_at: row.int("created_at")?,
            last_seen_at: row.int("last_seen_at")?,
            expires_at: row.int("expires_at")?,
        })
    }
}

pub(super) async fn list(reads: &impl Reads, params: &Params) -> Outcome<Vec<Row>> {
    let key: &IdKey = reads.ids();
    Ok(reads
        .query::<Live>(SESSIONS, bind![page(params)])
        .await?
        .into_iter()
        .map(|row| Row {
            key: row.id.public(key).as_str().to_owned(),
            value: json!({
                "public_id": row.id.public(key),
                "identity": row.identity_id.public(key),
                // A resolved registration is named by its person; one that has
                // not resolved is named by nothing, because the only other
                // name it has is an address.
                "person": row.person_display,
                "acting_as": object_id(&row.acting_kind, row.acting_as, key),
                "acting_kind": row.acting_kind,
                "acting_display": row.acting_display,
                "created_at": row.created_at,
                "last_seen_at": row.last_seen_at,
                "expires_at": row.expires_at,
            }),
        })
        .collect())
}
