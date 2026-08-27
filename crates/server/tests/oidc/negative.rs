//! The negative space: everything a relying party, a browser or a stranger
//! must *not* be able to do.
//!
//! Each case names the rule it is about. What they have in common is that a
//! refusal here says nothing beyond "no": a code that never existed, a code
//! that belongs to another client and a code with the wrong verifier are one
//! `invalid_grant` with one sentence, because the difference between them is
//! exactly what a caller is measuring.

use axum::http::StatusCode;
use rn_api::oidc::{ClientAuthMethod, GrantType, Scope};
use serde_json::Value;

use super::{
    CHALLENGE, VERIFIER, basic, encode, ensure_key, metadata, parameter, register_client, server,
    state,
};
use crate::harness;

const REDIRECT: &str = "https://neg.example.test/callback";

/// A client, a signing key, an operator and somebody signed in.
struct World {
    state: rn_site::AppState,
    server: axum_test::TestServer,
    operator: harness::Somebody,
    person: harness::Somebody,
    client: super::Client,
}

async fn world(name: &str) -> World {
    let state = state().await;
    let server = server(&state);
    let operator =
        harness::register(&state, "Operator", &format!("neg-op-{name}@example.test")).await;
    harness::operate_platform(&state, operator.person).await;
    ensure_key(&state, &operator).await;
    let client = register_client(&state, &operator, metadata("Negative", REDIRECT)).await;
    let person = harness::register(&state, "Reader", &format!("neg-{name}@example.test")).await;
    World {
        state,
        server,
        operator,
        person,
        client,
    }
}

/// A code, freshly minted for the signed-in person.
async fn code(world: &World) -> String {
    let authorize = format!(
        "/oidc/authorize?response_type=code&client_id={}&redirect_uri={}&scope=openid\
         &code_challenge={CHALLENGE}&code_challenge_method=S256",
        world.client.id,
        encode(&world.client.redirect_uri),
    );
    let allowed = world
        .server
        .post("/oidc/authorize")
        .add_header("cookie", format!("rn_session={}", world.person.token))
        .add_header("sec-fetch-site", "same-origin")
        .form(&[
            ("decision", "allow"),
            ("response_type", "code"),
            ("client_id", world.client.id.as_str()),
            ("redirect_uri", world.client.redirect_uri.as_str()),
            ("scope", "openid"),
            ("code_challenge", CHALLENGE),
            ("code_challenge_method", "S256"),
        ])
        .await;
    let _ = authorize;
    let location = allowed.headers()["location"].to_str().expect("ascii");
    parameter(location, "code").expect("a code")
}

/// The authorization endpoint refuses everything it should, and refuses the
/// two it may not redirect by rendering instead.
#[test]
fn the_authorization_endpoint_never_redirects_to_a_uri_it_has_not_verified() {
    harness::run(async {
        let world = world("authz").await;
        let cookie = format!("rn_session={}", world.person.token);

        // An unknown client: rendered, never redirected.
        let unknown = world
            .server
            .get(
                "/oidc/authorize?response_type=code&client_id=c_AAAAAAAAAAAAAAAAAAAAAA\
                  &redirect_uri=https%3A%2F%2Fevil.test%2Fcb&scope=openid",
            )
            .add_header("cookie", cookie.clone())
            .await;
        assert_eq!(unknown.status_code(), StatusCode::BAD_REQUEST);
        assert!(
            unknown.headers().get("location").is_none(),
            "an unverified redirect_uri is never a Location header"
        );

        // A redirect URI the registration does not name: the same.
        let wrong = world
            .server
            .get(&format!(
                "/oidc/authorize?response_type=code&client_id={}\
                 &redirect_uri=https%3A%2F%2Fevil.test%2Fcb&scope=openid\
                 &code_challenge={CHALLENGE}&code_challenge_method=S256",
                world.client.id
            ))
            .add_header("cookie", cookie.clone())
            .await;
        assert_eq!(wrong.status_code(), StatusCode::BAD_REQUEST);
        assert!(wrong.headers().get("location").is_none());
    });
}

