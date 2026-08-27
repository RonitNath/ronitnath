//! Documents: the first registered resource kind.
//!
//! A document is a `resource` row — kind, owner, zone, status — plus a
//! `document` row holding the words. It is here to be the worked example of
//! "products plug in": nothing in `relation`, `resource` or the sharing
//! commands knows what a document *is*, only that it is a kind that admits
//! `viewer < commenter < editor` and accepts persons, organizations, groups
//! and links as subjects.
//!
//! ## The version tag
//!
//! `draft_rev` starts at 1 and increases by one per edit. An edit carries the
//! revision the client read, and the update guards on it, so two people
//! editing the same draft produce one write and one decline rather than one
//! write that silently ate the other. `published_rev` is NULL until
//! `PublishDocument`, and then it is the `draft_rev` that was published — so
//! "is there an unpublished change" is `draft_rev <> published_rev` and needs
//! no flag anybody has to remember to set.

use crate::bind;
use crate::error::Outcome;
use crate::ids::{Id, Resource};
use crate::store::{Cursor, FromRow, Reads, RowError};

/// Longest a title may be.
pub const TITLE_LIMIT: usize = 200;

/// Longest a body may be. A document is words, not an upload; something
/// larger than this is a file, and files are a different resource kind.
pub const BODY_LIMIT: usize = 100_000;

/// The revision a fresh document is at.
pub const FIRST_REV: i64 = 1;

/// A `document` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentRow {
    /// The resource it hangs off.
    pub resource_id: Id<Resource>,
    /// Its title.
    pub title: String,
    /// Its body.
    pub body: String,
    /// The revision of the draft.
    pub draft_rev: i64,
    /// The revision that is published, if any.
    pub published_rev: Option<i64>,
}

impl DocumentRow {
    /// Whether the draft has changes the published version does not have.
    pub fn has_unpublished_changes(&self) -> bool {
        self.published_rev != Some(self.draft_rev)
    }
}

impl FromRow for DocumentRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            resource_id: row.id("resource_id")?,
            title: row.text("title")?,
            body: row.text("body")?,
            draft_rev: row.int("draft_rev")?,
            published_rev: row.int_opt("published_rev")?,
        })
    }
}

/// One document by its resource id.
pub const LOAD_SQL: &str = "SELECT resource_id, title, body, draft_rev, published_rev \
                            FROM document WHERE resource_id = $1";

/// Create the document half of a new document.
pub const INSERT_SQL: &str = "INSERT INTO document (resource_id, title, body, draft_rev, \
                              published_rev) VALUES ($1, $2, $3, $4, NULL)";

/// Edit a draft. `COALESCE` is what lets a title change arrive without the
/// body; the guard on `draft_rev` is what makes a lost update a decline.
pub const EDIT_SQL: &str = "UPDATE document \
     SET title = COALESCE($1, title), body = COALESCE($2, body), draft_rev = draft_rev + 1 \
     WHERE resource_id = $3 AND draft_rev = $4";

/// Publish the draft that is there now.
pub const PUBLISH_SQL: &str =
    "UPDATE document SET published_rev = draft_rev WHERE resource_id = $1 AND draft_rev = $2";

/// Move the resource row's status along with it.
pub const PUBLISH_RESOURCE_SQL: &str =
    "UPDATE resource SET status = 'published' WHERE id = $1 AND status = 'draft'";

/// Read one document.
pub async fn load(store: &impl Reads, id: Id<Resource>) -> Outcome<Option<DocumentRow>> {
    Ok(store.query_opt::<DocumentRow>(LOAD_SQL, bind![id]).await?)
}
