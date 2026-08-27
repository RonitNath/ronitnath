//! Back-channel logout: telling the relying parties that a session ended.
//!
//! The POSTs happen **off the request path**, on a spawned task, and every
//! part of that sentence is deliberate. A reader signing out is already
//! leaving; an RP that takes four seconds to answer must not be the reason
//! their sign-out hangs. One attempt and one retry, bounded by a short
//! timeout, and then it is dropped: back-channel logout is a courtesy the
//! specification calls best-effort, and the *authoritative* answer is that the
//! token no longer works — which is already true, in the transaction that
//! ended the session.
//!
//! The clients to tell ride on the event ([`rn_kernel::Event::ended_session`])
//! because the batch that ends a session takes its tokens with it: by the time
//! this runs, there is nothing left to ask.

use std::time::Duration;

use rn_kernel::ids::{Id, OidcClient, Person, Session};
use rn_kernel::oidc::{self, LOGOUT_EVENT, client as registry, jwt, key, subject};
use rn_kernel::{Event, Timestamp};

use crate::state::AppState;

/// How long a logout token is good for. It is delivered immediately or not at
/// all, so this is a bound on clock skew rather than on anything else.
const TTL: Timestamp = 120;

/// How long one POST gets.
const ATTEMPT: Duration = Duration::from_secs(4);

/// How many times each RP is tried. One, and one retry — a third attempt is a
/// queue, and a queue is rung 7's.
const ATTEMPTS: usize = 2;

/// Send the logout tokens an event calls for, on a task of their own.
pub fn spawn(state: &AppState, event: &Event) {
    if let Some((session, clients)) = event.ended_session() {
        let (state, clients) = (state.clone(), clients.to_vec());
        tokio::spawn(async move { notify(state, Some(session), None, clients).await });
    } else if let Some((party, clients)) = event.disabled_party() {
        // A disable ends every session of every registration at once, so there
        // is no single `sid` to name: the logout tokens carry `sub` alone,
        // which is exactly "this subject, wherever they are".
        let (state, clients) = (state.clone(), clients.to_vec());
        tokio::spawn(async move { notify(state, None, Some(party), clients).await });
    }
}

/// POST one logout token to each client that registered an endpoint.
async fn notify(
    state: AppState,
    session: Option<Id<Session>>,
    person: Option<Id<Person>>,
    clients: Vec<Id<OidcClient>>,
) {
    if clients.is_empty() {
        return;
    }
    let reads = state.store.reads();
    let Ok(signing) = key::active(&reads, state.provider.seal()).await else {
        tracing::warn!("no signing key: back-channel logout tokens were not sent");
        return;
    };
    let sid = session.map(|session| session.public(state.ids()).to_string());

    for client_id in clients {
        let Ok(Some(client)) = registry::load(&reads, client_id).await else {
            continue;
        };
        let Some(endpoint) = client.metadata.backchannel_logout_uri.clone() else {
            continue;
        };
        // The `sub` is per sector, so it is the one *this* client knows.
        let sub = match person {
            Some(person) => subject::existing(&reads, client.sector(), person)
                .await
                .ok()
                .flatten()
                .map(|row| row.sub),
            None => None,
        };
        if sid.is_none() && sub.is_none() {
            // Back-Channel Logout §2.4: a token must carry `sub` or `sid`.
            continue;
        }
        let audience = client.id.public(state.ids()).to_string();
        let Ok(token) = logout_token(
            &signing,
            state.provider.issuer(),
            &audience,
            sid.as_deref(),
            sub.as_deref(),
            state.store.clock().now(),
        ) else {
            continue;
        };
        deliver(&endpoint, &token).await;
    }
}

