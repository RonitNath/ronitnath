//! Outstanding invitations, granted consents, and what a disable would end.
//!
//! Three reads about the same thing from three angles: the standing grants
//! nobody has to sign in to use.
//!
//! A **link** is a bearer secret with an expiry, and its grant is a relation
//! row — `group:X #member @link:T` — which is why the role is read off
//! `relation` and not off a column here. What the list never carries is the
//! token: an operator withdrawing somebody's invitation has no business
//! holding the secret that would let them claim it, so the row is named by its
//! public id and described by everything except the one field that would let
//! it be used.
//!
//! A **consent** is also a relation — `oidc_client:X #authorized @person:Y` —
//! with its scopes and its timestamp in `oidc_consent`.
//!
//! And [`cascade`] is the third: the counts a disable would end, read *before*
//! the command so the confirm can name them. It answers at the time of asking,
//! which is why the audit row the command writes is the record and this is
//! only the preview.

use rn_kernel::bind;
use rn_kernel::ids::{Id, IdKey, Link, OidcClient, Person};
use rn_kernel::store::{Count, Cursor, FromRow, Reads, RowError};
use rn_kernel::{Outcome, Timestamp};
use serde_json::json;

use super::platform::{page, subject};
use super::platform_parties::object_id;
use super::{Params, Row};

/// Every link that carries a grant, newest first.
///
/// `link` leads, walked backwards along its own primary key, and the grant is
/// reached by a correlated seek on `relation_subject_idx`. Written that way
/// round on purpose: the obvious join — `relation` first, on the constant
/// `subject_kind = 'link'` — is the plan SQLite picks by itself, and it makes
/// the newest-first order a sort over every invitation the deployment has ever
/// minted. Leading on the table the order belongs to costs nothing and sorts
/// nothing.
///
/// `verifies_factor_id IS NULL` leaves out the email-verification links, which
/// carry no grant and are not invitations.
pub(super) const LINKS: &str = "SELECT l.id, l.expires_at, l.claimed_at, l.suspended_at, \
       l.created_at, r.object_kind, r.object_id, r.relation, \
       container.display_name AS container_display, \
       minter.display_name AS minted_display, \
       claimer.display_name AS claimed_display \
     FROM link l \
     JOIN relation r ON r.id = (SELECT id FROM relation \
         WHERE subject_kind = 'link' AND subject_id = l.id LIMIT 1) \
     LEFT JOIN party container ON container.id = r.object_id \
     LEFT JOIN identity minted_by ON minted_by.id = r.granted_by \
     LEFT JOIN party minter ON minter.id = minted_by.person_id \
     LEFT JOIN identity claimed_by ON claimed_by.id = l.claimed_by_identity_id \
     LEFT JOIN party claimer ON claimer.id = claimed_by.person_id \
     WHERE l.verifies_factor_id IS NULL \
     ORDER BY l.id DESC LIMIT $1";

/// Every consent, in the primary key's own order — which is `(client_id,
/// person_id)`, so the list groups by client with no sort.
pub(super) const CONSENTS: &str = "SELECT k.client_id, k.person_id, k.scopes, k.at, \
       c.client_name, p.display_name AS person_display, p.handle \
     FROM oidc_consent k \
     JOIN oidc_client c ON c.id = k.client_id \
     JOIN party p ON p.id = k.person_id \
     ORDER BY k.client_id, k.person_id LIMIT $1";

/// The same, for one client — the leading column of that primary key, so it is
/// a seek. This is what the bulk control reads before it runs.
pub(super) const CONSENTS_OF_CLIENT: &str = "SELECT k.client_id, k.person_id, k.scopes, k.at, \
       c.client_name, p.display_name AS person_display, p.handle \
     FROM oidc_consent k \
     JOIN oidc_client c ON c.id = k.client_id \
     JOIN party p ON p.id = k.person_id \
     WHERE k.client_id = $1 ORDER BY k.person_id LIMIT $2";

/// How many live sessions a party's registrations hold. `identity_person_idx`
/// then `session_identity_idx`: two seeks, no scan.
pub(super) const CASCADE_SESSIONS: &str = "SELECT count(*) AS n FROM session s \
     JOIN identity i ON i.id = s.identity_id WHERE i.person_id = $1";

/// How many unexpired, unrevoked tokens they hold. `oidc_token_person_client_idx`
/// leads on `person_id`, which is the seek.
pub(super) const CASCADE_TOKENS: &str = "SELECT count(*) AS n FROM oidc_token \
     WHERE person_id = $1 AND expires_at > $2 AND revoked_at IS NULL";

/// How many unclaimed links they minted that a disable would put to sleep.
pub(super) const CASCADE_LINKS: &str = "SELECT count(*) AS n FROM link l \
     JOIN relation r ON r.subject_kind = 'link' AND r.subject_id = l.id \
     JOIN identity i ON i.id = r.granted_by \
     WHERE i.person_id = $1 AND l.claimed_at IS NULL AND l.suspended_at IS NULL";

/// How many consents they hold. `oidc_consent_person_idx`, added by migration
/// 7 for exactly this read.
pub(super) const CASCADE_CONSENTS: &str =
    "SELECT count(*) AS n FROM oidc_consent WHERE person_id = $1";

struct LinkRow {
    id: Id<Link>,
    expires_at: Timestamp,
    claimed_at: Option<Timestamp>,
    suspended_at: Option<Timestamp>,
    created_at: Timestamp,
    object_kind: String,
    object_id: i64,
    relation: String,
    container_display: Option<String>,
    minted_display: Option<String>,
    claimed_display: Option<String>,
}