/// Once the client and the redirect are established, every other failure is a
/// redirect carrying the code the specification gives it.
#[test]
fn every_other_authorization_failure_carries_the_code_the_specification_names() {
    harness::run(async {
        let world = world("codes").await;
        let cookie = format!("rn_session={}", world.person.token);
        let base = format!(
            "/oidc/authorize?client_id={}&redirect_uri={}",
            world.client.id,
            encode(&world.client.redirect_uri)
        );

        let cases = [
            (
                format!(
                    "{base}&response_type=token&scope=openid&code_challenge={CHALLENGE}&code_challenge_method=S256"
                ),
                "unsupported_response_type",
            ),
            (
                format!(
                    "{base}&response_type=code&scope=email&code_challenge={CHALLENGE}&code_challenge_method=S256"
                ),
                "invalid_scope",
            ),
            (
                format!("{base}&response_type=code&scope=openid"),
                "invalid_request",
            ),
            (
                format!(
                    "{base}&response_type=code&scope=openid&code_challenge={CHALLENGE}&code_challenge_method=plain"
                ),
                "invalid_request",
            ),
            (
                format!(
                    "{base}&response_type=code&scope=openid&request=ey&code_challenge={CHALLENGE}&code_challenge_method=S256"
                ),
                "request_not_supported",
            ),
            (
                format!(
                    "{base}&response_type=code&scope=openid&request_uri=https%3A%2F%2Fa&code_challenge={CHALLENGE}&code_challenge_method=S256"
                ),
                "request_uri_not_supported",
            ),
        ];
        for (url, expected) in cases {
            let response = world
                .server
                .get(&url)
                .add_header("cookie", cookie.clone())
                .await;
            assert_eq!(response.status_code(), StatusCode::FOUND, "{url}");
            let location = response.headers()["location"].to_str().expect("ascii");
            assert!(location.starts_with(REDIRECT), "{location}");
            assert_eq!(
                parameter(location, "error").as_deref(),
                Some(expected),
                "{url}"
            );
            assert_eq!(
                parameter(location, "iss").as_deref(),
                Some(harness::PUBLIC_ORIGIN),
                "even a failure says who answered"
            );
        }
    });
}

/// `prompt=none` never paints. Every case that would is one of the four
/// errors, redirected.
#[test]
fn prompt_none_answers_with_an_error_and_never_with_a_page() {
    harness::run(async {
        let world = world("prompt").await;
        let url = format!(
            "/oidc/authorize?response_type=code&client_id={}&redirect_uri={}&scope=openid\
             &prompt=none&code_challenge={CHALLENGE}&code_challenge_method=S256",
            world.client.id,
            encode(&world.client.redirect_uri)
        );

        // Nobody signed in.
        let anonymous = world.server.get(&url).await;
        assert_eq!(anonymous.status_code(), StatusCode::FOUND);
        let location = anonymous.headers()["location"].to_str().expect("ascii");
        assert_eq!(
            parameter(location, "error").as_deref(),
            Some("login_required")
        );

        // Signed in, and has not consented.
        let signed_in = world
            .server
            .get(&url)
            .add_header("cookie", format!("rn_session={}", world.person.token))
            .await;
        assert_eq!(signed_in.status_code(), StatusCode::FOUND);
        let location = signed_in.headers()["location"].to_str().expect("ascii");
        assert_eq!(
            parameter(location, "error").as_deref(),
            Some("consent_required")
        );
    });
}

