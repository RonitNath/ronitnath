//! An in-repo relying party, doing what a relying party does.
//!
//! Discovery, authorize, code, token, id-token validation against the
//! published JWKS, userinfo, refresh, revoke, and sign-out with a back-channel
//! receipt. Every step goes through the HTTP surface, because what is being
//! proved is that a real RP can complete the flow — not that the functions
//! behind it return what they say.

use axum::http::StatusCode;
use rn_api::oidc::Scope;
use rn_kernel::oidc::{jwt, key};
use serde_json::Value;

use super::{
    CHALLENGE, VERIFIER, basic, ensure_key, metadata, parameter, register_client, server, state,
};
use crate::harness;

const REDIRECT: &str = "https://rp.example.test/callback";

/// One complete round trip, asserted step by step.
///
/// One test rather than nine, because the steps are not independent: the code
/// exists because the authorization happened, and the token exists because the
/// code was redeemed. Nine tests would each have to re-run the eight before it.
#[test]
fn a_relying_party_signs_somebody_in_and_learns_exactly_what_it_asked_for() {
    harness::run(async {
        let state = state().await;
        let server = server(&state);
        let operator = harness::register(&state, "Operator", "rp-operator@example.test").await;
        harness::operate_platform(&state, operator.person).await;
        ensure_key(&state, &operator).await;
        let client =
            register_client(&state, &operator, metadata("A relying party", REDIRECT)).await;
        let person = harness::register(&state, "Reader", "rp-reader@example.test").await;

        // --- discovery ----------------------------------------------------
        let discovery = server.get("/.well-known/openid-configuration").await;
        discovery.assert_status_ok();
        let metadata: Value = discovery.json();
        assert_eq!(metadata["issuer"], harness::PUBLIC_ORIGIN);
        assert_eq!(
            metadata["authorization_endpoint"],
            format!("{}/oidc/authorize", harness::PUBLIC_ORIGIN)
        );
        assert_eq!(metadata["subject_types_supported"][0], "pairwise");
        assert_eq!(metadata["code_challenge_methods_supported"][0], "S256");
        assert_eq!(
            metadata["authorization_response_iss_parameter_supported"],
            true
        );
        assert_eq!(metadata["backchannel_logout_supported"], true);
        assert_eq!(metadata["request_parameter_supported"], false);
        // RFC 8414's path serves the same document, because it describes the
        // same server.
        let oauth: Value = server
            .get("/.well-known/oauth-authorization-server")
            .await
            .json();
        assert_eq!(oauth, metadata);

        // --- the keys -----------------------------------------------------
        let jwks: Value = server.get("/oidc/jwks").await.json();
        let published = jwks["keys"].as_array().expect("an array");
        assert!(
            !published.is_empty(),
            "a deployment with no key signs nothing"
        );
        assert!(
            published.iter().all(|key| key.get("d").is_none()),
            "a JWKS is public halves"
        );

        // --- authorize ----------------------------------------------------
        let authorize = format!(
            "/oidc/authorize?response_type=code&client_id={}&redirect_uri={}\
             &scope=openid%20profile%20email%20offline_access&state=st-1&nonce=n-1\
             &code_challenge={CHALLENGE}&code_challenge_method=S256",
            client.id,
            super::encode(&client.redirect_uri),
        );
        // The first visit is the consent page: this client is not trusted and
        // nobody has agreed to anything yet.
        let asked = server
            .get(&authorize)
            .add_header("cookie", format!("rn_session={}", person.token))
            .await;
        asked.assert_status_ok();
        assert!(
            asked.text().contains("A relying party"),
            "the consent page names the client asking"
        );
        assert!(asked.text().contains("Your email address"));

        let allowed = server
            .post("/oidc/authorize")
            .add_header("cookie", format!("rn_session={}", person.token))
            .add_header("sec-fetch-site", "same-origin")
            .form(&consent_form(&client.id, &client.redirect_uri))
            .await;
        assert_eq!(allowed.status_code(), StatusCode::SEE_OTHER);
        let location = allowed
            .headers()
            .get("location")
            .expect("a redirect")
            .to_str()
            .expect("ascii")
            .to_owned();
        assert!(location.starts_with(REDIRECT), "{location}");
        let code = parameter(&location, "code").expect("a code");
        assert_eq!(parameter(&location, "state").as_deref(), Some("st-1"));
        assert_eq!(
            parameter(&location, "iss").as_deref(),
            Some(harness::PUBLIC_ORIGIN),
            "RFC 9207: the response says which issuer answered"
        );

        // --- token --------------------------------------------------------
        let tokens = server
            .post("/oidc/token")
            .add_header("authorization", basic(&client))
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", client.redirect_uri.as_str()),
                ("code_verifier", VERIFIER),
            ])
            .await;
        tokens.assert_status_ok();
        assert_eq!(
            tokens.headers()["cache-control"],
            "no-store",
            "RFC 6749 §5.1"
        );
        assert_eq!(tokens.headers()["pragma"], "no-cache");
        let body: Value = tokens.json();
        assert_eq!(body["token_type"], "Bearer");
        let access = body["access_token"]
            .as_str()
            .expect("an access token")
            .to_owned();
        let refresh = body["refresh_token"]
            .as_str()
            .expect("a refresh token")
            .to_owned();
        let id_token = body["id_token"].as_str().expect("an id token").to_owned();
        assert!(
            body["scope"].as_str().expect("a scope").contains("openid"),
            "the granted scope is stated"
        );

        // --- the id token, validated the way an RP validates one -----------
        let parts = jwt::split(&id_token).expect("three segments");
        let kid = parts.kid().expect("a kid");
        let published = published
            .iter()
            .find(|key| key["kid"] == kid)
            .expect("the kid is in the JWKS the RP fetched");
        let row = key::KeyRow {
            id: rn_kernel::Id::new(1),
            kid: kid.to_owned(),
            status: "active".to_owned(),
            modulus: published["n"].as_str().expect("n").to_owned(),
            exponent: published["e"].as_str().expect("e").to_owned(),
            created_at: 0,
        };
        let public = key::public_of(&row).expect("a public key");
        assert!(
            jwt::verify(&public, &parts),
            "the id token verifies under the key the JWKS published"
        );
        assert_eq!(parts.claims["iss"], harness::PUBLIC_ORIGIN);
        assert_eq!(parts.claims["aud"], client.id);
        assert_eq!(parts.claims["azp"], client.id);
        assert_eq!(parts.claims["nonce"], "n-1");
        assert!(parts.claims["sid"].as_str().is_some(), "a session id");
        assert!(parts.claims["auth_time"].as_i64().is_some());
        let sub = parts.claims["sub"].as_str().expect("a sub").to_owned();

        // The `sub` is not any id of anything, which is the whole ruling.
        let key = state.ids();
        assert_ne!(sub, person.person.public(key).to_string());
        assert_ne!(sub, person.identity.public(key).to_string());
        assert!(!sub.starts_with("p_") && !sub.starts_with("i_"));

        // --- userinfo -----------------------------------------------------
        let claims: Value = server
            .get("/oidc/userinfo")
            .add_header("authorization", format!("Bearer {access}"))
            .await
            .json();
        assert_eq!(claims["sub"], sub, "the same subject the id token named");
        assert_eq!(claims["name"], "Reader");
        assert_eq!(claims["preferred_username"], "rp-reader-example-test");
        assert_eq!(claims["email"], "rp-reader@example.test");
        assert_eq!(
            claims["email_verified"], false,
            "nobody proved that address"
        );
        assert!(
            claims.get("groups").is_none(),
            "groups was not consented to, so it is not carried"
        );

        // --- refresh ------------------------------------------------------
        let refreshed = server
            .post("/oidc/token")
            .add_header("authorization", basic(&client))
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh.as_str()),
            ])
            .await;
        refreshed.assert_status_ok();
        let rotated: Value = refreshed.json();
        let second_access = rotated["access_token"]
            .as_str()
            .expect("a token")
            .to_owned();
        let second_refresh = rotated["refresh_token"]
            .as_str()
            .expect("a token")
            .to_owned();
        assert_ne!(second_refresh, refresh, "a refresh rotates");
        assert_ne!(second_access, access);
        // The id token a refresh mints names the same person to the same
        // client: a `sub` that rotated would make one person two.
        let refreshed_id =
            jwt::split(rotated["id_token"].as_str().expect("an id token")).expect("three segments");
        assert_eq!(refreshed_id.claims["sub"], sub);

        // The token the code produced is *not* revoked by a refresh — it has
        // its own hour — but the refresh token it came with is.
        let reused = server
            .post("/oidc/token")
            .add_header("authorization", basic(&client))
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh.as_str()),
            ])
            .await;
        assert_eq!(reused.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(reused.json::<Value>()["error"], "invalid_grant");
        // And reuse took the family: the token that replaced it is dead too.
        let after_reuse = server
            .post("/oidc/token")
            .add_header("authorization", basic(&client))
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", second_refresh.as_str()),
            ])
            .await;
        assert_eq!(
            after_reuse.status_code(),
            StatusCode::BAD_REQUEST,
            "RFC 9700 §4.14.2: a replayed refresh token revokes its family"
        );

        // --- revoke -------------------------------------------------------
        let revoked = server
            .post("/oidc/revoke")
            .add_header("authorization", basic(&client))
            .form(&[("token", second_access.as_str())])
            .await;
        revoked.assert_status_ok();
        let after = server
            .get("/oidc/userinfo")
            .add_header("authorization", format!("Bearer {second_access}"))
            .await;
        assert_eq!(after.status_code(), StatusCode::UNAUTHORIZED);
        // RFC 7009 §2.2: a token this deployment never minted is a success,
        // because the endpoint is not an oracle for which tokens exist.
        server
            .post("/oidc/revoke")
            .add_header("authorization", basic(&client))
            .form(&[("token", "never-minted-anywhere")])
            .await
            .assert_status_ok();

        // --- consent, remembered ------------------------------------------
        // The second authorization does not ask again: the person agreed, and
        // the grant is a relation row `check()` can see.
        let again = server
            .get(&authorize)
            .add_header("cookie", format!("rn_session={}", person.token))
            .await;
        assert_eq!(
            again.status_code(),
            StatusCode::SEE_OTHER,
            "an agreed client goes straight through"
        );

        // --- sign out, and the token dies with the session -----------------
        let live_access = {
            let location = again.headers()["location"].to_str().expect("ascii");
            let code = parameter(location, "code").expect("a code");
            let body: Value = server
                .post("/oidc/token")
                .add_header("authorization", basic(&client))
                .form(&[
                    ("grant_type", "authorization_code"),
                    ("code", code.as_str()),
                    ("redirect_uri", client.redirect_uri.as_str()),
                    ("code_verifier", VERIFIER),
                ])
                .await
                .json();
            body["access_token"].as_str().expect("a token").to_owned()
        };
        server
            .get("/oidc/userinfo")
            .add_header("authorization", format!("Bearer {live_access}"))
            .await
            .assert_status_ok();

        let out = server
            .post("/auth/sign-out")
            .add_header("cookie", format!("rn_session={}", person.token))
            .add_header("sec-fetch-site", "same-origin")
            .form(&[("next", "/")])
            .await;
        assert_eq!(out.status_code(), StatusCode::SEE_OTHER);
        let dead = server
            .get("/oidc/userinfo")
            .add_header("authorization", format!("Bearer {live_access}"))
            .await;
        assert_eq!(
            dead.status_code(),
            StatusCode::UNAUTHORIZED,
            "ending a session ends every token it minted"
        );
        assert!(
            dead.headers()["www-authenticate"]
                .to_str()
                .expect("ascii")
                .contains("invalid_token")
        );
    });
}

