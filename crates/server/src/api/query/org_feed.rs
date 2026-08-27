//! The organization's documents, its invitation links, and its audit tail.
//!
//! These three are the ones whose result set is not a single indexed range, so
//! each says how it is bounded instead:
//!
//! * **documents** ride `resource_owner_idx` from the owning party, and their
//!   grants are read in one further statement over `relation_object_idx` for
//!   the page's ids rather than one statement per document.
//! * **invitations** ride `relation_object_idx` twice — once for the
//!   organization's own party and once for its groups — because
//!   `(object_kind, object_id)` is a two-column index and a container list
//!   that mixed the kinds could seek on neither.
//! * **audit** is a forward range over `audit.id`, the change-feed offset
//!   itself, filtered to the rows this organization's containers appear on.
//!   Paging forward from an offset is what keeps it a rowid seek.

use rn_kernel::bind;
use rn_kernel::domain::{PartyKind, Vocabulary as _};
use rn_kernel::error::Outcome;
use rn_kernel::ids::{Id, IdKey, Link, Resource};
use rn_kernel::store::{Cursor, FromRow, Reads, RowError, Value};
use serde_json::json;

use super::org::{self, Scope};
use super::{Params, Row};

/// This organization's live documents, newest first.
///
/// `INDEXED BY` is not decoration. Left alone the planner takes
/// `resource_kind_created_idx`, because that index is already in the order the
/// `ORDER BY` wants — and pays for it by walking every document in the
/// deployment and discarding the ones this organization does not own. Pinned
/// to the owner index it seeks straight to this tenant's rows and sorts them,
/// which is a sort over one organization's documents rather than a walk over
/// everybody's.
pub const DOCUMENTS: &str = "SELECT r.id, r.status, r.created_at, \
     d.title, d.body, d.draft_rev, d.published_rev \
     FROM resource r INDEXED BY resource_owner_idx \
     JOIN document d ON d.resource_id = r.id \
     WHERE r.owner_party_id = $1 AND r.kind = 'document' AND r.status <> 'deleted' \
     ORDER BY r.created_at DESC, r.id DESC LIMIT $2";

/// Every grant on a page of documents, in one statement.
pub const GRANTS: &str = "SELECT rel.object_id AS document_id, rel.relation, \
     rel.subject_kind, rel.subject_id, p.kind AS party_kind, p.display_name \
     FROM json_each($1) d \
     CROSS JOIN relation rel ON rel.object_kind = 'document' AND rel.object_id = d.value \
     LEFT JOIN party p ON p.id = rel.subject_id \
     ORDER BY rel.object_id, rel.id";

/// The invitation links minted into one kind of container.
pub const INVITATIONS: &str = "SELECT l.id, l.expires_at, l.claimed_at, \
     l.created_at, rel.object_id AS container_id, rel.relation AS role, rel.granted_by, \
     c.kind AS container_kind, c.display_name AS container_display, \
     minter.display_name AS minted_by, claimant.display_name AS claimed_by \
     FROM json_each($1) g \
     CROSS JOIN relation rel ON rel.object_kind = $2 AND rel.object_id = g.value \
     JOIN link l ON l.id = rel.subject_id \
     JOIN party c ON c.id = rel.object_id \
     LEFT JOIN identity mi ON mi.id = rel.granted_by \
     LEFT JOIN party minter ON minter.id = mi.person_id \
     LEFT JOIN identity ci ON ci.id = l.claimed_by_identity_id \
     LEFT JOIN party claimant ON claimant.id = ci.person_id \
     WHERE rel.subject_kind = 'link' \
     ORDER BY l.id DESC";

