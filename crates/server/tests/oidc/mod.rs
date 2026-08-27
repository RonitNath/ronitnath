//! What both OpenID suites need: a node with an operator, a signing key, and
//! a registered client.
//!
//! Every fixture goes through a real command. Registering a client here runs
//! `cmd::register_client`, minting a key runs `cmd::rotate_signing_key`, and
//! consenting runs the endpoint — so a test cannot prove a path the product
//! does not have.

#![allow(dead_code)]

mod negative;
mod rp;

use axum_test::TestServer;
use rn_api::oidc::{ClientAuthMethod, ClientMetadata, GrantType, Scope};
use rn_kernel::domain::Token;
use rn_kernel::ids::{Id, OidcClient};
use rn_kernel::{Event, Principal};
use rn_site::AppState;
use tokio::sync::OnceCell;

use crate::harness;

static NODE: OnceCell<AppState> = OnceCell::const_new();

/// The one node both suites run against.
pub async fn state() -> AppState {
    harness::node(&NODE, "127.0.0.1:8214", "127.0.0.1:8314").await
}

/// A registered client, and what a test needs to speak as it.
pub struct Client {
    /// The `client_id` on the wire — the client's public id.
    pub id: String,
    /// The secret, minted once when it was registered.
    pub secret: Option<String>,
    /// Where its responses land.
    pub redirect_uri: String,
    /// The rowid, for the fixtures that read a row directly.
    pub row: Id<OidcClient>,
}

/// The metadata a confidential, first-party-shaped client registers with.
pub fn metadata(name: &str, redirect_uri: &str) -> ClientMetadata {
    ClientMetadata {
        client_name: name.to_owned(),
        client_uri: Some("https://example.test/".to_owned()),
        logo_uri: None,
        redirect_uris: vec![redirect_uri.to_owned()],
        post_logout_redirect_uris: vec!["https://example.test/bye".to_owned()],
        backchannel_logout_uri: None,
        token_endpoint_auth_method: ClientAuthMethod::ClientSecretBasic,
        jwks: None,
        grant_types: vec![GrantType::AuthorizationCode, GrantType::RefreshToken],
        scopes: vec![
            Scope::Openid,
            Scope::Profile,
            Scope::Email,
            Scope::Groups,
            Scope::OfflineAccess,
        ],
        trusted: false,
        members_only: false,
    }
}

/// The same registration, refusing anybody with no relation under its owner.
pub fn members_only(mut metadata: ClientMetadata) -> ClientMetadata {
    metadata.members_only = true;
    metadata
}

/// Register a client as the platform operator.
pub async fn register_client(
    state: &AppState,
    operator: &harness::Somebody,
    metadata: ClientMetadata,
) -> Client {
    let redirect_uri = metadata.redirect_uris[0].clone();
    let registered = rn_kernel::cmd::register_client(
        &harness::ctx(state, principal(state, operator).await),
        &rn_api::commands::RegisterClient {
            owner: None,
            metadata,
        },
    )
    .await
    .expect("a registration");
    let Event::ClientRegistered { client, .. } = registered.committed.event else {
        panic!("registering a client produces a registration event");
    };
    Client {
        id: client.public(state.ids()).to_string(),
        secret: registered.secret.map(|token| token.expose().to_owned()),
        redirect_uri,
        row: client,
    }
}

/// The signing key every token in these suites is signed under. Minted once —
/// RSA key generation is the slowest thing in the binary.
pub async fn ensure_key(state: &AppState, operator: &harness::Somebody) {
    static MINTED: OnceCell<()> = OnceCell::const_new();
    MINTED
        .get_or_init(|| async {
            rn_kernel::cmd::rotate_signing_key(
                &harness::ctx(state, principal(state, operator).await),
                &rn_api::commands::RotateSigningKey {},
            )
            .await
            .expect("a signing key");
        })
        .await;
}

/// The principal a session cookie resolves to.
pub async fn principal(state: &AppState, who: &harness::Somebody) -> Principal {
    rn_kernel::principal::resolve(&state.store.reads(), &Token::from_wire(&who.token))
        .await
        .expect("the session resolves")
        .principal
}

/// A server over the whole surface.
pub fn server(state: &AppState) -> TestServer {
    harness::server(state)
}

/// The verifier and challenge every flow here uses.
///
/// RFC 7636's own worked example (Appendix B), so a failure is a failure of
/// this deployment rather than of the fixture's arithmetic.
pub const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
/// The S256 challenge of [`VERIFIER`].
pub const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

/// An `Authorization: Basic` header for a client.
pub fn basic(client: &Client) -> String {
    use base64::Engine as _;
    let secret = client.secret.clone().unwrap_or_default();
    let raw = base64::engine::general_purpose::STANDARD.encode(format!("{}:{secret}", client.id));
    format!("Basic {raw}")
}

/// The `code` out of a redirect the authorization endpoint answered with.
pub fn parameter(location: &str, name: &str) -> Option<String> {
    let query = location.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| decode(value))
    })
}

/// Percent-encode a value into a query a test builds.
pub fn encode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Percent-decode a value, for a caller that has one rather than a query.
pub fn decode_value(raw: &str) -> String {
    decode(raw)
}

/// Percent-decode a query value the endpoint wrote.
fn decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => match u8::from_str_radix(&raw[i + 1..i + 3], 16) {
                Ok(byte) => {
                    out.push(byte);
                    i += 2;
                }
                Err(_) => out.push(b'%'),
            },
            other => out.push(other),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
