//! The OpenID Provider's wire vocabulary: the client registry's shape, the
//! scopes and grants this deployment admits, and the arguments of the thirteen
//! commands that move any of it.
//!
//! Every deployment is its own issuer with its own keys and its own client
//! registry; nothing here federates to another one. `iss` is the deployment's
//! public origin exactly, and a client registered on `ronitnath.com` is not a
//! client anywhere else.
//!
//! The protocol commands (`authorize`, `exchange-code`, `refresh-token`,
//! `client-credentials`, `revoke-token`, `end-session`) carry the credential
//! they are authorised by — a client secret or a `private_key_jwt` assertion —
//! rather than a flag some endpoint set after checking. They are bound at
//! `/api/cmd/<name>` like every other command, and that costs nothing: the
//! reply there is the event, never the secret the command minted, and the
//! credential still has to be right.

use serde::{Deserialize, Serialize};

use crate::ids::PublicId;

/// How a client proves it is itself at the token endpoint.
///
/// `none` is a public client — a browser or a native app, which cannot keep a
/// secret — and PKCE is what stands in its place. It is required of every
/// client type here, confidential ones included (RFC 9700 §2.1.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientAuthMethod {
    /// The secret in an `Authorization: Basic` header. The default, and the
    /// one RFC 6749 §2.3.1 says a server MUST support.
    ClientSecretBasic,
    /// The secret in the form body.
    ClientSecretPost,
    /// A JWT signed by the client's own key, verified against its registered
    /// JWKS. Replay is refused by `jti`.
    PrivateKeyJwt,
    /// No client authentication: a public client.
    None,
}

impl ClientAuthMethod {
    /// The value as the column stores it and the metadata document prints it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClientSecretBasic => "client_secret_basic",
            Self::ClientSecretPost => "client_secret_post",
            Self::PrivateKeyJwt => "private_key_jwt",
            Self::None => "none",
        }
    }

    /// Every method, for the discovery document and the vocabulary tests.
    pub const ALL: [Self; 4] = [
        Self::ClientSecretBasic,
        Self::ClientSecretPost,
        Self::PrivateKeyJwt,
        Self::None,
    ];

    /// Read one back, refusing anything else.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.as_str() == raw)
    }

    /// Whether this method is proven by a shared secret.
    #[must_use]
    pub const fn uses_secret(self) -> bool {
        matches!(self, Self::ClientSecretBasic | Self::ClientSecretPost)
    }
}

/// The grants this deployment admits. Implicit and hybrid are absent by
/// decision, not by omission — see `docs/oidc.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantType {
    /// The only interactive flow: `response_type=code`, PKCE S256 required.
    AuthorizationCode,
    /// Rotation with family revocation on reuse.
    RefreshToken,
    /// A service principal owned by the client's owner.
    ClientCredentials,
}

impl GrantType {
    /// The value as it appears on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AuthorizationCode => "authorization_code",
            Self::RefreshToken => "refresh_token",
            Self::ClientCredentials => "client_credentials",
        }
    }

    /// Every grant, for the discovery document.
    pub const ALL: [Self; 3] = [
        Self::AuthorizationCode,
        Self::RefreshToken,
        Self::ClientCredentials,
    ];

    /// Read one back.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|g| g.as_str() == raw)
    }
}

/// The scopes this deployment admits.
///
/// `openid` is what makes a request an OpenID Connect request rather than a
/// bare OAuth one, and the authorization endpoint requires it. The other three
/// each name a claim group `userinfo` returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// Ask for an id token at all.
    Openid,
    /// `name`, `preferred_username`, `updated_at`.
    Profile,
    /// `email`, `email_verified`.
    Email,
    /// Memberships under the client's owning organization, and nothing
    /// outside that subtree.
    Groups,
    /// A refresh token.
    OfflineAccess,
}

