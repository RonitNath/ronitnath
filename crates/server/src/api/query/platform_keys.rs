//! The signing keys and the relying parties registered against them.
//!
//! Leg O1 landed the model and no surface over any of it. Two lists close
//! that, and each exists to make one decision safe rather than to display a
//! table.
//!
//! **Keys.** A rotation gives a key three lives — `active`, `retiring`,
//! `retired` — and nothing ever wrote the third, so the JWKS grew by one key
//! per rotation forever. `RetireKey` is the decision that ends one, and the
//! number that makes it safe is `signed_tokens_alive`: how many unexpired,
//! unrevoked tokens were issued while that key was the one signing. Retiring
//! it is what makes those tokens unverifiable, so the count is not decoration
//! — and it is a count of rows the deployment holds, never an estimate. It is
//! the *same statement* the command itself refuses on
//! (`rn_kernel::cmd::RETIRE_ALIVE_SQL`), so the screen cannot say a retire is
//! safe and the command then decline.
//!
//! **Clients.** Registration, rotation and deletion all take something with
//! them, and the two numbers that say what are the consent count and the live
//! token count. They are on the row rather than fetched when a confirm opens,
//! because a confirm that has to ask before it can name what it will destroy
//! is a confirm somebody clicks through while it is still loading.

use rn_kernel::bind;
use rn_kernel::cmd::RETIRE_ALIVE_SQL;
use rn_kernel::ids::{Id, IdKey, OidcClient, Organization};
use rn_kernel::store::{Count, Cursor, FromRow, Reads, RowError};
use rn_kernel::{Outcome, Timestamp};
use serde_json::json;

use super::platform::page;
use super::{Params, Row};

/// A day, for rendering ages the operator reads in days rather than seconds.
const DAY: Timestamp = 86_400;

/// Every key ever minted, newest first. Three rows on a deployment that has
/// rotated twice; the `LIMIT` is the platform page bound and not a page the
/// reader chose.
pub(super) const KEYS: &str =
    "SELECT id, kid, status, created_at, retired_at FROM oidc_key ORDER BY id DESC LIMIT $1";

/// Every registration, newest first, with the two numbers a decision about it
/// needs. Three correlated seeks per row — the consent table's primary key,
/// and `oidc_token_client_idx` twice — rather than three more round trips.
pub(super) const CLIENTS: &str = "SELECT c.id, c.owner_party_id, c.client_name, c.client_uri, \
       c.redirect_uris, c.token_endpoint_auth_method, c.scopes, c.trusted, c.members_only, \
       c.created_at, c.rotated_at, c.deleted_at, o.display_name AS owner_display, \
       (SELECT count(*) FROM oidc_consent k WHERE k.client_id = c.id) AS consents, \
       (SELECT max(t.created_at) FROM oidc_token t WHERE t.client_id = c.id) AS last_issued, \
       (SELECT count(*) FROM oidc_token t WHERE t.client_id = c.id \
          AND t.expires_at > $1 AND t.revoked_at IS NULL) AS live_tokens \
     FROM oidc_client c LEFT JOIN party o ON o.id = c.owner_party_id \
     ORDER BY c.id DESC LIMIT $2";

struct KeyRow {
    id: i64,
    kid: String,
    status: String,
    created_at: Timestamp,
    retired_at: Option<Timestamp>,
}

impl FromRow for KeyRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.int("id")?,
            kid: row.text("kid")?,
            status: row.text("status")?,
            created_at: row.int("created_at")?,
            retired_at: row.int_opt("retired_at")?,
        })
    }
}

/// An age in whole days, which is the unit a key is read in.
///
/// Floored rather than rounded: a key minted this morning is nought days old,
/// and saying "1" about it would be the screen rounding up a fact.
#[must_use]
pub fn age_days(created_at: Timestamp, now: Timestamp) -> i64 {
    ((now - created_at).max(0)) / DAY
}