/// The token endpoint's refusals: a replayed code, a wrong verifier, a missing
/// one, a wrong secret, and a redirect URI that has changed.
#[test]
fn a_code_is_worth_one_exchange_and_only_with_the_verifier_that_made_it() {
    harness::run(async {
        let world = world("exchange").await;
        let redeem =
            |code: String, verifier: Option<&'static str>, redirect: String, auth: String| {
                let server = &world.server;
                async move {
                    let mut form = vec![
                        ("grant_type".to_owned(), "authorization_code".to_owned()),
                        ("code".to_owned(), code),
                        ("redirect_uri".to_owned(), redirect),
                    ];
                    if let Some(verifier) = verifier {
                        form.push(("code_verifier".to_owned(), verifier.to_owned()));
                    }
                    server
                        .post("/oidc/token")
                        .add_header("authorization", auth)
                        .form(&form)
                        .await
                }
            };

        // A verifier that is not the one: refused.
        let wrong = redeem(
            code(&world).await,
            Some("Zm9vYmFyZm9vYmFyZm9vYmFyZm9vYmFyZm9vYmFyZm9vYmFy"),
            world.client.redirect_uri.clone(),
            basic(&world.client),
        )
        .await;
        assert_eq!(wrong.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(wrong.json::<Value>()["error"], "invalid_grant");

        // No verifier at all — the downgrade PKCE exists to refuse.
        let none = redeem(
            code(&world).await,
            None,
            world.client.redirect_uri.clone(),
            basic(&world.client),
        )
        .await;
        assert_eq!(none.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(none.json::<Value>()["error"], "invalid_grant");

        // A redirect URI that is not the one the code was minted against.
        let moved = redeem(
            code(&world).await,
            Some(VERIFIER),
            "https://neg.example.test/elsewhere".to_owned(),
            basic(&world.client),
        )
        .await;
        assert_eq!(moved.status_code(), StatusCode::BAD_REQUEST);

        // A wrong secret is a `401` with a challenge, and it is the only kind
        // of failure here that is not `invalid_grant`.
        use base64::Engine as _;
        let forged = base64::engine::general_purpose::STANDARD
            .encode(format!("{}:not-the-secret", world.client.id));
        let bad_secret = redeem(
            code(&world).await,
            Some(VERIFIER),
            world.client.redirect_uri.clone(),
            format!("Basic {forged}"),
        )
        .await;
        assert_eq!(bad_secret.status_code(), StatusCode::UNAUTHORIZED);
        assert!(bad_secret.headers().contains_key("www-authenticate"));

        // And a code is worth exactly one exchange.
        let once = code(&world).await;
        redeem(
            once.clone(),
            Some(VERIFIER),
            world.client.redirect_uri.clone(),
            basic(&world.client),
        )
        .await
        .assert_status_ok();
        let twice = redeem(
            once,
            Some(VERIFIER),
            world.client.redirect_uri.clone(),
            basic(&world.client),
        )
        .await;
        assert_eq!(twice.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(twice.json::<Value>()["error"], "invalid_grant");
    });
}

/// An expired code is refused, and it is refused as an ordinary bad grant.
#[test]
fn a_code_older_than_a_minute_is_no_longer_a_code() {
    harness::run(async {
        let world = world("expiry").await;
        let code = code(&world).await;
        // Age *this* code in place: the clock this node runs on is the
        // system's, and the row is what the exchange reads. Addressed by its
        // own digest, because the suites share one node and a sweep of every
        // unredeemed code would expire another test's.
        world
            .state
            .store
            .execute(
                "UPDATE oidc_code SET expires_at = 1 WHERE code_hash = $1",
                rn_kernel::bind![rn_kernel::oidc::code::digest(&code).to_vec()],
            )
            .await
            .expect("the update");
        let response = world
            .server
            .post("/oidc/token")
            .add_header("authorization", basic(&world.client))
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", world.client.redirect_uri.as_str()),
                ("code_verifier", VERIFIER),
            ])
            .await;
        assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(response.json::<Value>()["error"], "invalid_grant");
    });
}

/// The two trust boundaries, from both sides.
#[test]
fn a_bearer_token_stops_at_the_two_routes_that_take_one_and_a_cookie_never_starts() {
    harness::run(async {
        let world = world("boundary").await;
        let code = code(&world).await;
        let issued: Value = world
            .server
            .post("/oidc/token")
            .add_header("authorization", basic(&world.client))
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", world.client.redirect_uri.as_str()),
                ("code_verifier", VERIFIER),
            ])
            .await
            .json();
        let access = issued["access_token"].as_str().expect("a token").to_owned();

        // It works where it is meant to.
        world
            .server
            .get("/oidc/userinfo")
            .add_header("authorization", format!("Bearer {access}"))
            .await
            .assert_status_ok();

        // And nowhere under `/api/*`, in a header or in a cookie.
        for path in ["/api/whoami", "/api/q/sessions", "/api/q/documents"] {
            let with_header = world
                .server
                .get(path)
                .add_header("authorization", format!("Bearer {access}"))
                .await;
            assert_eq!(
                with_header.status_code(),
                StatusCode::FORBIDDEN,
                "{path} accepted a bearer token"
            );
            let as_cookie = world
                .server
                .get(path)
                .add_header("cookie", format!("rn_session={access}"))
                .await;
            assert_eq!(
                as_cookie.status_code(),
                StatusCode::FORBIDDEN,
                "{path} accepted an access token as a session"
            );
        }

        // A session cookie does not satisfy the bearer endpoint.
        let cookie = world
            .server
            .get("/oidc/userinfo")
            .add_header("cookie", format!("rn_session={}", world.person.token))
            .await;
        assert_eq!(cookie.status_code(), StatusCode::UNAUTHORIZED);
    });
}