impl Scope {
    /// The value as it appears in a `scope` parameter.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Openid => "openid",
            Self::Profile => "profile",
            Self::Email => "email",
            Self::Groups => "groups",
            Self::OfflineAccess => "offline_access",
        }
    }

    /// Every scope, for the discovery document.
    pub const ALL: [Self; 5] = [
        Self::Openid,
        Self::Profile,
        Self::Email,
        Self::Groups,
        Self::OfflineAccess,
    ];

    /// Read one back, refusing anything this deployment does not admit.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == raw)
    }

    /// Parse a space-delimited `scope` parameter, dropping nothing silently:
    /// an unknown word makes the whole list `None`.
    #[must_use]
    pub fn parse_list(raw: &str) -> Option<Vec<Self>> {
        raw.split_whitespace().map(Self::parse).collect()
    }

    /// Render a list back as a `scope` parameter.
    #[must_use]
    pub fn join(scopes: &[Self]) -> String {
        scopes
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The metadata a client registration carries (RFC 7591's shape, without
/// dynamic registration — `docs/oidc.md` says why).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientMetadata {
    /// How the consent page names it.
    pub client_name: String,
    /// The client's home page, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_uri: Option<String>,
    /// Its logo, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_uri: Option<String>,
    /// Where an authorization response may land. Matched exactly, with the
    /// loopback-port exception RFC 8252 §7.3 requires of a native app.
    pub redirect_uris: Vec<String>,
    /// Where an RP-initiated logout may land. Matched exactly.
    #[serde(default)]
    pub post_logout_redirect_uris: Vec<String>,
    /// Where a logout token is POSTed when a session that minted a token for
    /// this client ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backchannel_logout_uri: Option<String>,
    /// How it proves itself at the token endpoint.
    pub token_endpoint_auth_method: ClientAuthMethod,
    /// Its public keys, as a JWKS document, for `private_key_jwt`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jwks: Option<String>,
    /// Which grants it may use.
    pub grant_types: Vec<GrantType>,
    /// Which scopes it may ask for.
    pub scopes: Vec<Scope>,
    /// First-party: the consent page is skipped, because the deployment is
    /// already the party being consented to.
    #[serde(default)]
    pub trusted: bool,
    /// Refuse a person who holds no relation under the client's owner.
    #[serde(default)]
    pub members_only: bool,
}

/// Register a client. Operator-only today; the owner may be an organization,
/// which is what the resource shape is for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterClient {
    /// The organization that owns it. Absent means the deployment's platform
    /// organization — `platform:*`, the singleton every operator relation
    /// hangs off, which is also the sector operator-registered clients are
    /// pairwise under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<PublicId>,
    /// Everything else.
    #[serde(flatten)]
    pub metadata: ClientMetadata,
}

/// Replace a client's metadata. Every field is sent; a partial update would
/// need a merge rule, and "the registration is what you sent" has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateClient {
    /// Which client.
    pub client: PublicId,
    /// Its new metadata.
    #[serde(flatten)]
    pub metadata: ClientMetadata,
}

/// Mint a new secret for a client. The old one stops working immediately: an
/// overlap would be a second live credential with no way to tell which leaked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RotateClientSecret {
    /// Which client.
    pub client: PublicId,
}

/// Withdraw a client. Its tokens, codes and consents go with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteClient {
    /// Which client.
    pub client: PublicId,
}

/// Choose or change the acting person's handle — their `preferred_username`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetHandle {
    /// Lowercase `[a-z0-9-]`, 3–32 characters, unique deployment-wide.
    pub handle: String,
}

/// Consent, and mint an authorization code.
///
/// The actor is the person: it is their session the code is bound to and
/// their consent it records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Authorize {
    /// The client, by its public id — which is its `client_id`.
    pub client_id: String,
    /// Where the response will land. Already matched against the
    /// registration by the endpoint, and matched again here.
    pub redirect_uri: String,
    /// What is being asked for.
    pub scopes: Vec<Scope>,
    /// The RP's replay nonce, carried into the id token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    /// The S256 PKCE challenge. Required of every client type.
    pub code_challenge: String,
}

/// Exchange an authorization code for tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangeCode {
    /// Which client is asking.
    pub client_id: String,
    /// Its secret, for the two `client_secret_*` methods.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// Its `private_key_jwt` assertion, for that method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assertion: Option<String>,
    /// The code.
    pub code: String,
    /// The redirect URI the code was minted against, identical.
    pub redirect_uri: String,
    /// The PKCE verifier. Absent where a challenge was stored is a downgrade
    /// and is refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_verifier: Option<String>,
}

/// Exchange a refresh token for a new pair. The old one dies; presenting it
/// again revokes the whole family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshToken {
    /// Which client is asking.
    pub client_id: String,
    /// Its secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// Its assertion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assertion: Option<String>,
    /// The refresh token.
    pub refresh_token: String,
    /// A narrower scope set than the one consented to, if the client wants
    /// one. Never a wider one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<Scope>>,
}

/// Mint a token whose principal is the client's own service party.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientCredentials {
    /// Which client is asking.
    pub client_id: String,
    /// Its secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// Its assertion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assertion: Option<String>,
    /// What it is asking for.
    #[serde(default)]
    pub scopes: Vec<Scope>,
}

/// Revoke a token (RFC 7009). A refresh token takes its family with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokeToken {
    /// Which client is asking.
    pub client_id: String,
    /// Its secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// Its assertion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assertion: Option<String>,
    /// The token being revoked.
    pub token: String,
}

