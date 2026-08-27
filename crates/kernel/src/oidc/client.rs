//! The client registry: what a relying party is here, and how it proves it.
//!
//! Registration is an operator command today (`register-client`), and the row
//! is shaped like an RFC 7591 registration request anyway, so the day dynamic
//! registration is wanted the endpoint has nothing to migrate. The owner is a
//! person or an organization — never a group, which the schema's trigger makes
//! unrepresentable — and the owner is also the **sector**: a person's `sub` at
//! this client is pairwise under whoever owns it.
//!
//! ## Redirect matching
//!
//! Exact string equality, with one exception, and it is the exception RFC 8252
//! §7.3 requires: a native app that listens on loopback cannot know its port
//! until it has one, so `http://127.0.0.1:*/path` and `http://[::1]:*/path`
//! match on everything but the port. Nothing else is relaxed — no prefix
//! match, no wildcard host, no ignored query — because every one of those is a
//! way to land an authorization code somewhere the registration did not name.
//!
//! ## The secret
//!
//! 256 bits from the OS, handed back once, stored as its SHA-256 and compared
//! in constant time. Not argon2id: there is no dictionary to run against a
//! secret this server minted and no human who chose it, so a slow hash would
//! buy nothing and cost the token endpoint its latency. Argon2 is for the
//! things people type.

use rn_api::oidc::{ClientAuthMethod, ClientMetadata, GrantType, Scope};
use sha2::{Digest as _, Sha256};

use crate::Timestamp;
use crate::error::{KernelError, Outcome, decline};
use crate::ids::{Id, OidcClient, Person};
use crate::store::{Cursor, FromRow, Reads, RowError};
use crate::{bind, domain::Token};

/// A registered client.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientRow {
    /// The rowid. Its public id is the `client_id` on the wire.
    pub id: Id<OidcClient>,
    /// The person or organization that owns it — and the sector its subjects
    /// are pairwise under. `None` is the deployment itself, `platform:*`.
    pub owner_party_id: Option<Id<Person>>,
    /// The service party `client_credentials` mints tokens for.
    pub service_party_id: Id<Person>,
    /// Everything the registration said.
    pub metadata: ClientMetadata,
    /// The SHA-256 of its secret, for the methods that use one.
    pub secret_hash: Option<Vec<u8>>,
    /// When it was registered.
    pub created_at: Timestamp,
}

impl ClientRow {
    /// The columns this row reads.
    pub const COLUMNS: &'static str = "id, owner_party_id, service_party_id, client_name, \
        client_uri, logo_uri, redirect_uris, post_logout_redirect_uris, backchannel_logout_uri, \
        token_endpoint_auth_method, jwks, grant_types, scopes, trusted, members_only, \
        secret_hash, created_at";

    /// Whether this client may use a grant.
    #[must_use]
    pub fn allows(&self, grant: GrantType) -> bool {
        self.metadata.grant_types.contains(&grant)
    }

    /// Whether every scope asked for is one this client may ask for.
    #[must_use]
    pub fn allows_scopes(&self, asked: &[Scope]) -> bool {
        asked.iter().all(|s| self.metadata.scopes.contains(s))
    }

    /// Whether a redirect URI is one of the registered ones.
    #[must_use]
    pub fn accepts_redirect(&self, candidate: &str) -> bool {
        self.metadata
            .redirect_uris
            .iter()
            .any(|registered| redirect_matches(registered, candidate))
    }

    /// The sector this client's subjects are pairwise under: its owner's
    /// party, or [`super::subject::PLATFORM_SECTOR`] for the deployment's own.
    #[must_use]
    pub fn sector(&self) -> i64 {
        self.owner_party_id
            .map_or(super::subject::PLATFORM_SECTOR, Id::get)
    }

    /// Whether a post-logout redirect URI is one of the registered ones.
    #[must_use]
    pub fn accepts_post_logout(&self, candidate: &str) -> bool {
        self.metadata
            .post_logout_redirect_uris
            .iter()
            .any(|registered| redirect_matches(registered, candidate))
    }
}

