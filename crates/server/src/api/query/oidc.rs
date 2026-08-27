//! The three lists the OpenID Provider adds: the client registry, the tokens
//! one session is holding, and the clients a person has authorised.
//!
//! Two of them are a platform operator's and one is the reader's own, and the
//! split is the same one every other query here makes. What is *not* in any of
//! them is a secret: a token row is named by its rowid and described by its
//! client, its scopes and its expiry, and the digest that would let somebody
//! use it is never selected.

use rn_kernel::bind;
use rn_kernel::ids::{Id, IdKey, OidcClient, OidcToken, Person, Session};
use rn_kernel::store::{Cursor, FromRow, Reads, RowError};
use rn_kernel::{Outcome, Timestamp};
use serde_json::json;

use super::platform::{page, subject};
use super::{Params, Row};

/// Every registered client, newest first. Ordered by rowid, which is creation
/// order and the primary key, so the order this wants is the order the table
/// is already in.
const CLIENTS: &str = "SELECT c.id, c.owner_party_id, c.client_name, c.client_uri, \
     c.token_endpoint_auth_method, c.grant_types, c.scopes, c.trusted, c.members_only, \
     c.backchannel_logout_uri, c.created_at, c.rotated_at, c.deleted_at, \
     o.display_name AS owner_display \
     FROM oidc_client c LEFT JOIN party o ON o.id = c.owner_party_id \
     ORDER BY c.id DESC LIMIT $1";

struct ClientRow {
    id: Id<OidcClient>,
    owner_party_id: Option<Id<Person>>,
    owner_display: Option<String>,
    client_name: String,
    client_uri: Option<String>,
    method: String,
    grant_types: String,
    scopes: String,
    trusted: bool,
    members_only: bool,
    has_backchannel: bool,
    created_at: Timestamp,
    rotated_at: Option<Timestamp>,
    deleted_at: Option<Timestamp>,
}

impl FromRow for ClientRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            owner_party_id: row.id_opt("owner_party_id")?,
            owner_display: row.text_opt("owner_display")?,
            client_name: row.text("client_name")?,
            client_uri: row.text_opt("client_uri")?,
            method: row.text("token_endpoint_auth_method")?,
            grant_types: row.text("grant_types")?,
            scopes: row.text("scopes")?,
            trusted: row.int("trusted")? != 0,
            members_only: row.int("members_only")? != 0,
            has_backchannel: row.text_opt("backchannel_logout_uri")?.is_some(),
            created_at: row.int("created_at")?,
            rotated_at: row.int_opt("rotated_at")?,
            deleted_at: row.int_opt("deleted_at")?,
        })
    }
}

/// `/api/q/oidc-clients` — the registry, for a platform operator.
pub(super) async fn clients(reads: &impl Reads, params: &Params) -> Outcome<Vec<Row>> {
    let key: &IdKey = reads.ids();
    Ok(reads
        .query::<ClientRow>(CLIENTS, bind![page(params)])
        .await?
        .into_iter()
        .map(|row| Row {
            key: row.id.public(key).as_str().to_owned(),
            value: json!({
                // The public id *is* the `client_id`, so an operator reading
                // this page is reading the thing they hand a relying party.
                "public_id": row.id.public(key),
                "client_name": row.client_name,
                "client_uri": row.client_uri,
                // Absent means the deployment's own — `platform:*`, which has
                // no party row to name.
                "owner": row.owner_party_id.map(|owner| {
                    Id::<rn_kernel::ids::Organization>::new(owner.get()).public(key)
                }),
                "owner_display": row.owner_display,
                "token_endpoint_auth_method": row.method,
                "grant_types": words(&row.grant_types),
                "scopes": words(&row.scopes),
                "trusted": row.trusted,
                "members_only": row.members_only,
                "backchannel_logout": row.has_backchannel,
                "created_at": row.created_at,
                "rotated_at": row.rotated_at,
                "withdrawn_at": row.deleted_at,
            }),
        })
        .collect())
}

/// A JSON array column, as a list. An unreadable one is empty rather than an
/// error: this is a page, and a page that will not render because one
/// registration has a malformed column is worse than one that shows nothing
/// for it.
fn words(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

/// The tokens under one session, newest first. `oidc_token_session_idx` is
/// what makes it a seek; `explain_the_clients_a_session_has_to_be_told_about`
/// is the neighbouring assertion.
const TOKENS: &str = "SELECT t.id, t.kind, t.client_id, t.scopes, t.created_at, t.expires_at, \
     t.revoked_at, c.client_name \
     FROM oidc_token t JOIN oidc_client c ON c.id = t.client_id \
     WHERE t.session_id = $1 ORDER BY t.id DESC LIMIT $2";

struct TokenRow {
    id: Id<OidcToken>,
    kind: String,
    client_id: Id<OidcClient>,
    client_name: String,
    scopes: String,
    created_at: Timestamp,
    expires_at: Timestamp,
    revoked_at: Option<Timestamp>,
}

impl FromRow for TokenRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            kind: row.text("kind")?,
            client_id: row.id("client_id")?,
            client_name: row.text("client_name")?,
            scopes: row.text("scopes")?,
            created_at: row.int("created_at")?,
            expires_at: row.int("expires_at")?,
            revoked_at: row.int_opt("revoked_at")?,
        })
    }
}

/// `/api/q/oidc-tokens?subject=<session>` — what one session is holding.
///
/// A drill-in, so a request with no `subject` reads nothing rather than
/// reading every token on the deployment: the parameter is the whole question,
/// and a missing one is not a wildcard.
pub(super) async fn tokens(reads: &impl Reads, params: &Params) -> Outcome<Vec<Row>> {
    let key: &IdKey = reads.ids();
    let Some(session) = subject::<Session>(key, params) else {
        return Ok(Vec::new());
    };
    Ok(reads
        .query::<TokenRow>(TOKENS, bind![session, page(params)])
        .await?
        .into_iter()
        .map(|row| Row {
            // The token's own rowid has no public form — the secret is the
            // only handle it has, and the row keeps a digest of that. The key
            // is therefore its position under the session, which is stable.
            key: format!("{:020}", row.id.get()),
            value: json!({
                "kind": row.kind,
                "client": row.client_id.public(key),
                "client_name": row.client_name,
                "scopes": row.scopes,
                "created_at": row.created_at,
                "expires_at": row.expires_at,
                "revoked_at": row.revoked_at,
            }),
        })
        .collect())
}

struct Authorization {
    client_id: Id<OidcClient>,
    client_name: String,
    client_uri: Option<String>,
    scopes: String,
    at: Timestamp,
}

impl FromRow for Authorization {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            client_id: row.id("client_id")?,
            client_name: row.text("client_name")?,
            client_uri: row.text_opt("client_uri")?,
            scopes: row.text("scopes")?,
            at: row.int("at")?,
        })
    }
}

/// `/api/q/authorizations` — the clients this person has said yes to.
///
/// The reader's own, always: the statement is keyed on the acting person and
/// takes no parameter that could name anybody else's.
pub(super) async fn authorizations(
    reads: &impl Reads,
    person: Id<Person>,
    key: &IdKey,
) -> Outcome<Vec<Row>> {
    Ok(reads
        .query::<Authorization>(rn_kernel::oidc::consent::BY_PERSON_SQL, bind![person])
        .await?
        .into_iter()
        .map(|row| Row {
            key: row.client_id.public(key).as_str().to_owned(),
            value: json!({
                "client": row.client_id.public(key),
                "client_name": row.client_name,
                "client_uri": row.client_uri,
                "scopes": row.scopes,
                "at": row.at,
            }),
        })
        .collect())
}