/// `groups` never crosses out of the client's owner's subtree.
#[test]
fn a_client_learns_no_membership_outside_the_organization_that_owns_it() {
    harness::run(async {
        let world = world("groups").await;
        // The person belongs to an organization nobody's client owns.
        harness::operate_organization(&world.state, world.person.person, "Somewhere Else").await;

        let authorize = world
            .server
            .post("/oidc/authorize")
            .add_header("cookie", format!("rn_session={}", world.person.token))
            .add_header("sec-fetch-site", "same-origin")
            .form(&[
                ("decision", "allow"),
                ("response_type", "code"),
                ("client_id", world.client.id.as_str()),
                ("redirect_uri", world.client.redirect_uri.as_str()),
                ("scope", "openid groups"),
                ("code_challenge", CHALLENGE),
                ("code_challenge_method", "S256"),
            ])
            .await;
        let location = authorize.headers()["location"].to_str().expect("ascii");
        let code = parameter(location, "code").expect("a code");
        let issued: Value = world
            .server
            .post("/oidc/token")
            .add_header("authorization", basic(&world.client))
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", world.client.redirect_uri.as_str()),
                ("code_verifier", VERIFIER),
            ])
            .await
            .json();
        let access = issued["access_token"].as_str().expect("a token");
        let claims: Value = world
            .server
            .get("/oidc/userinfo")
            .add_header("authorization", format!("Bearer {access}"))
            .await
            .json();
        assert_eq!(
            claims["groups"],
            serde_json::json!([]),
            "this client is the platform's own, so it has no subtree at all"
        );
    });
}

/// A `members_only` client refuses somebody with no relation under its owner.
#[test]
fn a_members_only_client_refuses_a_person_who_is_not_one() {
    harness::run(async {
        let state = state().await;
        let server = server(&state);
        let operator = harness::register(&state, "Op", "members-op@example.test").await;
        harness::operate_platform(&state, operator.person).await;
        ensure_key(&state, &operator).await;
        let client = register_client(
            &state,
            &operator,
            super::members_only(metadata("Members only", REDIRECT)),
        )
        .await;
        let stranger = harness::register(&state, "Stranger", "members-stranger@example.test").await;

        let response = server
            .get(&format!(
                "/oidc/authorize?response_type=code&client_id={}&redirect_uri={}&scope=openid\
                 &code_challenge={CHALLENGE}&code_challenge_method=S256",
                client.id,
                encode(&client.redirect_uri)
            ))
            .add_header("cookie", format!("rn_session={}", stranger.token))
            .await;
        assert_eq!(response.status_code(), StatusCode::FOUND);
        let location = response.headers()["location"].to_str().expect("ascii");
        assert_eq!(
            parameter(location, "error").as_deref(),
            Some("access_denied")
        );

        // The operator, who *is* one, gets through to the consent page.
        let allowed = server
            .get(&format!(
                "/oidc/authorize?response_type=code&client_id={}&redirect_uri={}&scope=openid\
                 &code_challenge={CHALLENGE}&code_challenge_method=S256",
                client.id,
                encode(&client.redirect_uri)
            ))
            .add_header("cookie", format!("rn_session={}", operator.token))
            .await;
        allowed.assert_status_ok();
    });
}