/// A signing key that has been rotated keeps verifying while it is retiring,
/// and stops when it is retired.
#[test]
fn a_rotated_key_verifies_through_its_overlap_and_not_after_it() {
    harness::run(async {
        let state = state().await;
        let server = server(&state);
        let operator = harness::register(&state, "Rotator", "rotate-op@example.test").await;
        harness::operate_platform(&state, operator.person).await;
        ensure_key(&state, &operator).await;

        let before: Vec<String> = kids(&server).await;
        assert!(!before.is_empty());
        rn_kernel::cmd::rotate_signing_key(
            &harness::ctx(&state, super::principal(&state, &operator).await),
            &rn_api::commands::RotateSigningKey {},
        )
        .await
        .expect("a rotation");

        let after = kids(&server).await;
        assert!(
            after.len() > before.len(),
            "the old key is still published while it retires"
        );
        for old in &before {
            assert!(after.contains(old), "{old} stopped verifying too early");
        }

        // Retire it for good, and it leaves the document.
        let retiring = before.first().expect("a key").clone();
        state
            .store
            .execute(
                key::RETIRE_SQL,
                rn_kernel::bind![1_800_000_000_i64, retiring.as_str()],
            )
            .await
            .expect("the retirement");
        let finally = kids(&server).await;
        assert!(
            !finally.contains(&retiring),
            "a retired key is not published"
        );
    });
}

