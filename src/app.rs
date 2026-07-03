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

/// Builds the application router.
///
/// Keep this a readable table of contents: declare routes here and put the
/// request logic in [`crate::handlers`]. Page routes use the plain `.route()`
/// pass-through; JSON API routes go through `.routes(routes!(...))` so they
/// show up in `/api/openapi.json` too.
pub fn build_router(state: AppState, config: &Config) -> Router {
    // `client-errors` is the only unauthenticated write route left in
    // stage_2 — guestbook writes now require an `AccountScope` (see
    // `handlers::guestbook`), so they no longer need IP-based rate
    // limiting on top of that.
    let rate_limiter = RateLimiter::new(config.rate_limit_per_minute, config.trust_proxy);
    let client_error_api = OpenApiRouter::new()
        .routes(routes!(handlers::client_errors::report))
        .route_layer(middleware::from_fn_with_state(
            rate_limiter.clone(),
            rate_limit::enforce,
        ));

    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .route("/", get(handlers::home::index))
        .route("/about", get(handlers::about::index))
        .route("/guestbook", get(handlers::guestbook::index))
        .routes(routes!(handlers::guestbook::api_list, handlers::guestbook::api_create))
        .routes(routes!(handlers::health::healthz))
        .merge(client_error_api)
        .route("/login", get(handlers::auth::login_page).post(handlers::auth::login_submit))
        .route("/signup", get(handlers::auth::signup_page).post(handlers::auth::signup_submit))
        .route("/logout", post(handlers::auth::logout_submit))
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
        .route("/account", get(handlers::account::page).post(handlers::account::rename))
        .route("/account/audit", get(handlers::account::audit))
        .nest_service("/static", ServeDir::new("static"))
        .fallback(handlers::errors::not_found)
        // Layered individually (not via one `ServiceBuilder`): bundling the
        // `from_fn` error-page middleware together with `tower_http`'s
        // generic body-rewrapping layers (timeout, body-limit) in a single
        // `ServiceBuilder` stack defeats rustc's inference before it ever
        // reaches a concrete `Router` to anchor against. Each `.layer()`
        // call here resolves against the router directly instead.
        //
        // Router::layer's *last* call becomes the *outermost* wrapper (the
        // opposite of `ServiceBuilder`, where the first call is outermost)
        // — so this list reads innermost-first. Outer-to-inner, a request
        // actually passes through: SetRequestId, Propagate, the security
        // headers, Trace, attach_session, the error-page middleware,
        // Timeout, then the body-size limit, then the route.
        //
        // Timeout and body-limit sit innermost because both rewrap the
        // request/response body into types `Trace`'s `make_span` and the
        // error-page middleware's `Request` extractor aren't generic over
        // — those two are pinned to the plain `Body` type.
        .layer(RequestBodyLimitLayer::new(config.max_body_bytes))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(config.request_timeout_secs),
        ))
        // Renders the templated page for any AppError response. Must sit
        // inside (run after) attach_session, since the error page's nav
        // needs the session context attach_session inserts.
        .layer(middleware::from_fn(error::render_error_pages))
        // Resolves the session cookie once per request into request
        // extensions — `AccountScope`, the nav, and render_error_pages all
        // read the result from there instead of re-querying.
        .layer(middleware::from_fn_with_state(state.clone(), auth::middleware::attach_session))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(telemetry::make_span)
                .on_response(telemetry::on_response),
        )
        .layer(security_headers::content_security_policy())
        .layer(security_headers::referrer_policy())
        .layer(security_headers::x_frame_options())
        .layer(security_headers::x_content_type_options())
        // Propagate + the security headers wrap everything below, including
        // the timeout layer: if a request times out, the layers *inside*
        // the timeout (trace, error pages, the handler) never run for it,
        // so anything that must land on every response has to sit out here.
        .layer(PropagateRequestIdLayer::x_request_id())
        // Assign (or trust an inbound) x-request-id before anything else
        // runs, so every later layer can rely on it.
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state)
        .split_for_parts();

    router.route(
        "/api/openapi.json",
        get(move || {
            let api = api.clone();
            async move { Json(api) }
        }),
    )
}

/// Waits for Ctrl+C or SIGTERM so [`run`] can drain in-flight requests
/// before the process exits, instead of dropping connections on deploy.
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

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    info!("shutting down");
}

fn auth_config(config: &Config) -> AuthConfig {
    AuthConfig {
        cookie_secure: config.cookie_secure,
        session_ttl_secs: config.session_ttl_secs,
        signup_open: config.signup_open,
    }
}