/// Whether a candidate redirect matches a registered one.
///
/// Equality, plus the loopback-port exception of RFC 8252 §7.3 and nothing
/// else. The exception applies only when *both* sides are loopback over
/// `http`, so a registration naming a real host is never matched by a
/// redirect naming loopback or the other way round.
#[must_use]
pub fn redirect_matches(registered: &str, candidate: &str) -> bool {
    if registered == candidate {
        return true;
    }
    match (loopback_parts(registered), loopback_parts(candidate)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// A loopback redirect with its port removed: `(host, rest)`.
fn loopback_parts(uri: &str) -> Option<(&'static str, String)> {
    for host in ["127.0.0.1", "[::1]"] {
        let prefix = format!("http://{host}");
        if let Some(rest) = uri.strip_prefix(&prefix) {
            // Either `:<port><rest>` or `<rest>` with no port at all. A port
            // that is not digits is not a port, and the URI is not loopback.
            let rest = match rest.strip_prefix(':') {
                Some(after) => {
                    let digits = after.chars().take_while(char::is_ascii_digit).count();
                    if digits == 0 {
                        return None;
                    }
                    &after[digits..]
                }
                None => rest,
            };
            if rest.is_empty() || rest.starts_with('/') {
                return Some((host, rest.to_owned()));
            }
            return None;
        }
    }
    None
}

impl FromRow for ClientRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let method = row.text("token_endpoint_auth_method")?;
        Ok(Self {
            id: row.id("id")?,
            owner_party_id: row.id_opt("owner_party_id")?,
            service_party_id: row.id("service_party_id")?,
            metadata: ClientMetadata {
                client_name: row.text("client_name")?,
                client_uri: row.text_opt("client_uri")?,
                logo_uri: row.text_opt("logo_uri")?,
                redirect_uris: strings(&row.text("redirect_uris")?, "redirect_uris")?,
                post_logout_redirect_uris: strings(
                    &row.text("post_logout_redirect_uris")?,
                    "post_logout_redirect_uris",
                )?,
                backchannel_logout_uri: row.text_opt("backchannel_logout_uri")?,
                token_endpoint_auth_method: ClientAuthMethod::parse(&method).ok_or(
                    RowError::Column {
                        column: "token_endpoint_auth_method".to_owned(),
                        wanted: "ClientAuthMethod",
                    },
                )?,
                jwks: row.text_opt("jwks")?,
                grant_types: words(&row.text("grant_types")?, GrantType::parse, "grant_types")?,
                scopes: words(&row.text("scopes")?, Scope::parse, "scopes")?,
                trusted: row.int("trusted")? != 0,
                members_only: row.int("members_only")? != 0,
            },
            secret_hash: row.blob("secret_hash").ok(),
            created_at: row.int("created_at")?,
        })
    }
}

fn strings(raw: &str, column: &str) -> Result<Vec<String>, RowError> {
    serde_json::from_str(raw).map_err(|_| RowError::Column {
        column: column.to_owned(),
        wanted: "a JSON array of strings",
    })
}

fn words<T>(raw: &str, parse: fn(&str) -> Option<T>, column: &str) -> Result<Vec<T>, RowError> {
    let list: Vec<String> = strings(raw, column)?;
    list.into_iter()
        .map(|word| {
            parse(&word).ok_or_else(|| RowError::Column {
                column: column.to_owned(),
                wanted: "a word this deployment admits",
            })
        })
        .collect()
}

/// Render a list of words as the JSON array a column stores.
#[must_use]
pub fn as_json<T: Copy>(words: &[T], as_str: fn(T) -> &'static str) -> String {
    let list: Vec<&str> = words.iter().copied().map(as_str).collect();
    serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_owned())
}

/// Render a list of strings as the JSON array a column stores.
#[must_use]
pub fn strings_as_json(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_owned())
}

/// One live client by rowid. A withdrawn one reads as absent, which is what
/// makes `delete-client` a tombstone rather than a `DELETE` that would take
/// the audit trail's referents with it.
pub const BY_ID_SQL: &str = "SELECT id, owner_party_id, service_party_id, client_name, \
     client_uri, logo_uri, redirect_uris, post_logout_redirect_uris, backchannel_logout_uri, \
     token_endpoint_auth_method, jwks, grant_types, scopes, trusted, members_only, \
     secret_hash, created_at \
     FROM oidc_client WHERE id = $1 AND deleted_at IS NULL";

/// Insert a registration.
pub const INSERT_SQL: &str = "INSERT INTO oidc_client \
     (owner_party_id, service_party_id, client_name, client_uri, logo_uri, redirect_uris, \
      post_logout_redirect_uris, backchannel_logout_uri, token_endpoint_auth_method, jwks, \
      grant_types, scopes, trusted, members_only, secret_hash, created_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16) RETURNING id";