async fn kids(server: &axum_test::TestServer) -> Vec<String> {
    let jwks: Value = server.get("/oidc/jwks").await.json();
    jwks["keys"]
        .as_array()
        .expect("an array")
        .iter()
        .filter_map(|key| key["kid"].as_str().map(str::to_owned))
        .collect()
}

/// A person's `sub` survives a merge, from either direction.
#[test]
fn both_subjects_resolve_to_the_survivor_after_a_merge() {
    harness::run(async {
        let state = state().await;
        let reads = state.store.reads();
        let sector = 0; // the platform's own clients

        // Two people, two subjects at one sector.
        let one = harness::register(&state, "One", "merge-sub-one@example.test").await;
        let two = harness::register(&state, "Two", "merge-sub-two@example.test").await;
        let now = state.store.clock().now();
        let mut subs = Vec::new();
        for person in [one.person, two.person] {
            let (sub, stmt) = rn_kernel::oidc::subject::resolve(&reads, sector, person, now)
                .await
                .expect("a subject");
            if let Some(stmt) = stmt {
                state
                    .store
                    .execute(rn_kernel::oidc::subject::INSERT_SQL, stmt.params)
                    .await
                    .expect("the row");
            }
            subs.push(sub);
        }
        assert_ne!(subs[0], subs[1], "two people are two subjects");

        // Merge the second into the first, the way `person_alias` records it.
        state
            .store
            .execute(
                "INSERT INTO person_alias (old_person_id, person_id, at) VALUES ($1, $2, $3)",
                rn_kernel::bind![two.person, one.person, now],
            )
            .await
            .expect("the alias");

        for sub in &subs {
            let resolved = rn_kernel::oidc::subject::person_of(&reads, sub)
                .await
                .expect("the query runs");
            assert_eq!(
                resolved,
                Some(one.person),
                "{sub} stopped resolving to the survivor"
            );
        }
    });
}