/// `client_credentials` needs a confidential client, and mints a token whose
/// principal is that client's own service party.
#[test]
fn a_service_token_is_only_for_a_client_that_can_hold_a_secret() {
    harness::run(async {
        let state = state().await;
        let server = server(&state);
        let operator = harness::register(&state, "Op", "svc-op@example.test").await;
        harness::operate_platform(&state, operator.person).await;
        ensure_key(&state, &operator).await;

        let mut confidential = metadata("Service", "https://svc.example.test/cb");
        confidential.grant_types = vec![GrantType::ClientCredentials];
        confidential.scopes = vec![Scope::Openid];
        let client = register_client(&state, &operator, confidential).await;

        let issued = server
            .post("/oidc/token")
            .add_header("authorization", basic(&client))
            .form(&[("grant_type", "client_credentials")])
            .await;
        issued.assert_status_ok();
        let body: Value = issued.json();
        assert!(body["access_token"].as_str().is_some());
        assert!(
            body.get("id_token").is_none(),
            "there is no end user to make claims about"
        );

        // And that token is not somebody: `userinfo` is about a person.
        let access = body["access_token"].as_str().expect("a token");
        let claims = server
            .get("/oidc/userinfo")
            .add_header("authorization", format!("Bearer {access}"))
            .await;
        assert_eq!(claims.status_code(), StatusCode::UNAUTHORIZED);

        // A public client asking for the same is refused: `none` here would be
        // an access token for the asking.
        let mut public = metadata("Public", "https://pub.example.test/cb");
        public.grant_types = vec![GrantType::ClientCredentials];
        public.token_endpoint_auth_method = ClientAuthMethod::None;
        public.scopes = vec![Scope::Openid];
        let open = register_client(&state, &operator, public).await;
        let refused = server
            .post("/oidc/token")
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", open.id.as_str()),
            ])
            .await;
        assert_eq!(refused.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(refused.json::<Value>()["error"], "invalid_grant");
    });
}

/// Registering a client is operator work, and a group never owns one.
#[test]
fn only_an_operator_registers_a_client_and_never_on_behalf_of_a_group() {
    harness::run(async {
        let state = state().await;
        let nobody = harness::register(&state, "Nobody", "reg-nobody@example.test").await;
        let refused = rn_kernel::cmd::register_client(
            &harness::ctx(&state, super::principal(&state, &nobody).await),
            &rn_api::commands::RegisterClient {
                owner: None,
                metadata: metadata("Not yours", REDIRECT),
            },
        )
        .await;
        assert!(refused.is_err(), "a member is not an operator");

        // The schema refuses a group as an owner even if a command forgot to.
        let operator = harness::register(&state, "Op", "reg-op@example.test").await;
        harness::operate_platform(&state, operator.person).await;
        let group = state
            .store
            .execute(
                "INSERT INTO party (kind, display_name, status, created_at) \
                 VALUES ('group', 'a group', 'active', 1)",
                rn_kernel::bind![],
            )
            .await;
        assert!(group.is_ok());
        let refused = state
            .store
            .execute(
                "INSERT INTO oidc_client (owner_party_id, service_party_id, client_name, \
                 redirect_uris, post_logout_redirect_uris, token_endpoint_auth_method, \
                 grant_types, scopes, trusted, members_only, created_at) \
                 SELECT id, id, 'c', '[]', '[]', 'none', '[]', '[]', 0, 0, 1 FROM party \
                 WHERE kind = 'group' LIMIT 1",
                rn_kernel::bind![],
            )
            .await;
        assert!(refused.is_err(), "a group never owns a client");
    });
}