/// The logout token (Back-Channel Logout §2.4).
///
/// `nonce` MUST NOT be present, and that is not a style note: an RP validating
/// this as if it were an id token is exactly the confusion the prohibition
/// exists to prevent, and the `typ` header says so too.
fn logout_token(
    signing: &oidc::SigningKey,
    issuer: &str,
    audience: &str,
    sid: Option<&str>,
    sub: Option<&str>,
    now: Timestamp,
) -> rn_kernel::Outcome<String> {
    let mut jti = [0u8; 16];
    getrandom::fill(&mut jti).ok();
    let mut claims = serde_json::json!({
        "iss": issuer,
        "aud": audience,
        "iat": now,
        "exp": now + TTL,
        "jti": hex(&jti),
        "events": { LOGOUT_EVENT: {} },
    });
    let fields = claims.as_object_mut().expect("an object");
    if let Some(sid) = sid {
        fields.insert("sid".to_owned(), sid.into());
    }
    if let Some(sub) = sub {
        fields.insert("sub".to_owned(), sub.into());
    }
    jwt::sign(&signing.private, &signing.kid, jwt::TYP_LOGOUT, &claims)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// One POST, with one retry.
async fn deliver(endpoint: &str, token: &str) {
    for attempt in 1..=ATTEMPTS {
        match tokio::time::timeout(ATTEMPT, post(endpoint, token)).await {
            Ok(Ok(())) => return,
            Ok(Err(error)) => {
                tracing::debug!(endpoint, attempt, %error, "a logout token was not accepted");
            }
            Err(_) => tracing::debug!(endpoint, attempt, "a logout POST timed out"),
        }
    }
    tracing::info!(endpoint, "a relying party was not told about a sign-out");
}

/// The POST itself, written against a raw socket.
///
/// There is no HTTP *client* in this workspace, and adding one for a single
/// `application/x-www-form-urlencoded` POST to a registered URI would be a
/// dependency for one request shape. What this needs is the shape Back-Channel
/// Logout §2.5 specifies and nothing else: one form field, and a status.
async fn post(endpoint: &str, token: &str) -> std::io::Result<()> {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let (host, port, path, tls) = parts(endpoint).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "not an http(s) URI")
    })?;
    if tls {
        // A registered `https` endpoint needs a TLS client this workspace does
        // not have. Saying so is the honest answer; `docs/oidc.md` carries it
        // as the one declared gap in back-channel delivery.
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "https back-channel endpoints are not delivered to yet",
        ));
    }
    let body = format!("logout_token={}", super::url::encode(token));
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: \
         application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut socket = tokio::net::TcpStream::connect((host.as_str(), port)).await?;
    socket.write_all(request.as_bytes()).await?;
    let mut response = Vec::with_capacity(256);
    socket.read_to_end(&mut response).await?;
    let status = String::from_utf8_lossy(&response);
    let ok = status.starts_with("HTTP/1.1 2") || status.starts_with("HTTP/1.0 2");
    if ok {
        Ok(())
    } else {
        Err(std::io::Error::other(
            status.lines().next().unwrap_or("no status line").to_owned(),
        ))
    }
}

/// A URI, split into what a socket needs.
fn parts(uri: &str) -> Option<(String, u16, String, bool)> {
    let (tls, rest) = match uri.split_once("://") {
        Some(("http", rest)) => (false, rest),
        Some(("https", rest)) => (true, rest),
        _ => return None,
    };
    let (authority, path) = rest
        .split_once('/')
        .map_or((rest, "/".to_owned()), |(a, p)| (a, format!("/{p}")));
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() => {
            (host.to_owned(), port.parse().ok()?)
        }
        _ => (authority.to_owned(), if tls { 443 } else { 80 }),
    };
    Some((host, port, path, tls))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uri_splits_into_what_a_socket_needs() {
        assert_eq!(
            parts("http://127.0.0.1:4180/oauth2/sign_out"),
            Some((
                "127.0.0.1".to_owned(),
                4180,
                "/oauth2/sign_out".to_owned(),
                false
            ))
        );
        assert_eq!(
            parts("http://app.test/logout"),
            Some(("app.test".to_owned(), 80, "/logout".to_owned(), false))
        );
        assert_eq!(
            parts("https://app.test/logout"),
            Some(("app.test".to_owned(), 443, "/logout".to_owned(), true))
        );
        assert_eq!(
            parts("http://app.test"),
            Some(("app.test".to_owned(), 80, "/".to_owned(), false))
        );
        assert!(parts("ftp://app.test/").is_none());
        assert!(parts("app.test/logout").is_none());
    }

    #[test]
    fn a_logout_token_says_what_the_specification_says_and_never_a_nonce() {
        let private = rsa::RsaPrivateKey::new(&mut rand_core::OsRng, 1024).expect("a key");
        let signing = oidc::SigningKey {
            kid: "k1".to_owned(),
            private,
        };
        let token = logout_token(
            &signing,
            "https://example.test",
            "c_one",
            Some("s_abc"),
            Some("a-subject"),
            1_800_000_000,
        )
        .expect("it signs");
        let parts = jwt::split(&token).expect("three segments");
        assert_eq!(parts.header["typ"], jwt::TYP_LOGOUT);
        assert_eq!(parts.claims["iss"], "https://example.test");
        assert_eq!(parts.claims["aud"], "c_one");
        assert_eq!(parts.claims["sid"], "s_abc");
        assert_eq!(parts.claims["sub"], "a-subject");
        assert!(parts.claims.get("jti").is_some());
        assert_eq!(parts.claims["exp"], 1_800_000_000_i64 + TTL);
        assert!(parts.claims["events"].get(LOGOUT_EVENT).is_some());
        assert!(
            parts.claims.get("nonce").is_none(),
            "Back-Channel Logout §2.4: a nonce MUST NOT be present"
        );
    }
}