/// The consent form, as the page posts it back.
fn consent_form(client_id: &str, redirect_uri: &str) -> Vec<(String, String)> {
    vec![
        ("decision".into(), "allow".into()),
        ("response_type".into(), "code".into()),
        ("client_id".into(), client_id.into()),
        ("redirect_uri".into(), redirect_uri.into()),
        (
            "scope".into(),
            Scope::join(&[
                Scope::Openid,
                Scope::Profile,
                Scope::Email,
                Scope::OfflineAccess,
            ]),
        ),
        ("state".into(), "st-1".into()),
        ("nonce".into(), "n-1".into()),
        ("code_challenge".into(), CHALLENGE.into()),
        ("code_challenge_method".into(), "S256".into()),
    ]
}

/// RP-initiated logout, end to end, with the back-channel receipt.
///
/// The whole point of back-channel logout is that the relying party is *told*
/// rather than left to find out, so the assertion is on what arrived at the
/// RP's socket: a `logout_token` carrying the session it names and no `nonce`.
#[test]
fn signing_out_at_a_relying_party_ends_the_session_here_and_tells_the_others() {
    harness::run(async {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let state = state().await;
        let server = server(&state);
        let operator = harness::register(&state, "Op", "bc-op@example.test").await;
        harness::operate_platform(&state, operator.person).await;
        ensure_key(&state, &operator).await;

        // A socket standing in for the relying party's logout endpoint.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback socket");
        let port = listener.local_addr().expect("an address").port();
        let received = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("a connection");
            let mut buffer = vec![0u8; 8192];
            let read = socket.read(&mut buffer).await.expect("the request");
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .await
                .ok();
            String::from_utf8_lossy(&buffer[..read]).into_owned()
        });

        let mut registration = metadata("Back channel", "https://bc.example.test/cb");
        registration.backchannel_logout_uri = Some(format!("http://127.0.0.1:{port}/logout"));
        registration.post_logout_redirect_uris = vec!["https://bc.example.test/bye".to_owned()];
        let client = register_client(&state, &operator, registration).await;
        let person = harness::register(&state, "Leaver", "bc-leaver@example.test").await;
        let cookie = format!("rn_session={}", person.token);

        // Sign in at the RP.
        let allowed = server
            .post("/oidc/authorize")
            .add_header("cookie", cookie.clone())
            .add_header("sec-fetch-site", "same-origin")
            .form(&[
                ("decision", "allow"),
                ("response_type", "code"),
                ("client_id", client.id.as_str()),
                ("redirect_uri", client.redirect_uri.as_str()),
                ("scope", "openid"),
                ("code_challenge", CHALLENGE),
                ("code_challenge_method", "S256"),
            ])
            .await;
        let location = allowed.headers()["location"].to_str().expect("ascii");
        let code = parameter(location, "code").expect("a code");
        let issued: Value = server
            .post("/oidc/token")
            .add_header("authorization", basic(&client))
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", client.redirect_uri.as_str()),
                ("code_verifier", VERIFIER),
            ])
            .await
            .json();
        let id_token = issued["id_token"].as_str().expect("an id token").to_owned();
        let access = issued["access_token"].as_str().expect("a token").to_owned();
        let sid = jwt::split(&id_token).expect("segments").claims["sid"]
            .as_str()
            .expect("a session id")
            .to_owned();

        // The RP sends them here to sign out. The hint names this session, so
        // there is nothing to confirm.
        let out = server
            .get(&format!(
                "/oidc/end_session?id_token_hint={}&post_logout_redirect_uri={}&state=bye",
                super::encode(&id_token),
                super::encode("https://bc.example.test/bye"),
            ))
            .add_header("cookie", cookie.clone())
            .await;
        assert_eq!(out.status_code(), StatusCode::SEE_OTHER);
        let landing = out.headers()["location"].to_str().expect("ascii");
        assert!(
            landing.starts_with("https://bc.example.test/bye"),
            "{landing}"
        );
        assert_eq!(parameter(landing, "state").as_deref(), Some("bye"));

        // The session is gone, and so is the token it minted.
        let dead = server
            .get("/oidc/userinfo")
            .add_header("authorization", format!("Bearer {access}"))
            .await;
        assert_eq!(dead.status_code(), StatusCode::UNAUTHORIZED);

        // And the relying party was told.
        let request = tokio::time::timeout(std::time::Duration::from_secs(10), received)
            .await
            .expect("the relying party was told within ten seconds")
            .expect("the receiving task");
        assert!(request.starts_with("POST /logout "), "{request}");
        assert!(
            request.contains("application/x-www-form-urlencoded"),
            "{request}"
        );
        let body = request.split("\r\n\r\n").nth(1).expect("a body");
        let token = body
            .strip_prefix("logout_token=")
            .expect("the one form field")
            .to_owned();
        let logout = jwt::split(&super::decode_value(&token)).expect("three segments");
        assert_eq!(logout.header["typ"], "logout+jwt");
        assert_eq!(logout.claims["iss"], harness::PUBLIC_ORIGIN);
        assert_eq!(logout.claims["aud"], client.id);
        assert_eq!(logout.claims["sid"], sid, "the session that ended");
        assert!(logout.claims["jti"].as_str().is_some());
        assert!(
            logout.claims["events"]
                .get("http://schemas.openid.net/event/backchannel-logout")
                .is_some()
        );
        assert!(
            logout.claims.get("nonce").is_none(),
            "Back-Channel Logout §2.4: a nonce MUST NOT be present"
        );
    });
}

/// A sign-out request with no hint is asked about rather than obeyed.
#[test]
fn a_sign_out_nobody_proved_they_asked_for_is_confirmed_first() {
    harness::run(async {
        let state = state().await;
        let server = server(&state);
        let person = harness::register(&state, "Reader", "confirm-me@example.test").await;
        let cookie = format!("rn_session={}", person.token);

        let asked = server
            .get("/oidc/end_session")
            .add_header("cookie", cookie.clone())
            .await;
        asked.assert_status_ok();
        assert!(asked.text().contains("Sign out"), "a button, not an action");

        // And the session is still live afterwards.
        server
            .get("/api/whoami")
            .add_header("cookie", cookie.clone())
            .await
            .assert_status_ok();

        // The button is what ends it.
        let done = server
            .post("/oidc/end_session")
            .add_header("cookie", cookie.clone())
            .add_header("sec-fetch-site", "same-origin")
            .form(&[("confirmed", "1")])
            .await;
        assert_eq!(done.status_code(), StatusCode::SEE_OTHER);
        let gone = server.get("/api/whoami").add_header("cookie", cookie).await;
        assert_eq!(gone.status_code(), StatusCode::FORBIDDEN);
    });
}