/// One page of the organization's audit tail.
///
/// Four ways a row belongs to an organization, and they are the four field
/// names the commands' own `json_object(…)` payloads use: the principal was
/// acting as it, the command named one of its containers, it named the
/// organization itself, or it created something the organization owns. A row
/// that names none of them is somebody else's and is not in the range.
///
/// `a.id > $1` is the whole of the pagination and the whole of the plan: the
/// rowid is the change-feed offset, so a page is a range seek from where the
/// reader got to. The container test is a filter within that range, never a
/// way of finding it.
pub const AUDIT: &str = "SELECT a.id, a.command, a.at, actor.display_name AS actor_display \
     FROM audit a \
     LEFT JOIN identity i ON i.id = a.actor_identity_id \
     LEFT JOIN party actor ON actor.id = i.person_id \
     WHERE a.id > $1 AND ($2 = '' OR a.command = $2) \
       AND (a.acting_as IN (SELECT value FROM json_each($3)) \
            OR json_extract(a.payload, '$.container') IN (SELECT value FROM json_each($3)) \
            OR json_extract(a.payload, '$.organization') IN (SELECT value FROM json_each($3)) \
            OR json_extract(a.payload, '$.owner') IN (SELECT value FROM json_each($3))) \
     ORDER BY a.id LIMIT $4";

/// The most documents one organization page carries.
const DOCUMENT_PAGE: i64 = 200;

/// The organization's documents, each with the grants made on it.
pub async fn documents(reads: &impl Reads, scope: Scope) -> Outcome<Vec<Row>> {
    let key = reads.ids();
    let rows = reads
        .query::<DocumentRow>(DOCUMENTS, bind![scope.party, DOCUMENT_PAGE])
        .await?;
    let ids: Vec<i64> = rows.iter().map(|row| row.id.get()).collect();
    let grants = reads.query::<GrantRow>(GRANTS, bind![list(&ids)]).await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let public_id = row.id.public(key);
            let shared: Vec<_> = grants
                .iter()
                .filter(|grant| grant.document == row.id.get())
                .map(|grant| grant.render(key))
                .collect();
            Row {
                key: public_id.as_str().to_owned(),
                value: json!({
                    "public_id": public_id,
                    "title": row.title,
                    "body": row.body,
                    "status": row.status,
                    "created_at": row.created_at,
                    "draft_rev": row.draft_rev,
                    "published_rev": row.published_rev,
                    "unpublished": row.published_rev != Some(row.draft_rev),
                    "shared": shared,
                }),
            }
        })
        .collect())
}

/// Every link minted into this organization or one of its groups.
pub async fn invitations(reads: &impl Reads, scope: Scope, params: &Params) -> Outcome<Vec<Row>> {
    let key = reads.ids();
    let now = reads.now();
    let mut rows = reads
        .query::<LinkRow>(
            INVITATIONS,
            bind![list(&[scope.party.get()]), "organization"],
        )
        .await?;
    let groups: Vec<i64> = org::containers(reads, scope)
        .await?
        .into_iter()
        .filter(|id| *id != scope.party.get())
        .collect();
    if !groups.is_empty() {
        rows.extend(
            reads
                .query::<LinkRow>(INVITATIONS, bind![list(&groups), "group"])
                .await?,
        );
    }
    // A drill-in narrows to one group; the page itself lists everything.
    let only = match params.group.as_ref() {
        Some(named) => Some(org::group_of(reads, scope, named).await?.get()),
        None => None,
    };
    rows.sort_by_key(|row| std::cmp::Reverse(row.id));
    Ok(rows
        .into_iter()
        .filter(|row| only.is_none_or(|id| row.container == id))
        .map(|row| row.render(key, now))
        .collect())
}

/// One page of the organization's audit tail, forward from `after`.
pub async fn audit(reads: &impl Reads, scope: Scope, params: &Params) -> Outcome<Vec<Row>> {
    let containers = list(&org::containers(reads, scope).await?);
    let after = i64::try_from(params.after).unwrap_or(i64::MAX);
    let command = params.command.clone().unwrap_or_default();
    Ok(reads
        .query::<AuditRow>(AUDIT, bind![after, command, containers, params.page()])
        .await?
        .into_iter()
        .map(|row| Row {
            // Zero-padded, so the client's key-ordered map reads
            // chronologically instead of lexicographically wrong at every
            // power of ten.
            key: format!("{:020}", row.offset.max(0)),
            value: json!({
                "offset": row.offset,
                "command": row.command,
                "at": row.at,
                "actor": row.actor,
            }),
        })
        .collect())
}

