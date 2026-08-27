//! What the API is allowed to say, and what it must never say.
//!
//! The route matrix (`surface.rs`) proves who may reach what. These prove what
//! comes back: that `whoami` carries no address and no internal id, that the
//! queries carry no factor secrets, that a command replies with the offset its
//! event landed at — and that a password typed into the sign-in form does not
//! turn up in a log line on the way through.

mod harness;

use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use serde_json::Value;
use tokio::sync::OnceCell;

static NODE: OnceCell<rn_site::AppState> = OnceCell::const_new();

async fn state() -> rn_site::AppState {
    harness::node(&NODE, "127.0.0.1:8197", "127.0.0.1:8297").await
}

/// Everything this binary's process logged, so a test can read it back.
///
/// The subscriber is installed once for the whole binary — a global default
/// can only be set once — which is why the canary test asserts over
/// *everything* logged rather than only its own span. That is the stronger
/// claim anyway: the sentinel must not appear in any line at all.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("the log buffer").clone()).into_owned()
    }
}

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("the log buffer")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn logs() -> Captured {
    static CAPTURED: std::sync::OnceLock<Captured> = std::sync::OnceLock::new();
    CAPTURED
        .get_or_init(|| {
            let captured = Captured::default();
            let writer = captured.clone();
            // Everything, down to trace: a canary that only checked `info`
            // would pass a build that logged the password at `debug`.
            let _ = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::TRACE)
                .with_writer(move || writer.clone())
                .try_init();
            captured
        })
        .clone()
}

#[test]
fn whoami_leaks_nothing() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let somebody = harness::register(&state, "Leak Check", "whoami-leak@example.invalid").await;
        harness::operate_organization(&state, somebody.person, "Leak Check Organization").await;
        harness::operate_platform(&state, somebody.person).await;

        let response = server
            .get("/api/whoami")
            .add_header("cookie", format!("rn_session={}", somebody.token))
            .await;
        response.assert_status_ok();
        let text = response.text();

        assert!(
            !text.contains('@'),
            "whoami carried something address-shaped: {text}"
        );
        assert!(
            !text.contains(&somebody.token),
            "whoami echoed the session token"
        );

        // Every number in the document, by the path it sits at. The session's
        // expiry is the only one there is: an internal id reaching the wire
        // would be a number somewhere else, whatever it was called.
        let body: Value = serde_json::from_str(&text).expect("whoami is JSON");
        let mut numbers = Vec::new();
        collect_numbers(&body, String::new(), &mut numbers);
        assert_eq!(
            numbers,
            vec!["session_expires_at".to_owned()],
            "whoami carries a number that is not the session expiry: {numbers:?}"
        );

        // And the ids it does carry are derived, prefixed and of the right kind.
        assert!(
            body["identity"]["public_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("i_") && id.len() == 24)
        );
        assert!(
            body["person"]["public_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("p_"))
        );
        assert!(
            body["organizations"][0]["public_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("o_"))
        );
        assert_eq!(
            body["tiers"],
            serde_json::json!(["member", "org", "platform"])
        );
    });
}

/// Every numeric leaf's path, so a test can name what it found.
fn collect_numbers(value: &Value, path: String, into: &mut Vec<String>) {
    match value {
        Value::Number(_) => into.push(path),
        Value::Object(fields) => {
            for (name, child) in fields {
                collect_numbers(child, name.clone(), into);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_numbers(item, path.clone(), into);
            }
        }
        _ => {}
    }
}