/// Replace a registration's metadata. Guarded on the row being live.
pub const UPDATE_SQL: &str = "UPDATE oidc_client SET client_name = $2, client_uri = $3, \
     logo_uri = $4, redirect_uris = $5, post_logout_redirect_uris = $6, \
     backchannel_logout_uri = $7, token_endpoint_auth_method = $8, jwks = $9, \
     grant_types = $10, scopes = $11, trusted = $12, members_only = $13 \
     WHERE id = $1 AND deleted_at IS NULL";

/// Mint a new secret. The old one stops working in the same statement: an
/// overlap would be two live credentials with no way to tell which leaked.
pub const ROTATE_SECRET_SQL: &str =
    "UPDATE oidc_client SET secret_hash = $2, rotated_at = $3 WHERE id = $1 AND deleted_at IS NULL";

/// Withdraw a client.
pub const DELETE_SQL: &str =
    "UPDATE oidc_client SET deleted_at = $2 WHERE id = $1 AND deleted_at IS NULL";

/// The service party a client's `client_credentials` tokens speak as.
pub const SERVICE_PARTY_SQL: &str = "INSERT INTO party (kind, display_name, status, created_at) \
     VALUES ('service', $1, 'active', $2) RETURNING id";

/// Read a client.
pub async fn load(store: &impl Reads, id: Id<OidcClient>) -> Outcome<Option<ClientRow>> {
    Ok(store.query_opt::<ClientRow>(BY_ID_SQL, bind![id]).await?)
}

/// Mint a client secret: 256 bits, once.
#[must_use]
pub fn mint_secret() -> Token {
    Token::mint()
}

/// The digest a `secret_hash` column stores.
#[must_use]
pub fn secret_digest(secret: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hasher.finalize().to_vec()
}

/// Compare a presented secret against a stored digest, in time that does not
/// depend on how much of it was right.
#[must_use]
pub fn secret_matches(stored: Option<&Vec<u8>>, presented: &str) -> bool {
    let Some(stored) = stored else {
        return false;
    };
    let digest = secret_digest(presented);
    if digest.len() != stored.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in digest.iter().zip(stored) {
        difference |= a ^ b;
    }
    difference == 0
}

/// What a caller presented at the token endpoint.
#[derive(Debug, Clone, Default)]
pub struct Credential<'a> {
    /// A client secret, from the header or the body.
    pub secret: Option<&'a str>,
    /// A `private_key_jwt` assertion.
    pub assertion: Option<&'a str>,
}

/// Record a `private_key_jwt` assertion's `jti`, so a replay collides at the
/// database rather than at a check the endpoint might skip.
pub const ASSERTION_SQL: &str =
    "INSERT INTO oidc_assertion (jti, client_id, expires_at) VALUES ($1, $2, $3)";

/// Whether an assertion's `jti` has already been seen.
pub const ASSERTION_SEEN_SQL: &str =
    "SELECT count(*) AS n FROM oidc_assertion WHERE jti = $1 AND expires_at > $2";

/// Authenticate a client, by the method its registration names.
///
/// The method is the *registration's*, never the request's: a client
/// registered for `private_key_jwt` that sends a secret is not authenticated
/// by the secret, and a client registered for `none` that sends one is not
/// authenticated by it either. Choosing the method from what arrived is how a
/// downgrade gets in.
///
/// `audience` is the token endpoint's URL, which a `private_key_jwt` assertion
/// must name (RFC 7523 §3): an assertion is a bearer credential, and one that
/// did not say where it was for could be replayed at another issuer.
pub async fn authenticate(
    store: &impl Reads,
    client: &ClientRow,
    credential: &Credential<'_>,
    audience: &str,
    now: Timestamp,
) -> Outcome<()> {
    match client.metadata.token_endpoint_auth_method {
        ClientAuthMethod::None => Ok(()),
        ClientAuthMethod::ClientSecretBasic | ClientAuthMethod::ClientSecretPost => {
            match credential.secret {
                Some(secret) if secret_matches(client.secret_hash.as_ref(), secret) => Ok(()),
                _ => decline(),
            }
        }
        ClientAuthMethod::PrivateKeyJwt => {
            let (Some(assertion), Some(jwks)) = (credential.assertion, &client.metadata.jwks)
            else {
                return decline();
            };
            verify_assertion(store, client, assertion, jwks, audience, now).await
        }
    }
}