/// A list of row ids as the JSON array `json_each` takes.
fn list(ids: &[i64]) -> Value {
    Value::Text(serde_json::to_string(ids).unwrap_or_else(|_| "[]".to_owned()))
}

struct DocumentRow {
    id: Id<Resource>,
    status: String,
    created_at: i64,
    title: String,
    body: String,
    draft_rev: i64,
    published_rev: Option<i64>,
}

impl FromRow for DocumentRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            status: row.text("status")?,
            created_at: row.int("created_at")?,
            title: row.text("title")?,
            body: row.text("body")?,
            draft_rev: row.int("draft_rev")?,
            published_rev: row.int_opt("published_rev")?,
        })
    }
}

struct GrantRow {
    document: i64,
    relation: String,
    subject_kind: String,
    subject: i64,
    party_kind: Option<String>,
    display: Option<String>,
}

impl FromRow for GrantRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            document: row.int("document_id")?,
            relation: row.text("relation")?,
            subject_kind: row.text("subject_kind")?,
            subject: row.int("subject_id")?,
            party_kind: row.text_opt("party_kind")?,
            display: row.text_opt("display_name")?,
        })
    }
}

impl GrantRow {
    /// A grant as the sharing panel renders it.
    ///
    /// A subject with no party row — `public`, `authenticated`, a link — has no
    /// public id to hand back, so the row carries its kind and nothing that
    /// could be posted back as an id.
    fn render(&self, key: &IdKey) -> serde_json::Value {
        let public_id = self
            .party_kind
            .as_deref()
            .and_then(PartyKind::parse)
            .map(|kind| crate::api::party::public_of(kind, self.subject, key));
        json!({
            "relation": self.relation,
            "subject_kind": self.subject_kind,
            "subject": public_id,
            "display": self.display,
        })
    }
}

struct LinkRow {
    id: i64,
    expires_at: i64,
    claimed_at: Option<i64>,
    created_at: i64,
    container: i64,
    container_kind: String,
    container_display: String,
    role: String,
    minted_by: Option<String>,
    claimed_by: Option<String>,
}

impl FromRow for LinkRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.int("id")?,
            expires_at: row.int("expires_at")?,
            claimed_at: row.int_opt("claimed_at")?,
            created_at: row.int("created_at")?,
            container: row.int("container_id")?,
            role: row.text("role")?,
            container_kind: row.text("container_kind")?,
            container_display: row.text("container_display")?,
            minted_by: row.text_opt("minted_by")?,
            claimed_by: row.text_opt("claimed_by")?,
        })
    }
}

impl LinkRow {
    /// An invitation as the list renders it.
    ///
    /// The token is not here and cannot be: the row keeps only its SHA-256, so
    /// the claim URL exists exactly once, in the reply to the `Invite` that
    /// minted it. A list of past invitations is a list of their *fates* — plus
    /// the link's own public id, which names the row without naming the
    /// secret and is what `RevokeLink` takes.
    fn render(&self, key: &IdKey, now: i64) -> Row {
        let container = PartyKind::parse(&self.container_kind)
            .map(|kind| crate::api::party::public_of(kind, self.container, key));
        // Keyed by the rowid, zero-padded, so a key-ordered client map reads
        // newest-last rather than lexicographically wrong at every power of
        // ten. The id the reader *acts* on is the public one beside it.
        let key_text = format!("{:020}", self.id.max(0));
        let link = Id::<Link>::new(self.id).public(key);
        Row {
            key: key_text.clone(),
            value: json!({
                "key": key_text,
                "id": link,
                "container": container,
                "container_display": self.container_display,
                "role": self.role,
                "expires_at": self.expires_at,
                "created_at": self.created_at,
                "claimed_at": self.claimed_at,
                "claimed_by": self.claimed_by,
                "minted_by": self.minted_by,
                "live": self.claimed_at.is_none() && self.expires_at > now,
            }),
        }
    }
}

struct AuditRow {
    offset: i64,
    command: String,
    at: i64,
    actor: Option<String>,
}

impl FromRow for AuditRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            offset: row.int("id")?,
            command: row.text("command")?,
            at: row.int("at")?,
            actor: row.text_opt("actor_display")?,
        })
    }
}