#[test]
fn the_queries_carry_a_persons_own_rows_and_none_of_their_secrets() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let mine = harness::register(&state, "Mine", "queries-mine@example.invalid").await;
        let theirs = harness::register(&state, "Theirs", "queries-theirs@example.invalid").await;
        let cookie = format!("rn_session={}", mine.token);

        let sessions = server
            .get("/api/q/sessions")
            .add_header("cookie", cookie.clone())
            .await;
        sessions.assert_status_ok();
        assert_eq!(
            sessions
                .headers()
                .get("cache-control")
                .and_then(|value| value.to_str().ok()),
            Some("private, no-store")
        );
        let rows: Vec<Value> = sessions.json();
        assert_eq!(rows.len(), 1, "one device, one row");
        assert_eq!(rows[0]["current"], true);
        assert!(
            rows[0]["public_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("s_"))
        );
        assert!(
            !sessions.text().contains(&mine.token),
            "a device list must not hand back the secret that authenticates it"
        );

        let identities = server
            .get("/api/q/identities")
            .add_header("cookie", cookie.clone())
            .await;
        identities.assert_status_ok();
        let text = identities.text();
        assert!(
            !text.contains('@'),
            "the identities query carried an address: {text}"
        );
        assert!(
            !text.contains("$argon2"),
            "the identities query carried a password hash"
        );
        let rows: Vec<Value> = identities.json();
        assert_eq!(rows.len(), 1, "one registration, not everybody's");
        let kinds: Vec<&str> = rows[0]["factors"]
            .as_array()
            .expect("factors")
            .iter()
            .filter_map(|factor| factor["kind"].as_str())
            .collect();
        assert_eq!(kinds, vec!["email", "password"]);
        assert_eq!(rows[0]["factors"][0]["verified"], false);

        // Somebody else's rows are not in anybody's result set.
        let theirs_sessions = server
            .get("/api/q/sessions")
            .add_header("cookie", format!("rn_session={}", theirs.token))
            .await;
        let theirs_rows: Vec<Value> = theirs_sessions.json();
        assert_ne!(theirs_rows[0]["public_id"], rows[0]["public_id"]);

        // A name no query serves is refused, not guessed at.
        for absent in ["people", "everything", "..%2fwhoami"] {
            let response = server
                .get(&format!("/api/q/{absent}"))
                .add_header("cookie", cookie.clone())
                .await;
            assert_eq!(
                response.status_code(),
                StatusCode::NOT_FOUND,
                "/api/q/{absent} answered"
            );
        }
    });
}

#[test]
fn the_audit_query_pages_forward_by_offset() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let somebody = harness::register(&state, "Audited", "audit-page@example.invalid").await;
        let cookie = format!("rn_session={}", somebody.token);

        let first: Vec<Value> = server
            .get("/api/q/audit")
            .add_header("cookie", cookie.clone())
            .await
            .json();
        assert!(!first.is_empty(), "a registration is an audited command");
        assert_eq!(first[0]["command"], "register");
        let offset = first[0]["offset"].as_u64().expect("an offset");

        let after: Vec<Value> = server
            .get(&format!("/api/q/audit?after={offset}"))
            .add_header("cookie", cookie.clone())
            .await
            .json();
        assert!(
            after
                .iter()
                .all(|row| row["offset"].as_u64() > Some(offset)),
            "a page after an offset carried a row at or before it"
        );

        let page: Vec<Value> = server
            .get("/api/q/audit?limit=1")
            .add_header("cookie", cookie)
            .await
            .json();
        assert_eq!(page.len(), 1);
    });
}

#[test]
fn a_command_replies_with_the_offset_its_event_landed_at() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let somebody =
            harness::register(&state, "Commander", "command-reply@example.invalid").await;
        let cookie = format!("rn_session={}", somebody.token);

        let body = r#"{"key":"9c1e5a70-0000-4000-8000-000000000010",
                       "kind":"email","value":"command-second@example.invalid"}"#;
        let response = server
            .post("/api/cmd/add-factor")
            .add_header("cookie", cookie.clone())
            .add_header("sec-fetch-site", "same-origin")
            .text(body)
            .content_type("application/json")
            .await;
        response.assert_status_ok();
        let reply: Value = response.json();
        assert!(reply["offset"].as_u64().is_some_and(|offset| offset > 0));
        assert!(
            reply["result"]["factor"]
                .as_str()
                .is_some_and(|id| id.starts_with("f_"))
        );

        // The same key and the same body replays the original reply.
        let replayed = server
            .post("/api/cmd/add-factor")
            .add_header("cookie", cookie.clone())
            .add_header("sec-fetch-site", "same-origin")
            .text(body)
            .content_type("application/json")
            .await;
        replayed.assert_status_ok();
        assert_eq!(replayed.json::<Value>(), reply);

        // The same key with a different body is the one thing that is neither
        // a replay nor a fresh command.
        //
        // The bodies differ in `kind` rather than in `value`, and that is not
        // arbitrary: `value` is a credential-bearing field, so it is redacted
        // before the digest is taken (`kernel::audit::REDACTED_FIELDS`) and
        // two calls that differ only in it are — deliberately — a replay. A
        // caller retrying with a corrected password under a key they have
        // already spent gets the first call's answer, which is the safer of
        // the two.
        let conflict = server
            .post("/api/cmd/add-factor")
            .add_header("cookie", cookie)
            .add_header("sec-fetch-site", "same-origin")
            .text(
                r#"{"key":"9c1e5a70-0000-4000-8000-000000000010",
                    "kind":"password","value":"an obviously fake test password"}"#,
            )
            .content_type("application/json")
            .await;
        assert_eq!(conflict.status_code(), StatusCode::CONFLICT);
        assert_eq!(conflict.text(), r#"{"decline":"declined"}"#);
    });
}

