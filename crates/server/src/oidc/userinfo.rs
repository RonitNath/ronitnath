//! `GET|POST /oidc/userinfo` — the claims an access token is worth.
//!
//! This is one of exactly two routes in the deployment that accept a bearer
//! access token (the other is `/oidc/revoke`), and the only one that answers
//! with anything about a person. Three rules hold it in:
//!
//! * **A cookie is not a credential here.** The extractor reads
//!   `Authorization: Bearer` and nothing else, so a browser that happens to be
//!   signed in gets a `401` exactly like a stranger. The reverse — a bearer
//!   token under `/api/*` — is prevented by there being no code that reads one
//!   there at all.
//! * **Scopes decide the claims.** `profile`, `email` and `groups` each name a
//!   group of claims, and a token that was not granted one does not carry it.
//! * **`groups` never leaves the client's owner's subtree.** The memberships
//!   returned are those under the organization that owns the client, and no
//!   others — a client of one tenant cannot learn a person's memberships in
//!   another.

use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Json, http::Method};
use rn_api::oidc::Scope;
use rn_kernel::bind;
use rn_kernel::ids::{Id, Person};
use rn_kernel::oidc::{PREFERRED_USERNAME, client as registry, handle, subject, token as tokens};
use rn_kernel::store::{Cursor, FromRow, Reads as _, RowError};
use serde_json::{Map, Value, json};

use crate::state::AppState;

/// The route, for both methods. Core §5.3 says GET and POST both, and a POST
/// is what a client with a long token sends.
pub async fn any(State(state): State<AppState>, method: Method, headers: HeaderMap) -> Response {
    let _ = method;
    let Some(presented) = bearer(&headers) else {
        return challenge("invalid_token", "no bearer token was presented");
    };
    let reads = state.store.reads();
    let now = state.store.clock().now();

    let Ok(Some(row)) = tokens::by_secret(&reads, &presented).await else {
        return challenge(
            "invalid_token",
            "that token is not one this deployment knows",
        );
    };
    // Expired, revoked, and revoked-because-the-session-ended are one answer.
    // The third is the interesting one: a session that ended took its tokens
    // with it, so this is where "sign out" reaches a relying party even before
    // the back-channel POST does.
    if !row.is_live(now) || row.kind != "access" {
        return challenge("invalid_token", "that token is no longer good");
    }
    let Some(person) = row.person_id else {
        // A `client_credentials` token speaks for a service, and a service has
        // no claims: Core §5.3 is about an end user.
        return challenge("invalid_token", "that token has no end user");
    };
    let Ok(Some(client)) = registry::load(&reads, row.client_id).await else {
        return challenge("invalid_token", "that token's client is gone");
    };

    let scopes = Scope::parse_list(&row.scopes).unwrap_or_default();
    let Ok(Some(sub)) = subject::existing(&reads, client.sector(), person).await else {
        return challenge("invalid_token", "that token has no subject");
    };

    let mut claims = Map::new();
    claims.insert("sub".to_owned(), sub.sub.into());
    if scopes.contains(&Scope::Profile) {
        for (name, value) in profile(&state, person).await {
            claims.insert(name, value);
        }
    }
    if scopes.contains(&Scope::Email) {
        for (name, value) in email(&state, person).await {
            claims.insert(name, value);
        }
    }
    if scopes.contains(&Scope::Groups) {
        claims.insert(
            "groups".to_owned(),
            json!(groups(&state, &client, person).await),
        );
    }
    Json(Value::Object(claims)).into_response()
}

/// The bearer token, from the header the specification puts it in.
fn bearer(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = raw.strip_prefix("Bearer ")?.trim();
    (!token.is_empty()).then(|| token.to_owned())
}

/// The `401` this endpoint gives for every reason it has (RFC 6750 §3).
fn challenge(error: &'static str, description: &'static str) -> Response {
    let mut response = StatusCode::UNAUTHORIZED.into_response();
    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        HeaderValue::from_str(&format!(
            "Bearer error=\"{error}\", error_description=\"{description}\""
        ))
        .unwrap_or_else(|_| HeaderValue::from_static("Bearer error=\"invalid_token\"")),
    );
    response
}

struct Named {
    display_name: String,
}

impl FromRow for Named {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            display_name: row.text("display_name")?,
        })
    }
}