/// A `private_key_jwt` client proves itself with a signature, once.
#[test]
fn an_assertion_authenticates_its_client_and_cannot_be_replayed() {
    harness::run(async {
        use rn_kernel::oidc::{jwt, key};

        let state = state().await;
        let server = server(&state);
        let operator = harness::register(&state, "Op", "pkj-op@example.test").await;
        harness::operate_platform(&state, operator.person).await;
        ensure_key(&state, &operator).await;

        // The client's own key, published as a JWKS in its registration.
        let private = rsa::RsaPrivateKey::new(&mut rand_core::OsRng, 2048).expect("a key");
        let (n, e) = key::jwk_parts(&rsa::RsaPublicKey::from(&private));
        let mut registration = metadata("Assertive", "https://pkj.example.test/cb");
        registration.token_endpoint_auth_method = ClientAuthMethod::PrivateKeyJwt;
        registration.jwks = Some(
            serde_json::json!({"keys": [{"kty": "RSA", "kid": "rp-1", "n": n, "e": e}]})
                .to_string(),
        );
        registration.grant_types = vec![GrantType::ClientCredentials];
        registration.scopes = vec![Scope::Openid];
        let client = register_client(&state, &operator, registration).await;
        assert!(
            client.secret.is_none(),
            "a client that signs has no secret to leak"
        );

        let assertion = |jti: &str, audience: &str| {
            jwt::sign(
                &private,
                "rp-1",
                jwt::TYP_JWT,
                &serde_json::json!({
                    "iss": client.id,
                    "sub": client.id,
                    "aud": audience,
                    "jti": jti,
                    "exp": 4_102_444_800_i64,
                }),
            )
            .expect("it signs")
        };
        let token_endpoint = format!("{}/oidc/token", harness::PUBLIC_ORIGIN);
        let post = |assertion: String| {
            let server = &server;
            let client_id = client.id.clone();
            async move {
                server
                    .post("/oidc/token")
                    .form(&[
                        ("grant_type", "client_credentials"),
                        ("client_id", client_id.as_str()),
                        ("client_assertion", assertion.as_str()),
                        (
                            "client_assertion_type",
                            "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
                        ),
                    ])
                    .await
            }
        };

        let first = assertion("jti-one", &token_endpoint);
        post(first.clone()).await.assert_status_ok();

        // The same assertion again: refused, because a `jti` is single use.
        let replayed = post(first).await;
        assert_eq!(replayed.status_code(), StatusCode::UNAUTHORIZED);

        // An assertion for another audience: refused, because that is the
        // whole of what `aud` is for.
        let elsewhere = post(assertion("jti-two", "https://another.test/token")).await;
        assert_eq!(elsewhere.status_code(), StatusCode::UNAUTHORIZED);

        // One signed by somebody else's key: refused.
        let stranger = rsa::RsaPrivateKey::new(&mut rand_core::OsRng, 2048).expect("a key");
        let forged = jwt::sign(
            &stranger,
            "rp-1",
            jwt::TYP_JWT,
            &serde_json::json!({
                "iss": client.id,
                "sub": client.id,
                "aud": token_endpoint,
                "jti": "jti-three",
                "exp": 4_102_444_800_i64,
            }),
        )
        .expect("it signs");
        assert_eq!(
            post(forged).await.status_code(),
            StatusCode::UNAUTHORIZED,
            "a signature is only worth the key that made it"
        );

        // And a fresh, correctly signed one still works — the refusals above
        // were about the requests, not about the client.
        post(assertion("jti-four", &token_endpoint))
            .await
            .assert_status_ok();
    });
}

/// The token endpoint stops answering a client that keeps getting it wrong.
#[test]
fn a_client_secret_cannot_be_ground_down_at_the_token_endpoint() {
    harness::run(async {
        let state = state().await;
        let server = server(&state);
        let operator = harness::register(&state, "Op", "limit-op@example.test").await;
        harness::operate_platform(&state, operator.person).await;
        ensure_key(&state, &operator).await;
        let client = register_client(
            &state,
            &operator,
            metadata("Guessable", "https://limit.example.test/cb"),
        )
        .await;

        use base64::Engine as _;
        let wrong = |n: usize| {
            let raw = base64::engine::general_purpose::STANDARD
                .encode(format!("{}:guess-{n}", client.id));
            format!("Basic {raw}")
        };
        let mut refused = 0;
        for n in 0..(rn_site::oidc::limits::BURST as usize + 4) {
            let response = server
                .post("/oidc/token")
                .add_header("authorization", wrong(n))
                .form(&[("grant_type", "client_credentials")])
                .await;
            if response.status_code() == StatusCode::TOO_MANY_REQUESTS {
                refused += 1;
            }
        }
        assert!(
            refused > 0,
            "a client that has failed {} times is asked to wait",
            rn_site::oidc::limits::BURST
        );

        // A different client is not slowed down by that one's failures.
        let other = register_client(
            &state,
            &operator,
            metadata("Innocent", "https://limit2.example.test/cb"),
        )
        .await;
        let response = server
            .post("/oidc/token")
            .add_header("authorization", basic(&other))
            .form(&[("grant_type", "client_credentials")])
            .await;
        assert_ne!(
            response.status_code(),
            StatusCode::TOO_MANY_REQUESTS,
            "the bucket is per client"
        );
    });
}