#[test]
fn a_name_that_is_not_a_command_is_not_a_command() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let somebody = harness::register(&state, "Early", "unbound-command@example.invalid").await;
        let cookie = format!("rn_session={}", somebody.token);

        // The contract is fully bound, so there is no name in it that answers
        // a decline. `commands.rs` runs every one of them.
        assert!(
            rn_site::api::cmd::unbound().is_empty(),
            "a declared command is not wired: {:?}",
            rn_site::api::cmd::unbound()
        );

        // A name outside it reads like everything else that is not here.
        for nonsense in ["delete-everything", "register-admin", "..%2fwhoami"] {
            let response = server
                .post(&format!("/api/cmd/{nonsense}"))
                .add_header("cookie", cookie.clone())
                .add_header("sec-fetch-site", "same-origin")
                .text(r#"{"key":"9c1e5a70-0000-4000-8000-000000000020"}"#)
                .content_type("application/json")
                .await;
            assert_eq!(
                response.status_code(),
                StatusCode::NOT_FOUND,
                "/api/cmd/{nonsense} answered"
            );
            assert_eq!(response.text(), r#"{"decline":"declined"}"#);
        }
    });
}

#[test]
fn the_auth_page_registers_signs_in_and_signs_out_a_browser() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);

        let page = server.get("/auth?next=%2Forg").await;
        page.assert_status_ok();
        let html = page.text();
        assert!(html.contains(r#"action="/auth/sign-in""#));
        assert!(html.contains(r#"action="/auth/register""#));
        assert!(html.contains(r#"value="/org""#), "the next path is carried");

        // An open redirect never survives the round trip.
        let hostile = server.get("/auth?next=https%3A%2F%2Fevil.example").await;
        assert!(!hostile.text().contains("evil.example"));

        let registered = server
            .post("/auth/register")
            .add_header("sec-fetch-site", "same-origin")
            .text(harness::form(&[
                ("display_name", "Browser"),
                ("email", "auth-browser@example.invalid"),
                ("password", harness::PASSWORD),
                ("next", "/app"),
            ]))
            .content_type("application/x-www-form-urlencoded")
            .await;
        assert_eq!(registered.status_code(), StatusCode::SEE_OTHER);
        assert_eq!(
            registered
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok()),
            Some("/app")
        );
        let cookie = registered
            .headers()
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
            .expect("a session cookie")
            .to_owned();
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        let session = cookie
            .split_once("rn_session=")
            .and_then(|(_, rest)| rest.split_once(';'))
            .map(|(token, _)| token.to_owned())
            .expect("a token in the cookie");

        // That cookie is a member everywhere a member is expected.
        server
            .get("/app")
            .add_header("cookie", format!("rn_session={session}"))
            .await
            .assert_status_ok();

        // Registering the same address again reads exactly like a wrong
        // password: the form must not become a list of who is registered.
        let taken = server
            .post("/auth/register")
            .add_header("sec-fetch-site", "same-origin")
            .text(harness::form(&[
                ("display_name", "Impostor"),
                ("email", "auth-browser@example.invalid"),
                ("password", harness::PASSWORD),
            ]))
            .content_type("application/x-www-form-urlencoded")
            .await;
        assert_eq!(taken.status_code(), StatusCode::FORBIDDEN);
        assert!(!taken.text().contains("taken"));
        assert!(!taken.text().contains("already"));

        let out = server
            .post("/auth/sign-out")
            .add_header("cookie", format!("rn_session={session}"))
            .add_header("sec-fetch-site", "same-origin")
            .text("next=%2F")
            .content_type("application/x-www-form-urlencoded")
            .await;
        assert_eq!(out.status_code(), StatusCode::SEE_OTHER);
        assert!(
            out.headers()
                .get("set-cookie")
                .and_then(|value| value.to_str().ok())
                .is_some_and(|cookie| cookie.contains("Max-Age=0"))
        );

        // And the session is gone, not merely forgotten by the browser.
        let after = server
            .get("/api/whoami")
            .add_header("cookie", format!("rn_session={session}"))
            .await;
        assert_eq!(after.status_code(), StatusCode::FORBIDDEN);
    });
}

#[test]
fn a_validation_failure_is_rendered_beside_the_field_that_failed() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);

        let short = server
            .post("/auth/register")
            .add_header("sec-fetch-site", "same-origin")
            .text(harness::form(&[
                ("display_name", "Short"),
                ("email", "auth-short@example.invalid"),
                ("password", "short"),
            ]))
            .content_type("application/x-www-form-urlencoded")
            .await;
        assert_eq!(short.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        let html = short.text();
        assert!(
            html.contains("a password is at least 12 characters"),
            "{html}"
        );
        // The address is given back so a typo is a correction, and the
        // password never is.
        assert!(html.contains("auth-short@example.invalid"));
        assert!(!html.contains(">short<") && !html.contains("value=\"short\""));
    });
}