/// The `profile` claims.
async fn profile(state: &AppState, person: Id<Person>) -> Vec<(String, Value)> {
    let reads = state.store.reads();
    let mut claims = Vec::new();
    if let Ok(Some(row)) = reads
        .query_opt::<Named>(
            "SELECT display_name FROM party WHERE id = $1",
            bind![person],
        )
        .await
    {
        claims.push(("name".to_owned(), row.display_name.into()));
    }
    if let Ok(Some(chosen)) = handle::of_person(&reads, person).await {
        claims.push((PREFERRED_USERNAME.to_owned(), chosen.into()));
    }
    // `updated_at` is a number of seconds since the epoch (Core §5.1), and
    // what it is *about* is the profile: the party row's own creation is the
    // only instant this model has for it.
    if let Ok(Some(at)) = created_at(state, person).await {
        claims.push(("updated_at".to_owned(), at.into()));
    }
    claims
}

async fn created_at(state: &AppState, person: Id<Person>) -> rn_kernel::Outcome<Option<i64>> {
    struct At(i64);
    impl FromRow for At {
        fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
            Ok(Self(row.int("created_at")?))
        }
    }
    Ok(state
        .store
        .reads()
        .query_opt::<At>("SELECT created_at FROM party WHERE id = $1", bind![person])
        .await?
        .map(|at| at.0))
}

struct Address {
    value: String,
    verified: bool,
}

impl FromRow for Address {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            value: row.text("value")?,
            verified: row.int_opt("verified_at")?.is_some(),
        })
    }
}

/// The `email` claims.
///
/// The *verified* address wins when there is one, because `email_verified` is
/// a claim an RP acts on: a client that provisions an account from it must not
/// be handed an address nobody proved. Ordering by `verified_at IS NULL` puts
/// the proven ones first, and the oldest of those is the person's own.
async fn email(state: &AppState, person: Id<Person>) -> Vec<(String, Value)> {
    const SQL: &str = "SELECT f.value, f.verified_at FROM factor f \
         JOIN identity i ON i.id = f.identity_id \
         WHERE i.person_id = $1 AND f.kind = 'email' \
         ORDER BY (f.verified_at IS NULL), f.id LIMIT 1";
    match state
        .store
        .reads()
        .query_opt::<Address>(SQL, bind![person])
        .await
    {
        Ok(Some(address)) => vec![
            ("email".to_owned(), address.value.into()),
            ("email_verified".to_owned(), address.verified.into()),
        ],
        _ => Vec::new(),
    }
}

struct GroupName(String);

impl FromRow for GroupName {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self(row.text("display_name")?))
    }
}

/// The `groups` claim: memberships under the client's owning organization,
/// and nothing outside that subtree.
///
/// The statement is the rule. A group counts when the organization owns it —
/// `resource.owner_party_id` is the owner, and `party_resource` is the join
/// from that resource to the group's party row — or when it *is* the
/// organization, which is its own root group. A client owned by the platform
/// has no subtree, and gets nothing rather than everything.
async fn groups(
    state: &AppState,
    client: &rn_kernel::oidc::ClientRow,
    person: Id<Person>,
) -> Vec<String> {
    const SQL: &str = "SELECT p.display_name FROM membership m \
         JOIN party p ON p.id = m.group_id \
         WHERE m.party_id = $1 AND (p.id = $2 OR p.id IN ( \
             SELECT pr.party_id FROM party_resource pr \
             JOIN resource r ON r.id = pr.resource_id \
             WHERE r.owner_party_id = $2 AND r.kind = 'group')) \
         ORDER BY p.display_name";
    let Some(owner) = client.owner_party_id else {
        return Vec::new();
    };
    state
        .store
        .reads()
        .query::<GroupName>(SQL, bind![person, owner])
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|row| row.0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn only_the_bearer_header_is_read_and_only_in_the_shape_it_has() {
        let with = |value: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::AUTHORIZATION, HeaderValue::from_str(value).unwrap());
            headers
        };
        assert_eq!(bearer(&with("Bearer abc")).as_deref(), Some("abc"));
        assert_eq!(bearer(&with("Bearer  abc ")).as_deref(), Some("abc"));
        assert!(bearer(&with("bearer abc")).is_none(), "the scheme is cased");
        assert!(bearer(&with("Basic abc")).is_none());
        assert!(bearer(&with("Bearer ")).is_none());
        assert!(bearer(&HeaderMap::new()).is_none());
        // A cookie is not a credential here, and there is no code that would
        // read one: this is the whole of what the endpoint accepts.
        let mut cookie = HeaderMap::new();
        cookie.insert(header::COOKIE, HeaderValue::from_static("rn_session=abc"));
        assert!(bearer(&cookie).is_none());
    }

    #[test]
    fn every_refusal_is_the_same_challenge_with_a_different_sentence() {
        let response = challenge("invalid_token", "no");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let value = response.headers()[header::WWW_AUTHENTICATE]
            .to_str()
            .expect("ascii");
        assert!(
            value.starts_with("Bearer error=\"invalid_token\""),
            "{value}"
        );
    }
}