/// `/api/q/platform-keys`.
pub(super) async fn keys(reads: &impl Reads, params: &Params) -> Outcome<Vec<Row>> {
    let now = reads.now();
    let rows = reads.query::<KeyRow>(KEYS, bind![page(params)]).await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        // One indexed range read per key, and a deployment has a handful of
        // keys. The alternative — one statement with a window over every
        // token ever issued — would be a pass over the whole table to answer
        // a question about three rows.
        let alive = reads
            .query_opt::<Count>(RETIRE_ALIVE_SQL, bind![row.id, now])
            .await?
            .map_or(0, |count| count.0);
        out.push(Row {
            key: row.kid.clone(),
            value: json!({
                "kid": row.kid,
                "status": row.status,
                "created_at": row.created_at,
                "age_days": age_days(row.created_at, now),
                "retired_at": row.retired_at,
                // What retiring this key would make unverifiable.
                "signed_tokens_alive": alive,
            }),
        });
    }
    Ok(out)
}

struct ClientRow {
    id: Id<OidcClient>,
    owner_party_id: Option<i64>,
    owner_display: Option<String>,
    client_name: String,
    client_uri: Option<String>,
    redirect_uris: String,
    method: String,
    scopes: String,
    trusted: bool,
    members_only: bool,
    created_at: Timestamp,
    rotated_at: Option<Timestamp>,
    deleted_at: Option<Timestamp>,
    consents: i64,
    last_issued: Option<Timestamp>,
    live_tokens: i64,
}

impl FromRow for ClientRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            owner_party_id: row.int_opt("owner_party_id")?,
            owner_display: row.text_opt("owner_display")?,
            client_name: row.text("client_name")?,
            client_uri: row.text_opt("client_uri")?,
            redirect_uris: row.text("redirect_uris")?,
            method: row.text("token_endpoint_auth_method")?,
            scopes: row.text("scopes")?,
            trusted: row.int("trusted")? != 0,
            members_only: row.int("members_only")? != 0,
            created_at: row.int("created_at")?,
            rotated_at: row.int_opt("rotated_at")?,
            deleted_at: row.int_opt("deleted_at")?,
            consents: row.int("consents")?,
            last_issued: row.int_opt("last_issued")?,
            live_tokens: row.int("live_tokens")?,
        })
    }
}

/// A JSON array column as a list. An unreadable one is empty rather than an
/// error: a page that will not render because one registration has a malformed
/// column is worse than one that shows nothing for it.
fn words(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

/// `/api/q/platform-clients`.
pub(super) async fn clients(reads: &impl Reads, params: &Params) -> Outcome<Vec<Row>> {
    let key: &IdKey = reads.ids();
    let now = reads.now();
    Ok(reads
        .query::<ClientRow>(CLIENTS, bind![now, page(params)])
        .await?
        .into_iter()
        .map(|row| Row {
            key: row.id.public(key).as_str().to_owned(),
            value: json!({
                // The public id *is* the `client_id`, so an operator reading
                // this row is reading what they hand a relying party.
                "public_id": row.id.public(key),
                "client_name": row.client_name,
                "client_uri": row.client_uri,
                // Absent means the deployment's own — `platform:*`, which has
                // no party row to name.
                "owner": row.owner_party_id.map(|owner| Id::<Organization>::new(owner).public(key)),
                "owner_display": row.owner_display,
                "redirect_uris": words(&row.redirect_uris),
                "token_endpoint_auth_method": row.method,
                "scopes": words(&row.scopes),
                "trusted": row.trusted,
                "members_only": row.members_only,
                "created_at": row.created_at,
                "rotated_at": row.rotated_at,
                "withdrawn_at": row.deleted_at,
                "consents": row.consents,
                // What a delete would take with it, and what a rotation would
                // not: the two numbers a confirm has to name.
                "live_tokens": row.live_tokens,
                "last_issued_at": row.last_issued,
            }),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_age_is_whole_days_and_is_never_rounded_up() {
        let now = 1_800_000_000;
        assert_eq!(age_days(now, now), 0);
        assert_eq!(age_days(now - DAY + 1, now), 0);
        assert_eq!(age_days(now - DAY, now), 1);
        assert_eq!(age_days(now - 6 * DAY - 3, now), 6);
        // A clock that went backwards is nought days, not a negative age.
        assert_eq!(age_days(now + DAY, now), 0);
    }

    #[test]
    fn a_malformed_json_column_renders_as_nothing_rather_than_failing_the_page() {
        assert_eq!(words(r#"["https://a.test/cb"]"#), vec!["https://a.test/cb"]);
        assert!(words("not json").is_empty());
    }
}