#[test]
fn the_claim_page_says_what_the_link_grants_and_the_grant_lands_once() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let somebody = harness::register(&state, "Claimer", "claim-page@example.invalid").await;
        let token = harness::mint_link(&state, somebody.identity).await;
        let cookie = format!("rn_session={}", somebody.token);

        let anonymous = server.get(&format!("/links/{token}")).await;
        anonymous.assert_status_ok();
        assert!(anonymous.text().contains("Confirm an email address"));
        assert!(
            anonymous.text().contains("Sign in to accept"),
            "a reader who is nobody yet is offered the way to become somebody"
        );
        assert!(
            !anonymous.text().contains('@'),
            "the claim page names no address"
        );

        let signed_in = server
            .get(&format!("/links/{token}"))
            .add_header("cookie", cookie.clone())
            .await;
        assert!(signed_in.text().contains("Accept"));

        let claimed = server
            .post(&format!("/links/{token}/claim"))
            .add_header("cookie", cookie.clone())
            .add_header("sec-fetch-site", "same-origin")
            .text("")
            .content_type("application/x-www-form-urlencoded")
            .await;
        assert_eq!(claimed.status_code(), StatusCode::SEE_OTHER);

        // A link is claimed once. Afterwards it is indistinguishable from one
        // that never existed.
        let again = server
            .get(&format!("/links/{token}"))
            .add_header("cookie", cookie.clone())
            .await;
        assert_eq!(again.status_code(), StatusCode::NOT_FOUND);
        assert_eq!(again.text(), r#"{"decline":"declined"}"#);

        // And the factor it proved is now verified.
        let identities: Vec<Value> = server
            .get("/api/q/identities")
            .add_header("cookie", cookie)
            .await
            .json();
        assert_eq!(identities[0]["factors"][0]["verified"], true);
    });
}

#[test]
fn a_secret_never_reaches_a_log_line() {
    let captured = logs();
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);

        // Register with the sentinel as the password, then use it every way a
        // browser can: a good sign-in, a bad one, and a command.
        let registered = server
            .post("/auth/register")
            .add_header("sec-fetch-site", "same-origin")
            .text(harness::form(&[
                ("display_name", "Canary"),
                ("email", "canary@example.invalid"),
                ("password", harness::CANARY),
            ]))
            .content_type("application/x-www-form-urlencoded")
            .await;
        assert_eq!(registered.status_code(), StatusCode::SEE_OTHER);
        let cookie = registered
            .headers()
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
            .expect("a session cookie")
            .split_once("rn_session=")
            .and_then(|(_, rest)| rest.split_once(';'))
            .map(|(token, _)| token.to_owned())
            .expect("a token");

        for password in [harness::CANARY, "the wrong one entirely"] {
            server
                .post("/auth/sign-in")
                .add_header("sec-fetch-site", "same-origin")
                .text(harness::form(&[
                    ("email", "canary@example.invalid"),
                    ("password", password),
                ]))
                .content_type("application/x-www-form-urlencoded")
                .await;
        }
        server
            .get("/api/whoami")
            .add_header("cookie", format!("rn_session={cookie}"))
            .await
            .assert_status_ok();
        server
            .post("/api/cmd/nonsense")
            .add_header("cookie", format!("rn_session={cookie}"))
            .add_header("sec-fetch-site", "same-origin")
            .text(format!(
                r#"{{"key":"bad","password":"{}"}}"#,
                harness::CANARY
            ))
            .content_type("application/json")
            .await;

        let log = captured.text();
        // Four secrets, not two. The password and the session token are the
        // ones a request carries; the id key and the two raft secrets are the
        // ones the *process* holds, and nothing formats them today — which is
        // exactly the claim worth having a test for, since the day something
        // does it will be a `Debug` of a config struct in a hurry.
        for (secret, what) in [
            (harness::CANARY, "the sentinel password"),
            (cookie.as_str(), "the session token"),
            (harness::CANARY_ID_KEY, "the id key"),
            (rn_site::db::cluster::DEV_SECRET_RAFT, "the raft secret"),
            (rn_site::db::cluster::DEV_SECRET_API, "the raft API secret"),
        ] {
            assert!(!log.contains(secret), "{what} reached a log line");
        }
        assert!(
            !log.is_empty(),
            "nothing was captured at all, so the canary proved nothing"
        );
    });
}
