//! Router assembly and server bootstrap.

use std::net::SocketAddr;
use std::time::Duration;

use axum::http::StatusCode;
use axum::middleware;
use axum::routing::{get, post};
use axum::{Json, Router};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::services::ServeDir;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing::info;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth;
use crate::config::Config;
use crate::error;
use crate::handlers;
use crate::openapi::ApiDoc;
use crate::rate_limit::{self, RateLimiter};
use crate::security_headers;
use crate::state::{AppState, AuthConfig};
use crate::store::Store;
use crate::telemetry;

/// Builds the deliberately unauthenticated public surface for `site`.
pub fn build_site_router(state: AppState, config: &Config) -> Router {
    let rate_limiter = RateLimiter::new(config.rate_limit_per_minute, config.trust_proxy);
    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .route("/", get(handlers::home::index))
        .routes(routes!(handlers::health::healthz))
        .merge(client_error_api(&rate_limiter))
        .nest_service("/static", ServeDir::new("static"))
        .fallback(handlers::errors::not_found)
        .with_state(state.clone())
        .split_for_parts();
    apply_layers(with_openapi_json(router, api), state, config, false)
}

/// Builds the full authenticated/admin surface, preserving the stage_2 routes.
pub fn build_admin_router(state: AppState, config: &Config) -> Router {
    let rate_limiter = RateLimiter::new(config.rate_limit_per_minute, config.trust_proxy);
    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .route("/", get(handlers::home::index))
        .route("/about", get(handlers::about::index))
        .route("/guestbook", get(handlers::guestbook::index))
        .routes(routes!(
            handlers::guestbook::api_list,
            handlers::guestbook::api_create
        ))
        .routes(routes!(handlers::health::healthz))
        .merge(client_error_api(&rate_limiter))
        .route(
            "/login",
            get(handlers::auth::login_page).post(handlers::auth::login_submit),
        )
        .route(
            "/signup",
            get(handlers::auth::signup_page).post(handlers::auth::signup_submit),
        )
        .route("/logout", post(handlers::auth::logout_submit))
        .route("/auth/oidc/{key}/start", get(handlers::auth::oidc_start))
        .route(
            "/auth/oidc/{key}/callback",
            get(handlers::auth::oidc_callback),
        )
        .route("/settings", get(handlers::settings::page))
        .route("/settings/tokens", post(handlers::settings::mint_token))
        .route(
            "/settings/factors/{factor_id}/remove",
            post(handlers::settings::remove_factor),
        )
        .route(
            "/settings/sessions/{session_id}/revoke",
            post(handlers::settings::revoke_session),
        )
        .route(
            "/account",
            get(handlers::account::page).post(handlers::account::rename),
        )
        .route("/account/audit", get(handlers::account::audit))
        .nest_service("/static", ServeDir::new("static"))
        .fallback(handlers::errors::not_found)
        .with_state(state.clone())
        .split_for_parts();
    apply_layers(with_openapi_json(router, api), state, config, true)
}

fn client_error_api(rate_limiter: &RateLimiter) -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::client_errors::report))
        .route_layer(middleware::from_fn_with_state(
            rate_limiter.clone(),
            rate_limit::enforce,
        ))
}

fn with_openapi_json(router: Router, api: utoipa::openapi::OpenApi) -> Router {
    router.route(
        "/api/openapi.json",
        get(move || {
            let api = api.clone();
            async move { Json(api) }
        }),
    )
}

fn apply_layers(router: Router, state: AppState, config: &Config, attach_sessions: bool) -> Router {
    // Layered individually (not via one `ServiceBuilder`): bundling the
    // `from_fn` error-page middleware with generic body-rewrapping layers
    // defeats rustc inference before a concrete `Router` anchors it.
    //
    // Router::layer's last call is outermost. Request flow is request-id,
    // security headers, trace, optional session resolution, error pages,
    // timeout, body limit, then route. Session resolution must remain outside
    // error rendering because error-page navigation reads its context.
    let router = router
        .layer(RequestBodyLimitLayer::new(config.max_body_bytes))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(config.request_timeout_secs),
        ))
        .layer(middleware::from_fn(error::render_error_pages));
    let router = if attach_sessions {
        router.layer(middleware::from_fn_with_state(
            state,
            auth::middleware::attach_session,
        ))
    } else {
        router
    };
    router
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(telemetry::make_span)
                .on_response(telemetry::on_response),
        )
        .layer(security_headers::content_security_policy())
        .layer(security_headers::referrer_policy())
        .layer(security_headers::x_frame_options())
        .layer(security_headers::x_content_type_options())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
    info!("shutting down");
}

fn auth_config(config: &Config) -> AuthConfig {
    AuthConfig {
        cookie_secure: config.cookie_secure,
        session_ttl_secs: config.session_ttl_secs,
        signup_open: config.signup_open,
    }
}