impl FromRow for LinkRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            expires_at: row.int("expires_at")?,
            claimed_at: row.int_opt("claimed_at")?,
            suspended_at: row.int_opt("suspended_at")?,
            created_at: row.int("created_at")?,
            object_kind: row.text("object_kind")?,
            object_id: row.int("object_id")?,
            relation: row.text("relation")?,
            container_display: row.text_opt("container_display")?,
            minted_display: row.text_opt("minted_display")?,
            claimed_display: row.text_opt("claimed_display")?,
        })
    }
}

/// What state an invitation is in, as one word.
///
/// Four, and they are four different facts: it was used, its minter is
/// disabled and it is asleep, its hour has passed, or it is waiting. Computed
/// from the columns rather than stored, because three of the four are the
/// passage of time and nothing writes a row when a clock ticks.
#[must_use]
pub fn link_state(
    claimed_at: Option<Timestamp>,
    suspended_at: Option<Timestamp>,
    expires_at: Timestamp,
    now: Timestamp,
) -> &'static str {
    if claimed_at.is_some() {
        "claimed"
    } else if suspended_at.is_some() {
        "suspended"
    } else if expires_at <= now {
        "expired"
    } else {
        "open"
    }
}

/// `/api/q/platform-links`.
pub(super) async fn links(reads: &impl Reads, params: &Params) -> Outcome<Vec<Row>> {
    let key: &IdKey = reads.ids();
    let now = reads.now();
    Ok(reads
        .query::<LinkRow>(LINKS, bind![page(params)])
        .await?
        .into_iter()
        .map(|row| Row {
            key: row.id.public(key).as_str().to_owned(),
            value: json!({
                "public_id": row.id.public(key),
                "container": object_id(&row.object_kind, row.object_id, key),
                "container_kind": row.object_kind,
                "container_display": row.container_display,
                // The role the claim would grant. It is the relation word the
                // schema uses, read as vocabulary rather than translated.
                "role": row.relation,
                "minted_by": row.minted_display,
                "created_at": row.created_at,
                "expires_at": row.expires_at,
                "claimed_at": row.claimed_at,
                "claimed_by": row.claimed_display,
                "suspended_at": row.suspended_at,
                "state": link_state(row.claimed_at, row.suspended_at, row.expires_at, now),
            }),
        })
        .collect())
}

struct ConsentRow {
    client_id: Id<OidcClient>,
    person_id: Id<Person>,
    client_name: String,
    person_display: String,
    handle: Option<String>,
    scopes: String,
    at: Timestamp,
}

impl FromRow for ConsentRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            client_id: row.id("client_id")?,
            person_id: row.id("person_id")?,
            client_name: row.text("client_name")?,
            person_display: row.text("person_display")?,
            handle: row.text_opt("handle")?,
            scopes: row.text("scopes")?,
            at: row.int("at")?,
        })
    }
}

/// `/api/q/platform-consents`, and `?subject=<client>` for one client's.
///
/// The consent's own key is the pair, because that is what its primary key is:
/// one person's consent to one client is one row, and a diff addressing it by
/// either half alone would address several.
pub(super) async fn consents(reads: &impl Reads, params: &Params) -> Outcome<Vec<Row>> {
    let key: &IdKey = reads.ids();
    let rows = match subject::<OidcClient>(key, params) {
        Some(client) => {
            reads
                .query::<ConsentRow>(CONSENTS_OF_CLIENT, bind![client, page(params)])
                .await?
        }
        // No client named is every consent, not none: unlike a drill-in, this
        // is a list whose unfiltered form is the page an operator opens.
        None => {
            reads
                .query::<ConsentRow>(CONSENTS, bind![page(params)])
                .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| Row {
            key: format!(
                "{}:{}",
                row.client_id.public(key),
                row.person_id.public(key)
            ),
            value: json!({
                "client": row.client_id.public(key),
                "client_name": row.client_name,
                "person": row.person_id.public(key),
                "person_display": row.person_display,
                "handle": row.handle,
                "scopes": row.scopes,
                "at": row.at,
            }),
        })
        .collect())
}

/// `/api/q/platform-cascade?subject=<person>` — what a disable would end.
///
/// Four counts, each a seek, read before the command so the confirm can name
/// what it is about to take. It is true *at the time of asking*: the audit row
/// the command writes carries the counts it actually ended, and where the two
/// differ the row is the truth.
pub(super) async fn cascade(reads: &impl Reads, params: &Params) -> Outcome<Vec<Row>> {
    let key: &IdKey = reads.ids();
    let Some(person) = subject::<Person>(key, params) else {
        return Ok(Vec::new());
    };
    let now = reads.now();
    let count = async |sql, args| -> Outcome<i64> {
        Ok(reads
            .query_opt::<Count>(sql, args)
            .await?
            .map_or(0, |row| row.0))
    };
    Ok(vec![Row {
        key: person.public(key).as_str().to_owned(),
        value: json!({
            "party": person.public(key),
            "sessions": count(CASCADE_SESSIONS, bind![person]).await?,
            "tokens": count(CASCADE_TOKENS, bind![person, now]).await?,
            "links": count(CASCADE_LINKS, bind![person]).await?,
            "consents": count(CASCADE_CONSENTS, bind![person]).await?,
        }),
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_links_state_is_read_off_its_columns_and_the_clock() {
        let now = 1_800_000_000;
        assert_eq!(link_state(None, None, now + 60, now), "open");
        assert_eq!(link_state(None, None, now, now), "expired");
        assert_eq!(link_state(None, Some(now), now + 60, now), "suspended");
        // Claimed outranks everything: a link that was used is used, whatever
        // happened to its minter afterwards.
        assert_eq!(link_state(Some(now), Some(now), now - 1, now), "claimed");
    }
}