/// Whether a `private_key_jwt` assertion is this client's, unexpired and not
/// a replay.
async fn verify_assertion(
    store: &impl Reads,
    client: &ClientRow,
    assertion: &str,
    jwks: &str,
    audience: &str,
    now: Timestamp,
) -> Outcome<()> {
    use super::jwt;
    let Some(parts) = jwt::split(assertion) else {
        return decline();
    };
    let client_id = client.id.public(store.ids()).to_string();
    // `iss` and `sub` are both the client_id (RFC 7523 §3); `aud` is the
    // endpoint, so the assertion cannot be replayed at another issuer.
    if jwt::claim(&parts.claims, "iss") != Some(client_id.as_str())
        || jwt::claim(&parts.claims, "sub") != Some(client_id.as_str())
        || !jwt::audience_contains(&parts.claims, audience)
    {
        return decline();
    }
    match jwt::claim_int(&parts.claims, "exp") {
        Some(exp) if exp > now => {}
        _ => return decline(),
    }
    let Some(jti) = jwt::claim(&parts.claims, "jti") else {
        return decline();
    };
    if store
        .query::<crate::store::Count>(ASSERTION_SEEN_SQL, bind![jti, now])
        .await?
        .first()
        .is_some_and(|count| count.0 > 0)
    {
        return decline();
    }
    if !super::jwks_verifies(jwks, &parts) {
        return decline();
    }
    Ok(())
}

/// Turn a public id string a caller sent into the client it names.
pub fn decode_client_id(key: &crate::ids::IdKey, raw: &str) -> Outcome<Id<OidcClient>> {
    crate::ids::parse::<OidcClient>(key, raw).map_or_else(|_| decline(), Ok)
}

/// The rows a withdrawal takes with it, in the order they are written.
pub const REVOKE_CLIENT_TOKENS_SQL: &str =
    "UPDATE oidc_token SET revoked_at = $2 WHERE client_id = $1 AND revoked_at IS NULL";
/// The consents a withdrawal takes with it.
pub const DELETE_CLIENT_CONSENTS_SQL: &str = "DELETE FROM oidc_consent WHERE client_id = $1";
/// The consent *grants* a withdrawal takes with it.
pub const DELETE_CLIENT_GRANTS_SQL: &str =
    "DELETE FROM relation WHERE object_kind = 'oidc_client' AND object_id = $1";

/// An invariant failure with a sentence, for the paths that cannot decline.
pub fn invariant<T>(message: &str) -> Outcome<T> {
    Err(KernelError::Invariant(message.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_redirect_matches_exactly_and_a_loopback_one_ignores_its_port() {
        assert!(redirect_matches(
            "https://app.test/callback",
            "https://app.test/callback"
        ));
        assert!(!redirect_matches(
            "https://app.test/callback",
            "https://app.test/callback?x=1"
        ));
        assert!(!redirect_matches("https://app.test/", "https://app.test"));
        assert!(!redirect_matches(
            "https://app.test/callback",
            "https://app.test.evil/callback"
        ));
        assert!(redirect_matches(
            "http://127.0.0.1:1234/cb",
            "http://127.0.0.1:57922/cb"
        ));
        assert!(redirect_matches("http://[::1]:1/cb", "http://[::1]:2/cb"));
        assert!(
            !redirect_matches("http://127.0.0.1:1234/cb", "http://[::1]:1234/cb"),
            "two different loopback hosts"
        );
        assert!(
            !redirect_matches("http://127.0.0.1:1234/cb", "http://127.0.0.1:1234/other"),
            "the path is still exact"
        );
        assert!(
            !redirect_matches("http://127.0.0.1:1234/cb", "https://127.0.0.1:1234/cb"),
            "the scheme is still exact"
        );
        assert!(
            !redirect_matches("http://127.0.0.1:1234/cb", "http://127.0.0.1.evil/cb"),
            "a host that merely starts with loopback"
        );
    }

    #[test]
    fn a_secret_is_compared_against_its_digest_and_never_against_itself() {
        let secret = mint_secret();
        let stored = secret_digest(secret.expose());
        assert_ne!(stored, secret.expose().as_bytes());
        assert!(secret_matches(Some(&stored), secret.expose()));
        assert!(!secret_matches(Some(&stored), "not it"));
        assert!(!secret_matches(None, secret.expose()), "a public client");
        assert!(!secret_matches(Some(&stored), ""));
    }
}