async fn state(config: &Config) -> AppState {
    let store = Store::connect(&config.database_url)
        .await
        .unwrap_or_else(|e| panic!("failed to open database at {}: {e}", config.database_url));
    let oidc = auth::oidc::OidcRegistry::from_path(&config.oidc_providers_path)
        .await
        .unwrap_or_else(|e| {
            panic!(
                "failed to load OIDC providers from {}: {e}",
                config.oidc_providers_path
            )
        });
    AppState::new(store, auth_config(config)).with_oidc(oidc)
}

pub async fn run_site() {
    let config = Config::from_env();
    let app = build_site_router(state(&config).await, &config);
    serve(app, &config.bind_addr).await;
}

pub async fn run_admin() {
    let config = Config::from_env();
    let app = build_admin_router(state(&config).await, &config);
    serve(app, &config.admin_bind_addr).await;
}

async fn serve(app: Router, bind_addr: &str) {
    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {bind_addr}: {e}"));
    info!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .unwrap();
}

#[cfg(test)]
pub(crate) fn test_auth_config(config: &Config) -> AuthConfig {
    auth_config(config)
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode, header};
    use base64::Engine;
    use sha2::{Digest, Sha256};

    use crate::test_util::{
        Authed, extract_cookie, get, get_with_cookie, post_bytes, post_form, post_form_with_cookie,
        post_json_authed, post_json_from, seed_session, signup, test_app, test_site_app,
    };

    #[tokio::test]
    async fn home_page_renders() {
        let (app, _store) = test_app().await;
        let (status, headers, body) = get(&app, "/").await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            headers
                .get(header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("stage_2"));
    }

    #[tokio::test]
    async fn unknown_route_renders_templated_404_with_request_id() {
        let (app, _store) = test_app().await;
        let (status, headers, body) = get(&app, "/nonexistent").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let request_id = headers
            .get("x-request-id")
            .expect("x-request-id header on every response")
            .to_str()
            .unwrap()
            .to_string();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains(&format!("ref: {request_id}")));
    }

    #[tokio::test]
    async fn healthz_reports_version_and_uptime() {
        let (app, _store) = test_app().await;
        let (status, _, body) = get(&app, "/healthz").await;
        assert_eq!(status, StatusCode::OK);
        let health: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(health["version"].is_string());
        assert!(health["uptime_secs"].is_number());
    }

    #[tokio::test]
    async fn responses_carry_security_headers_and_csp_matches_served_theme_script() {
        let (app, _store) = test_app().await;
        let (_, headers, body) = get(&app, "/").await;

        assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
        assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");
        let csp = headers
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap();

        // Drift guard: hash the <script> the router actually served and
        // confirm it's the same hash the CSP header allows.
        let body = String::from_utf8(body.to_vec()).unwrap();
        let start = body.find("<script>").unwrap() + "<script>".len();
        let end = start + body[start..].find("</script>").unwrap();
        let served_script = &body[start..end];
        let digest = Sha256::digest(served_script.as_bytes());
        let hash = base64::engine::general_purpose::STANDARD.encode(digest);
        assert!(
            csp.contains(&hash),
            "CSP {csp:?} does not allow the served inline script hash {hash}"
        );
    }

    #[tokio::test]
    async fn rate_limiter_returns_429_after_budget_exhausted() {
        let (app, _store) = test_app().await;
        let addr = std::net::IpAddr::V4(std::net::Ipv4Addr::new(198, 51, 100, 7));
        let body = serde_json::json!({"message": "boom", "source": "test", "line": 1, "col": 1, "stack": ""});

        // Config::for_tests() allows 10/min; the 11th from the same client
        // should be refused. client-errors is the only unauthenticated
        // write route left in stage_2 (guestbook now requires login).
        for _ in 0..10 {
            let (status, _, _) = post_json_from(&app, "/api/client-errors", &body, addr).await;
            assert_eq!(status, StatusCode::NO_CONTENT);
        }
        let (status, _, _) = post_json_from(&app, "/api/client-errors", &body, addr).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn site_has_no_auth_routes() {
        let (app, _store) = test_site_app().await;
        let (status, _, _) = get(&app, "/login").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn oversized_body_is_rejected_with_413() {
        let (app, _store) = test_app().await;
        // Config::for_tests() caps bodies at 1024 bytes. /signup is a good
        // target: it's unauthenticated (no session to also set up) and
        // consumes its body via `Form<T>`, same as the JSON `Json<T>` case.
        let huge_name = "x".repeat(2048);
        let form = format!("display_name={huge_name}&email=a@example.com&password=password123");
        let (status, _, _) = post_bytes(
            &app,
            "/signup",
            "application/x-www-form-urlencoded",
            form.into_bytes(),
        )
        .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    }

    // --- stage_2 auth exemplars (see docs/plans/2026-07-stage2-hardened-fork-template.md) ---

    #[tokio::test]
    async fn signup_creates_identity_and_authenticated_session_roundtrips() {
        let (app, _store) = test_app().await;
        let Authed { cookie, .. } = signup(&app, "Alice", "alice@example.com", "password123").await;

        let (status, _, body) = get_with_cookie(&app, "/settings", &cookie).await;
        assert_eq!(status, StatusCode::OK);
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            body.contains("Alice"),
            "settings page should show the signed-up identity's name"
        );
    }

    #[tokio::test]
    async fn login_page_has_no_sso_markup_without_configured_providers() {
        let (app, _store) = test_app().await;
        let (status, _, body) = get(&app, "/login").await;
        assert_eq!(status, StatusCode::OK);
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(!body.contains("Continue with"));
        assert!(!body.contains("Or use SSO"));
    }

    #[tokio::test]
    async fn login_wrong_password_is_rejected_and_audited() {
        let (app, store) = test_app().await;
        signup(&app, "Alice", "alice@example.com", "password123").await;

        let before = store.count_audit_events("login.failed").await.unwrap();
        let form = "email=alice%40example.com&password=wrong-password";
        let (status, _, _) = post_form(&app, "/login", form).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let after = store.count_audit_events("login.failed").await.unwrap();
        assert_eq!(
            after,
            before + 1,
            "a failed login should audit login.failed"
        );
    }

    #[tokio::test]
    async fn mutating_post_requires_csrf_token() {
        let (app, _store) = test_app().await;
        let Authed { cookie, csrf_token } =
            signup(&app, "Alice", "alice@example.com", "password123").await;
        let entry = serde_json::json!({"author": "Alice", "message": "hi"});

        let (status, _, _) = post_json_authed(&app, "/api/guestbook", &entry, &cookie, None).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "missing CSRF token should be rejected"
        );

        let (status, _, _) =
            post_json_authed(&app, "/api/guestbook", &entry, &cookie, Some(&csrf_token)).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "a correct CSRF token should be accepted"
        );
    }

    #[tokio::test]
    async fn cross_account_isolation() {
        let (app, _store) = test_app().await;
        let alice = signup(&app, "Alice", "alice@example.com", "password123").await;
        let entry = serde_json::json!({"author": "Alice", "message": "alice's secret"});
        let (status, _, _) = post_json_authed(
            &app,
            "/api/guestbook",
            &entry,
            &alice.cookie,
            Some(&alice.csrf_token),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        // No route ever takes an account id from the request — Bob's
        // AccountScope always resolves to Bob's own account, so there's no
        // URL that could even attempt to address Alice's data. Bob's own
        // list is the observable proof: it must not contain her entry.
        let bob = signup(&app, "Bob", "bob@example.com", "password123").await;
        let (status, _, body) = get_with_cookie(&app, "/api/guestbook", &bob.cookie).await;
        assert_eq!(status, StatusCode::OK);
        let entries: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(
            entries.is_empty(),
            "Bob must not see Alice's guestbook entries"
        );
    }

    #[tokio::test]
    async fn member_role_cannot_hit_admin_route() {
        let (app, store) = test_app().await;
        signup(&app, "Alice", "alice@example.com", "password123").await;
        let alice_factor = store
            .find_factor_by_external("password", "alice@example.com")
            .await
            .unwrap()
            .unwrap();
        let alice_account = store
            .find_primary_membership(alice_factor.identity_id)
            .await
            .unwrap()
            .unwrap();

        signup(&app, "Bob", "bob@example.com", "password123").await;
        let bob_factor = store
            .find_factor_by_external("password", "bob@example.com")
            .await
            .unwrap()
            .unwrap();

        // No invite flow exists yet in phase 1 (that's phase 2) to produce
        // a non-owner membership through the UI — seed it directly, then
        // seed a session so Bob is acting as a "member" of Alice's account.
        store
            .create_membership(bob_factor.identity_id, alice_account.account_id, "member")
            .await
            .unwrap();
        let bob_as_member =
            seed_session(&store, bob_factor.identity_id, alice_account.account_id).await;

        let form = format!("name=renamed&csrf_token={}", bob_as_member.csrf_token);
        let (status, _, _) =
            post_form_with_cookie(&app, "/account", &form, Some(&bob_as_member.cookie)).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "a member should not be able to rename the account (admin+ only)"
        );
    }

    #[tokio::test]
    async fn revoked_session_redirects_to_login() {
        let (app, store) = test_app().await;
        let alice = signup(&app, "Alice", "alice@example.com", "password123").await;
        let alice_factor = store
            .find_factor_by_external("password", "alice@example.com")
            .await
            .unwrap()
            .unwrap();
        let sessions = store
            .list_sessions(alice_factor.identity_id, -1)
            .await
            .unwrap();
        store
            .revoke_session(sessions[0].id, alice_factor.identity_id)
            .await
            .unwrap();

        let (status, headers, _) = get_with_cookie(&app, "/settings", &alice.cookie).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert!(
            headers
                .get(header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("/login")
        );
    }

    #[tokio::test]
    async fn bearer_token_grants_api_access_until_revoked() {
        let (app, store) = test_app().await;
        let alice = signup(&app, "Alice", "alice@example.com", "password123").await;

        let form = format!("csrf_token={}", alice.csrf_token);
        let (status, _, body) =
            post_form_with_cookie(&app, "/settings/tokens", &form, Some(&alice.cookie)).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "minting a token re-renders /settings inline"
        );
        let raw_token = {
            let body = String::from_utf8(body.to_vec()).unwrap();
            let marker = "<code>";
            let start = body.find(marker).unwrap() + marker.len();
            let end = start + body[start..].find("</code>").unwrap();
            body[start..end].to_string()
        };

        let request = axum::http::Request::get("/api/guestbook")
            .header(header::AUTHORIZATION, format!("Bearer {raw_token}"))
            .body(axum::body::Body::empty())
            .unwrap();
        let response = tower::ServiceExt::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "a valid bearer token should authenticate the JSON API"
        );

        let alice_factor = store
            .find_factor_by_external("password", "alice@example.com")
            .await
            .unwrap()
            .unwrap();
        let token_factor = store
            .find_factor_by_secret_hash(&crate::auth::session::hash_token(&raw_token))
            .await
            .unwrap()
            .unwrap();
        store
            .delete_factor(token_factor.id, alice_factor.identity_id)
            .await
            .unwrap();

        let request = axum::http::Request::get("/api/guestbook")
            .header(header::AUTHORIZATION, format!("Bearer {raw_token}"))
            .body(axum::body::Body::empty())
            .unwrap();
        let response = tower::ServiceExt::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "a revoked token must stop authenticating"
        );
    }

    #[tokio::test]
    async fn factor_lifecycle_link_second_and_guard_last_factor() {
        let (app, store) = test_app().await;
        let alice = signup(&app, "Alice", "alice@example.com", "password123").await;
        let alice_factor = store
            .find_factor_by_external("password", "alice@example.com")
            .await
            .unwrap()
            .unwrap();

        // Link a second factor while authed.
        let form = format!("csrf_token={}", alice.csrf_token);
        let (status, _, _) =
            post_form_with_cookie(&app, "/settings/tokens", &form, Some(&alice.cookie)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            store.count_factors(alice_factor.identity_id).await.unwrap(),
            2
        );

        let factors = store.list_factors(alice_factor.identity_id).await.unwrap();
        let token_factor_id = factors.iter().find(|f| f.kind == "api_token").unwrap().id;

        // Removing one of two factors succeeds.
        let form = format!("csrf_token={}", alice.csrf_token);
        let (status, _, _) = post_form_with_cookie(
            &app,
            &format!("/settings/factors/{token_factor_id}/remove"),
            &form,
            Some(&alice.cookie),
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(
            store.count_factors(alice_factor.identity_id).await.unwrap(),
            1
        );

        // Removing the last remaining factor is rejected.
        let form = format!("csrf_token={}", alice.csrf_token);
        let (status, _, _) = post_form_with_cookie(
            &app,
            &format!("/settings/factors/{}/remove", alice_factor.id),
            &form,
            Some(&alice.cookie),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "removing the last login method must be rejected"
        );
    }

    #[derive(Clone, Copy)]
    enum MockMode {
        Good,
        NonceMismatch,
        BadSignature,
        Expired,
    }

    #[derive(Clone)]
    struct MockOp {
        issuer: String,
        router: axum::Router,
        mode: std::sync::Arc<std::sync::Mutex<MockMode>>,
        subject: std::sync::Arc<std::sync::Mutex<String>>,
    }

    #[derive(Clone)]
    struct MockOpState {
        issuer: String,
        signing_key: std::sync::Arc<openidconnect::core::CoreRsaPrivateSigningKey>,
        bad_signing_key: std::sync::Arc<openidconnect::core::CoreRsaPrivateSigningKey>,
        codes: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, MockCode>>>,
        mode: std::sync::Arc<std::sync::Mutex<MockMode>>,
        subject: std::sync::Arc<std::sync::Mutex<String>>,
    }

    #[derive(Clone)]
    struct MockCode {
        nonce: String,
        client_id: String,
    }

    impl MockOp {
        fn new() -> Self {
            use axum::routing::{get, post};
            let issuer = "http://mock-op.test".to_string();
            let signing_key = std::sync::Arc::new(
                openidconnect::core::CoreRsaPrivateSigningKey::from_pem(
                    TEST_RSA_KEY,
                    Some(openidconnect::JsonWebKeyId::new("good".into())),
                )
                .unwrap(),
            );
            let bad_signing_key = std::sync::Arc::new(
                openidconnect::core::CoreRsaPrivateSigningKey::from_pem(
                    TEST_RSA_KEY,
                    Some(openidconnect::JsonWebKeyId::new("bad".into())),
                )
                .unwrap(),
            );
            let mode = std::sync::Arc::new(std::sync::Mutex::new(MockMode::Good));
            let subject = std::sync::Arc::new(std::sync::Mutex::new("subject-1".to_string()));
            let state = MockOpState {
                issuer: issuer.clone(),
                signing_key,
                bad_signing_key,
                codes: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
                mode: mode.clone(),
                subject: subject.clone(),
            };
            let router = axum::Router::new()
                .route("/.well-known/openid-configuration", get(mock_discovery))
                .route("/jwks", get(mock_jwks))
                .route("/authorize", get(mock_authorize))
                .route("/token", post(mock_token))
                .with_state(state);
            Self {
                issuer,
                router,
                mode,
                subject,
            }
        }

        fn set_mode(&self, mode: MockMode) {
            *self.mode.lock().unwrap() = mode;
        }

        fn set_subject(&self, subject: &str) {
            *self.subject.lock().unwrap() = subject.to_string();
        }
    }

    impl crate::auth::oidc::OidcHttpClient for MockOp {
        fn call<'c>(
            &'c self,
            request: axum::http::Request<Vec<u8>>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<
                            axum::http::Response<Vec<u8>>,
                            crate::auth::oidc::OidcHttpError,
                        >,
                    > + Send
                    + 'c,
            >,
        > {
            Box::pin(async move {
                let (parts, body) = request.into_parts();
                let uri = parts
                    .uri
                    .path_and_query()
                    .map(|pq| pq.as_str().to_string())
                    .unwrap_or_else(|| "/".into());
                let mut builder = axum::http::Request::builder().method(parts.method).uri(uri);
                *builder.headers_mut().unwrap() = parts.headers;
                let request = builder.body(axum::body::Body::from(body)).unwrap();
                let response = tower::ServiceExt::oneshot(self.router.clone(), request)
                    .await
                    .unwrap();
                let status = response.status();
                let headers = response.headers().clone();
                let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap()
                    .to_vec();
                let mut builder = axum::http::Response::builder().status(status);
                *builder.headers_mut().unwrap() = headers;
                Ok(builder.body(body).unwrap())
            })
        }
    }

    async fn mock_discovery(
        axum::extract::State(state): axum::extract::State<MockOpState>,
    ) -> axum::Json<serde_json::Value> {
        axum::Json(serde_json::json!({
            "issuer": state.issuer,
            "authorization_endpoint": format!("{}/authorize", state.issuer),
            "token_endpoint": format!("{}/token", state.issuer),
            "jwks_uri": format!("{}/jwks", state.issuer),
            "response_types_supported": ["code"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["RS256"],
        }))
    }

    async fn mock_jwks(
        axum::extract::State(state): axum::extract::State<MockOpState>,
    ) -> axum::Json<serde_json::Value> {
        use openidconnect::PrivateSigningKey;
        let jwks = openidconnect::core::CoreJsonWebKeySet::new(vec![
            state.signing_key.as_verification_key(),
        ]);
        axum::Json(serde_json::to_value(jwks).unwrap())
    }

    #[derive(serde::Deserialize)]
    struct AuthorizeQuery {
        client_id: String,
        redirect_uri: String,
        state: String,
        nonce: String,
    }

    async fn mock_authorize(
        axum::extract::State(state): axum::extract::State<MockOpState>,
        axum::extract::Query(query): axum::extract::Query<AuthorizeQuery>,
    ) -> axum::response::Redirect {
        let code = crate::auth::session::generate_token();
        state.codes.lock().unwrap().insert(
            code.clone(),
            MockCode {
                nonce: query.nonce,
                client_id: query.client_id,
            },
        );
        axum::response::Redirect::to(&format!(
            "{}?code={}&state={}",
            query.redirect_uri, code, query.state
        ))
    }

    async fn mock_token(
        axum::extract::State(state): axum::extract::State<MockOpState>,
        body: String,
    ) -> (
        [(axum::http::header::HeaderName, &'static str); 1],
        axum::Json<serde_json::Value>,
    ) {
        let params = openidconnect::url::form_urlencoded::parse(body.as_bytes())
            .collect::<std::collections::HashMap<_, _>>();
        let code = params.get("code").unwrap().to_string();
        let code = state.codes.lock().unwrap().remove(&code).unwrap();
        let access_token = openidconnect::AccessToken::new(crate::auth::session::generate_token());
        let mode = *state.mode.lock().unwrap();
        let nonce = match mode {
            MockMode::NonceMismatch => "wrong-nonce".to_string(),
            _ => code.nonce,
        };
        let now = chrono::Utc::now();
        let expires_at = match mode {
            MockMode::Expired => now - chrono::Duration::seconds(30),
            _ => now + chrono::Duration::seconds(300),
        };
        let claims = openidconnect::core::CoreIdTokenClaims::new(
            openidconnect::IssuerUrl::new(state.issuer.clone()).unwrap(),
            vec![openidconnect::Audience::new(code.client_id)],
            expires_at,
            now,
            openidconnect::StandardClaims::new(openidconnect::SubjectIdentifier::new(
                state.subject.lock().unwrap().clone(),
            ))
            .set_name(Some(openidconnect::LocalizedClaim::from(
                openidconnect::EndUserName::new("OIDC Alice".into()),
            )))
            .set_email(Some(openidconnect::EndUserEmail::new(
                "oidc-alice@example.com".into(),
            ))),
            openidconnect::EmptyAdditionalClaims {},
        )
        .set_nonce(Some(openidconnect::Nonce::new(nonce)));
        let key = match mode {
            MockMode::BadSignature => &*state.bad_signing_key,
            _ => &*state.signing_key,
        };
        let id_token = openidconnect::core::CoreIdToken::new(
            claims,
            key,
            openidconnect::core::CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
            Some(&access_token),
            None,
        )
        .unwrap();
        (
            [(header::CONTENT_TYPE, "application/json")],
            axum::Json(serde_json::json!({
                "access_token": access_token.secret(),
                "token_type": "Bearer",
                "expires_in": 600,
                "id_token": id_token,
            })),
        )
    }

    async fn oidc_app(auto_provision: bool) -> (axum::Router, crate::store::Store, MockOp) {
        let op = MockOp::new();
        let registry = crate::auth::oidc::OidcRegistry::from_configs(
            vec![crate::auth::oidc::OidcProviderConfig {
                key: "mock".into(),
                display_name: "Mock SSO".into(),
                issuer_url: op.issuer.clone(),
                client_id: "stage2".into(),
                client_secret: "test-secret".into(),
                scopes: None,
                auto_provision: Some(auto_provision),
            }],
            crate::auth::oidc::DynOidcHttpClient::new(op.clone()),
        )
        .await
        .unwrap();
        let store = crate::store::Store::connect_in_memory().await;
        let config = crate::config::Config::for_tests();
        let state = crate::state::AppState::new(store.clone(), super::test_auth_config(&config))
            .with_oidc(registry);
        (super::build_admin_router(state, &config), store, op)
    }

    async fn run_oidc_login(
        app: &axum::Router,
        op: &MockOp,
    ) -> (StatusCode, axum::http::HeaderMap, axum::body::Bytes) {
        let (status, headers, _) = get(app, "/auth/oidc/mock/start").await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let authorize_url = headers.get(header::LOCATION).unwrap().to_str().unwrap();
        let authorize_path = openidconnect::url::Url::parse(authorize_url)
            .unwrap()
            .path()
            .to_owned()
            + "?"
            + openidconnect::url::Url::parse(authorize_url)
                .unwrap()
                .query()
                .unwrap();
        let (status, headers, _) = mock_get(op, &authorize_path).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let callback_url = headers.get(header::LOCATION).unwrap().to_str().unwrap();
        let parsed = openidconnect::url::Url::parse(callback_url).unwrap();
        let callback_path = parsed.path().to_owned() + "?" + parsed.query().unwrap();
        get(app, &callback_path).await
    }

    async fn mock_get(
        op: &MockOp,
        path: &str,
    ) -> (StatusCode, axum::http::HeaderMap, axum::body::Bytes) {
        let request = axum::http::Request::builder()
            .method(Method::GET)
            .uri(path)
            .body(axum::body::Body::empty())
            .unwrap();
        let response = tower::ServiceExt::oneshot(op.router.clone(), request)
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, headers, body)
    }

    #[tokio::test]
    async fn oidc_login_renders_button_and_provisions_then_reuses_identity() {
        let (app, store, op) = oidc_app(true).await;
        let (status, _, body) = get(&app, "/login").await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            String::from_utf8(body.to_vec())
                .unwrap()
                .contains("Continue with Mock SSO")
        );

        let (status, headers, _) = run_oidc_login(&app, &op).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let cookie = extract_cookie(&headers, "session");
        let factor = store
            .find_factor_by_external("oidc", "http://mock-op.test#subject-1")
            .await
            .unwrap()
            .unwrap();
        assert!(
            store
                .find_primary_membership(factor.identity_id)
                .await
                .unwrap()
                .is_some()
        );
        let (status, _, body) = get_with_cookie(&app, "/settings", &cookie).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            String::from_utf8(body.to_vec())
                .unwrap()
                .contains("OIDC Alice")
        );

        let (status, _, _) = run_oidc_login(&app, &op).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let same_factor = store
            .find_factor_by_external("oidc", "http://mock-op.test#subject-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(same_factor.identity_id, factor.identity_id);
        assert_eq!(store.count_factors(factor.identity_id).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn oidc_validation_failures_and_auto_provision_closed_are_rejected() {
        let (app, _store, op) = oidc_app(true).await;
        let (status, headers, _) = get(&app, "/auth/oidc/mock/start").await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let bad_state_url = headers.get(header::LOCATION).unwrap().to_str().unwrap();
        let parsed = openidconnect::url::Url::parse(bad_state_url).unwrap();
        let callback = format!(
            "/auth/oidc/mock/callback?code=unused&state=wrong-{}",
            parsed.query().unwrap().len()
        );
        let (status, _, _) = get(&app, &callback).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "state mismatch is rejected"
        );

        for mode in [
            MockMode::NonceMismatch,
            MockMode::BadSignature,
            MockMode::Expired,
        ] {
            op.set_mode(mode);
            let (status, _, _) = run_oidc_login(&app, &op).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED);
        }

        let (closed_app, closed_store, closed_op) = oidc_app(false).await;
        let (status, _, _) = run_oidc_login(&closed_app, &closed_op).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(
            closed_store
                .find_factor_by_external("oidc", "http://mock-op.test#subject-1")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn oidc_link_claim_conflict_and_last_factor_guard() {
        let (app, store, op) = oidc_app(true).await;
        let (status, _, _) = run_oidc_login(&app, &op).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let claimed = store
            .find_factor_by_external("oidc", "http://mock-op.test#subject-1")
            .await
            .unwrap()
            .unwrap();

        let alice = signup(&app, "Alice", "alice@example.com", "password123").await;
        let (status, _, body) = get_with_cookie(&app, "/settings", &alice.cookie).await;
        assert_eq!(status, StatusCode::OK);
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("Link Mock SSO"));
        let start = format!(
            "/auth/oidc/mock/start?intent=link&csrf_token={}",
            alice.csrf_token
        );
        let (status, headers, _) = get_with_cookie(&app, &start, &alice.cookie).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let authorize_url = headers.get(header::LOCATION).unwrap().to_str().unwrap();
        let parsed = openidconnect::url::Url::parse(authorize_url).unwrap();
        let authorize_path = parsed.path().to_owned() + "?" + parsed.query().unwrap();
        let (_, headers, _) = mock_get(&op, &authorize_path).await;
        let callback_url = headers.get(header::LOCATION).unwrap().to_str().unwrap();
        let parsed = openidconnect::url::Url::parse(callback_url).unwrap();
        let callback_path = parsed.path().to_owned() + "?" + parsed.query().unwrap();
        let (status, _, _) = get(&app, &callback_path).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "already-claimed subject cannot be linked"
        );

        op.set_subject("subject-2");
        let (status, headers, _) = get_with_cookie(&app, &start, &alice.cookie).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let authorize_url = headers.get(header::LOCATION).unwrap().to_str().unwrap();
        let parsed = openidconnect::url::Url::parse(authorize_url).unwrap();
        let authorize_path = parsed.path().to_owned() + "?" + parsed.query().unwrap();
        let (_, headers, _) = mock_get(&op, &authorize_path).await;
        let callback_url = headers.get(header::LOCATION).unwrap().to_str().unwrap();
        let parsed = openidconnect::url::Url::parse(callback_url).unwrap();
        let callback_path = parsed.path().to_owned() + "?" + parsed.query().unwrap();
        let (status, _, _) = get(&app, &callback_path).await;
        assert_eq!(
            status,
            StatusCode::SEE_OTHER,
            "unclaimed subject links successfully"
        );
        let alice_factor = store
            .find_factor_by_external("password", "alice@example.com")
            .await
            .unwrap()
            .unwrap();
        let linked = store
            .find_factor_by_external("oidc", "http://mock-op.test#subject-2")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(linked.identity_id, alice_factor.identity_id);
        assert_eq!(
            store.count_factors(alice_factor.identity_id).await.unwrap(),
            2
        );

        let oidc_only = seed_session(
            &store,
            claimed.identity_id,
            store
                .find_primary_membership(claimed.identity_id)
                .await
                .unwrap()
                .unwrap()
                .account_id,
        )
        .await;
        let form = format!("csrf_token={}", oidc_only.csrf_token);
        let (status, _, _) = post_form_with_cookie(
            &app,
            &format!("/settings/factors/{}/remove", claimed.id),
            &form,
            Some(&oidc_only.cookie),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "OIDC factors use the existing last-factor guard"
        );
    }

    const TEST_RSA_KEY: &str = "-----BEGIN RSA PRIVATE KEY-----\n\
         MIIEowIBAAKCAQEAn4EPtAOCc9AlkeQHPzHStgAbgs7bTZLwUBZdR8/KuKPEHLd4\n\
         rHVTeT+O+XV2jRojdNhxJWTDvNd7nqQ0VEiZQHz/AJmSCpMaJMRBSFKrKb2wqVwG\n\
         U/NsYOYL+QtiWN2lbzcEe6XC0dApr5ydQLrHqkHHig3RBordaZ6Aj+oBHqFEHYpP\n\
         e7Tpe+OfVfHd1E6cS6M1FZcD1NNLYD5lFHpPI9bTwJlsde3uhGqC0ZCuEHg8lhzw\n\
         OHrtIQbS0FVbb9k3+tVTU4fg/3L/vniUFAKwuCLqKnS2BYwdq/mzSnbLY7h/qixo\n\
         R7jig3//kRhuaxwUkRz5iaiQkqgc5gHdrNP5zwIDAQABAoIBAG1lAvQfhBUSKPJK\n\
         Rn4dGbshj7zDSr2FjbQf4pIh/ZNtHk/jtavyO/HomZKV8V0NFExLNi7DUUvvLiW7\n\
         0PgNYq5MDEjJCtSd10xoHa4QpLvYEZXWO7DQPwCmRofkOutf+NqyDS0QnvFvp2d+\n\
         Lov6jn5C5yvUFgw6qWiLAPmzMFlkgxbtjFAWMJB0zBMy2BqjntOJ6KnqtYRMQUxw\n\
         TgXZDF4rhYVKtQVOpfg6hIlsaoPNrF7dofizJ099OOgDmCaEYqM++bUlEHxgrIVk\n\
         wZz+bg43dfJCocr9O5YX0iXaz3TOT5cpdtYbBX+C/5hwrqBWru4HbD3xz8cY1TnD\n\
         qQa0M8ECgYEA3Slxg/DwTXJcb6095RoXygQCAZ5RnAvZlno1yhHtnUex/fp7AZ/9\n\
         nRaO7HX/+SFfGQeutao2TDjDAWU4Vupk8rw9JR0AzZ0N2fvuIAmr/WCsmGpeNqQn\n\
         ev1T7IyEsnh8UMt+n5CafhkikzhEsrmndH6LxOrvRJlsPp6Zv8bUq0kCgYEAuKE2\n\
         dh+cTf6ERF4k4e/jy78GfPYUIaUyoSSJuBzp3Cubk3OCqs6grT8bR/cu0Dm1MZwW\n\
         mtdqDyI95HrUeq3MP15vMMON8lHTeZu2lmKvwqW7anV5UzhM1iZ7z4yMkuUwFWoB\n\
         vyY898EXvRD+hdqRxHlSqAZ192zB3pVFJ0s7pFcCgYAHw9W9eS8muPYv4ZhDu/fL\n\
         2vorDmD1JqFcHCxZTOnX1NWWAj5hXzmrU0hvWvFC0P4ixddHf5Nqd6+5E9G3k4E5\n\
         2IwZCnylu3bqCWNh8pT8T3Gf5FQsfPT5530T2BcsoPhUaeCnP499D+rb2mTnFYeg\n\
         mnTT1B/Ue8KGLFFfn16GKQKBgAiw5gxnbocpXPaO6/OKxFFZ+6c0OjxfN2PogWce\n\
         TU/k6ZzmShdaRKwDFXisxRJeNQ5Rx6qgS0jNFtbDhW8E8WFmQ5urCOqIOYk28EBi\n\
         At4JySm4v+5P7yYBh8B8YD2l9j57z/s8hJAxEbn/q8uHP2ddQqvQKgtsni+pHSk9\n\
         XGBfAoGBANz4qr10DdM8DHhPrAb2YItvPVz/VwkBd1Vqj8zCpyIEKe/07oKOvjWQ\n\
         SgkLDH9x2hBgY01SbP43CvPk0V72invu2TGkI/FXwXWJLLG7tDSgw4YyfhrYrHmg\n\
         1Vre3XB9HH8MYBVB6UIexaAq4xSeoemRKTBesZro7OKjKT8/GmiO\n\
         -----END RSA PRIVATE KEY-----";
}