/// Builds the app and serves it until the process is terminated.
pub async fn run() {
    let config = Config::from_env();
    let store = Store::connect(&config.database_url)
        .await
        .unwrap_or_else(|e| panic!("failed to connect to database at {}: {e}", config.database_url));
    let state = AppState::new(store, auth_config(&config));
    let app = build_router(state, &config);

    let listener = tokio::net::TcpListener::bind(&config.bind_addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {}: {e}", config.bind_addr));
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
    use axum::http::{StatusCode, header};
    use base64::Engine;
    use sha2::{Digest, Sha256};

    use crate::test_util::{
        Authed, get, get_with_cookie, post_bytes, post_form, post_form_with_cookie, post_json_authed,
        post_json_from, seed_session, signup, test_app,
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
    async fn oversized_body_is_rejected_with_413() {
        let (app, _store) = test_app().await;
        // Config::for_tests() caps bodies at 1024 bytes. /signup is a good
        // target: it's unauthenticated (no session to also set up) and
        // consumes its body via `Form<T>`, same as the JSON `Json<T>` case.
        let huge_name = "x".repeat(2048);
        let form = format!("display_name={huge_name}&email=a@example.com&password=password123");
        let (status, _, _) = post_bytes(&app, "/signup", "application/x-www-form-urlencoded", form.into_bytes()).await;
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
        assert!(body.contains("Alice"), "settings page should show the signed-up identity's name");
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
        assert_eq!(after, before + 1, "a failed login should audit login.failed");
    }

    #[tokio::test]
    async fn mutating_post_requires_csrf_token() {
        let (app, _store) = test_app().await;
        let Authed { cookie, csrf_token } = signup(&app, "Alice", "alice@example.com", "password123").await;
        let entry = serde_json::json!({"author": "Alice", "message": "hi"});

        let (status, _, _) = post_json_authed(&app, "/api/guestbook", &entry, &cookie, None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "missing CSRF token should be rejected");

        let (status, _, _) = post_json_authed(&app, "/api/guestbook", &entry, &cookie, Some(&csrf_token)).await;
        assert_eq!(status, StatusCode::OK, "a correct CSRF token should be accepted");
    }

    #[tokio::test]
    async fn cross_account_isolation() {
        let (app, _store) = test_app().await;
        let alice = signup(&app, "Alice", "alice@example.com", "password123").await;
        let entry = serde_json::json!({"author": "Alice", "message": "alice's secret"});
        let (status, _, _) =
            post_json_authed(&app, "/api/guestbook", &entry, &alice.cookie, Some(&alice.csrf_token)).await;
        assert_eq!(status, StatusCode::OK);

        // No route ever takes an account id from the request — Bob's
        // AccountScope always resolves to Bob's own account, so there's no
        // URL that could even attempt to address Alice's data. Bob's own
        // list is the observable proof: it must not contain her entry.
        let bob = signup(&app, "Bob", "bob@example.com", "password123").await;
        let (status, _, body) = get_with_cookie(&app, "/api/guestbook", &bob.cookie).await;
        assert_eq!(status, StatusCode::OK);
        let entries: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(entries.is_empty(), "Bob must not see Alice's guestbook entries");
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
        let bob_as_member = seed_session(&store, bob_factor.identity_id, alice_account.account_id).await;

        let form = format!("name=renamed&csrf_token={}", bob_as_member.csrf_token);
        let (status, _, _) = post_form_with_cookie(&app, "/account", &form, Some(&bob_as_member.cookie)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "a member should not be able to rename the account (admin+ only)");
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
        let sessions = store.list_sessions(alice_factor.identity_id, -1).await.unwrap();
        store
            .revoke_session(sessions[0].id, alice_factor.identity_id)
            .await
            .unwrap();

        let (status, headers, _) = get_with_cookie(&app, "/settings", &alice.cookie).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert!(headers.get(header::LOCATION).unwrap().to_str().unwrap().starts_with("/login"));
    }

    #[tokio::test]
    async fn bearer_token_grants_api_access_until_revoked() {
        let (app, store) = test_app().await;
        let alice = signup(&app, "Alice", "alice@example.com", "password123").await;

        let form = format!("csrf_token={}", alice.csrf_token);
        let (status, _, body) = post_form_with_cookie(&app, "/settings/tokens", &form, Some(&alice.cookie)).await;
        assert_eq!(status, StatusCode::OK, "minting a token re-renders /settings inline");
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
        let response = tower::ServiceExt::oneshot(app.clone(), request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "a valid bearer token should authenticate the JSON API");

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
        store.delete_factor(token_factor.id, alice_factor.identity_id).await.unwrap();

        let request = axum::http::Request::get("/api/guestbook")
            .header(header::AUTHORIZATION, format!("Bearer {raw_token}"))
            .body(axum::body::Body::empty())
            .unwrap();
        let response = tower::ServiceExt::oneshot(app.clone(), request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "a revoked token must stop authenticating");
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
        let (status, _, _) = post_form_with_cookie(&app, "/settings/tokens", &form, Some(&alice.cookie)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(store.count_factors(alice_factor.identity_id).await.unwrap(), 2);

        let factors = store.list_factors(alice_factor.identity_id).await.unwrap();
        let token_factor_id = factors.iter().find(|f| f.kind == "api_token").unwrap().id;

        // Removing one of two factors succeeds.
        let form = format!("csrf_token={}", alice.csrf_token);
        let (status, _, _) =
            post_form_with_cookie(&app, &format!("/settings/factors/{token_factor_id}/remove"), &form, Some(&alice.cookie)).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(store.count_factors(alice_factor.identity_id).await.unwrap(), 1);

        // Removing the last remaining factor is rejected.
        let form = format!("csrf_token={}", alice.csrf_token);
        let (status, _, _) = post_form_with_cookie(
            &app,
            &format!("/settings/factors/{}/remove", alice_factor.id),
            &form,
            Some(&alice.cookie),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "removing the last login method must be rejected");
    }
}