/// Withdraw the acting person's consent to a client, and every token it holds
/// for them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokeConsent {
    /// Which client.
    pub client: PublicId,
}

/// Mint a fresh signing key; the previous active one becomes `retiring` and
/// keeps verifying until it is retired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RotateSigningKey {}

/// End the acting session at the OP's own request — RP-initiated logout.
///
/// Distinct from `sign-out` only in what produced it, and that difference is
/// the audit row: both end the session and cascade to its tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EndSession {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{id, round_trip};

    fn metadata() -> ClientMetadata {
        ClientMetadata {
            client_name: "oauth2-proxy".into(),
            client_uri: Some("https://example.test/".into()),
            logo_uri: None,
            redirect_uris: vec!["http://127.0.0.1:4180/oauth2/callback".into()],
            post_logout_redirect_uris: vec!["http://127.0.0.1:4180/".into()],
            backchannel_logout_uri: Some("http://127.0.0.1:4180/logout".into()),
            token_endpoint_auth_method: ClientAuthMethod::ClientSecretBasic,
            jwks: None,
            grant_types: vec![GrantType::AuthorizationCode, GrantType::RefreshToken],
            scopes: vec![Scope::Openid, Scope::Profile, Scope::Email],
            trusted: false,
            members_only: false,
        }
    }

    #[test]
    fn the_registry_commands_round_trip() {
        round_trip(&RegisterClient {
            owner: Some(id("o_")),
            metadata: metadata(),
        });
        round_trip(&UpdateClient {
            client: id("c_"),
            metadata: metadata(),
        });
        round_trip(&RotateClientSecret { client: id("c_") });
        round_trip(&DeleteClient { client: id("c_") });
        round_trip(&RevokeConsent { client: id("c_") });
        round_trip(&RotateSigningKey {});
        round_trip(&EndSession {});
        round_trip(&SetHandle {
            handle: "ronit".into(),
        });
    }

    #[test]
    fn the_protocol_commands_round_trip() {
        round_trip(&Authorize {
            client_id: "c_AAAAAAAAAAAAAAAAAAAAAA".into(),
            redirect_uri: "http://127.0.0.1:4180/oauth2/callback".into(),
            scopes: vec![Scope::Openid, Scope::Email],
            nonce: Some("n-0S6_WzA2Mj".into()),
            code_challenge: "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM".into(),
        });
        round_trip(&ExchangeCode {
            client_id: "c_AAAAAAAAAAAAAAAAAAAAAA".into(),
            client_secret: Some("secret".into()),
            assertion: None,
            code: "opaque".into(),
            redirect_uri: "http://127.0.0.1:4180/oauth2/callback".into(),
            code_verifier: Some("verifier".into()),
        });
        round_trip(&RefreshToken {
            client_id: "c_AAAAAAAAAAAAAAAAAAAAAA".into(),
            client_secret: None,
            assertion: Some("ey.ey.sig".into()),
            refresh_token: "opaque".into(),
            scopes: Some(vec![Scope::Openid]),
        });
        round_trip(&ClientCredentials {
            client_id: "c_AAAAAAAAAAAAAAAAAAAAAA".into(),
            client_secret: Some("secret".into()),
            assertion: None,
            scopes: vec![],
        });
        round_trip(&RevokeToken {
            client_id: "c_AAAAAAAAAAAAAAAAAAAAAA".into(),
            client_secret: Some("secret".into()),
            assertion: None,
            token: "opaque".into(),
        });
    }

    #[test]
    fn a_scope_list_is_all_of_it_or_none_of_it() {
        assert_eq!(
            Scope::parse_list("openid profile email"),
            Some(vec![Scope::Openid, Scope::Profile, Scope::Email])
        );
        assert_eq!(Scope::parse_list("openid nonsense"), None);
        assert_eq!(Scope::parse_list(""), Some(vec![]));
        assert_eq!(
            Scope::join(&[Scope::Openid, Scope::OfflineAccess]),
            "openid offline_access"
        );
    }

    #[test]
    fn the_vocabularies_are_distinct_and_round_trip_through_their_words() {
        for method in ClientAuthMethod::ALL {
            assert_eq!(ClientAuthMethod::parse(method.as_str()), Some(method));
        }
        for grant in GrantType::ALL {
            assert_eq!(GrantType::parse(grant.as_str()), Some(grant));
        }
        for scope in Scope::ALL {
            assert_eq!(Scope::parse(scope.as_str()), Some(scope));
        }
        assert_eq!(ClientAuthMethod::parse("client_secret_jwt"), None);
        assert_eq!(GrantType::parse("implicit"), None);
        assert_eq!(Scope::parse("address"), None);
    }
}
