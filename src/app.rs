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
        .route("/e/{token}", get(handlers::event_public::page))
        .merge(
            OpenApiRouter::new()
                .route(
                    "/e/{token}/claim",
                    get(handlers::guest_accounts::claim_page)
                        .post(handlers::guest_accounts::claim_submit),
                )
                .route(
                    "/login",
                    get(handlers::guest_accounts::login_page)
                        .post(handlers::guest_accounts::login_submit),
                )
                .route_layer(middleware::from_fn_with_state(
                    rate_limiter.clone(),
                    rate_limit::enforce,
                )),
        )
        .route("/logout", post(handlers::guest_accounts::logout_submit))
        .route("/my", get(handlers::guest_accounts::my_page))
        .route(
            "/my/events/{event_id}",
            get(handlers::guest_accounts::my_event_page),
        )
        .route(
            "/api/my/events/{event_id}",
            get(handlers::guest_accounts::api_my_view),
        )
        .route(
            "/api/my/events/{event_id}/rsvp",
            post(handlers::guest_accounts::api_my_rsvp),
        )
        .route("/events/{event_ref}/ics", get(handlers::event_public::ics))
        .routes(routes!(handlers::event_public::api_view))
        .merge(
            OpenApiRouter::new()
                .routes(routes!(handlers::event_public::api_rsvp))
                .route_layer(middleware::from_fn_with_state(
                    rate_limiter.clone(),
                    rate_limit::enforce,
                )),
        )
        .nest_service("/static", ServeDir::new("static"))
        .fallback(handlers::errors::not_found)
        .with_state(state.clone())
        .split_for_parts();
    // Public capability pages may recognize the owner session, but the site
    // router still exposes none of the admin auth/account routes.
    apply_layers(with_openapi_json(router, api), state, config, true)
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
            "/events",
            get(handlers::events_admin::list_page).post(handlers::events_admin::create_event),
        )
        .route(
            "/events/{event_id}",
            get(handlers::events_admin::detail_page).post(handlers::errors::not_found),
        )
        .route(
            "/events/{event_id}/audience",
            get(handlers::audience::page).post(handlers::audience::save),
        )
        .route(
            "/circles",
            get(handlers::circles::list).post(handlers::circles::create),
        )
        .route(
            "/circles/{id}",
            get(handlers::circles::detail).post(handlers::circles::rename),
        )
        .route("/circles/{id}/delete", post(handlers::circles::delete))
        .route("/circles/{id}/members", post(handlers::circles::add_member))
        .route(
            "/circles/{id}/members/{person_id}/delete",
            post(handlers::circles::remove_member),
        )
        .route(
            "/events/{event_id}/schedule",
            post(handlers::errors::not_found),
        )
        .route(
            "/events/{event_id}/schedule/{item_id}",
            post(handlers::errors::not_found),
        )
        .route(
            "/events/{event_id}/schedule/{item_id}/delete",
            post(handlers::errors::not_found),
        )
        .route(
            "/events/{event_id}/attendance/{person_id}",
            post(handlers::events_admin::update_attendance),
        )
        .route(
            "/events/{event_id}/links",
            post(handlers::events_admin::create_link),
        )
        .route(
            "/events/{event_id}/links/{link_id}/revoke",
            post(handlers::events_admin::revoke_link),
        )
        .route(
            "/events/{event_id}/people/bulk",
            post(handlers::events_admin::bulk_add_people),
        )
        .route("/people", get(handlers::people_admin::page))
        .route("/people/{person_id}", post(handlers::people_admin::update))
        .route(
            "/people/{person_id}/claim-status",
            get(handlers::people_admin::claim_status),
        )
        .route(
            "/people/{person_id}/claim-status/unlink",
            post(handlers::people_admin::force_unlink),
        )
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

async fn state(config: &Config, migrate: bool) -> AppState {
    let store = if migrate {
        Store::connect(&config.database_url).await
    } else {
        Store::connect_existing(&config.database_url).await
    }
    .unwrap_or_else(|e| panic!("failed to open database at {}: {e}", config.database_url));
    let oidc = auth::oidc::OidcRegistry::from_path(&config.oidc_providers_path)
        .await
        .unwrap_or_else(|e| {
            panic!(
                "failed to load OIDC providers from {}: {e}",
                config.oidc_providers_path
            )
        });
    // A brand-new database has no owner until the bootstrap signup. Once
    // present, primary-account ambiguity fails startup closed.
    let owner_account_id = store
        .find_primary_account()
        .await
        .unwrap_or_else(|e| panic!("failed to resolve primary account: {e}"));
    AppState::new(store, auth_config(config))
        .with_oidc(oidc)
        .with_public_url(config.public_url.clone())
        .with_owner_account_id(owner_account_id)
}

pub async fn run_site() {
    let config = Config::from_env();
    let app = build_site_router(state(&config, true).await, &config);
    serve(app, &config.bind_addr).await;
}

pub async fn run_admin() {
    let config = Config::from_env();
    let app = build_admin_router(state(&config, false).await, &config);
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
        assert!(body.contains("Ronit Nath"));
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
        // confirm it's the same hash the CSP header allows. Normalize
        // CRLF/CR to LF first — that's what the HTML spec does while
        // tokenizing (and so what a real browser hashes for a CSP
        // hash-source), and what `inline_tag_hash` in security_headers.rs
        // does too; without matching that here, this test would only prove
        // self-consistency with whatever line endings happen to be on disk
        // (see `core.autocrlf` on Windows checkouts), not actual
        // browser-CSP correctness.
        let body = String::from_utf8(body.to_vec()).unwrap();
        let start = body.find("<script>").unwrap() + "<script>".len();
        let end = start + body[start..].find("</script>").unwrap();
        let served_script = body[start..end].replace("\r\n", "\n").replace('\r', "\n");
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
    async fn site_has_guest_login_but_no_signup_route() {
        let (app, _store) = test_site_app().await;
        let (status, _, body) = get(&app, "/login").await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            !String::from_utf8(body.to_vec())
                .unwrap()
                .contains("Sign up")
        );
        let (status, _, _) = get(&app, "/signup").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn site_home_does_not_advertise_auth() {
        let (app, _store) = test_site_app().await;
        let (status, _, body) = get(&app, "/").await;
        assert_eq!(status, StatusCode::OK);
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(!body.contains("/login"));
        assert!(!body.contains("/signup"));
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

    // --- events platform exemplars: the tier-security model ---

    /// Seeds a published event with one public and one private schedule
    /// item, plus a public-tier and a private-tier shareable link.
    /// Returns (event_id, public_token, private_token, public_item_id,
    /// private_item_id).
    async fn seed_event(store: &crate::store::Store) -> (i64, String, String, i64, i64) {
        use crate::auth::session::{generate_token, hash_token};
        use crate::store::events::EventFields;
        use crate::store::schedule_items::ScheduleItemFields;

        let (_, account_id) = store
            .signup_with_password("Host", "host@example.com", "hash")
            .await
            .unwrap();
        let event = store
            .create_event(account_id, "test-party", "Test Party", "2026-07-04 13:00")
            .await
            .unwrap();
        store
            .update_event(
                account_id,
                event.id,
                &EventFields {
                    slug: "test-party".into(),
                    title: "Test Party".into(),
                    tagline: String::new(),
                    starts_at: "2026-07-04 13:00".into(),
                    ends_at: None,
                    timezone: "America/Los_Angeles".into(),
                    status: "published".into(),
                    summary: "A party.".into(),
                    area_name: "Somewhere, SF".into(),
                    address: "1 Secret St".into(),
                    entry_instructions: "SECRET ENTRY CODE".into(),
                    private_details: String::new(),
                    notice_html: "SECRET NOTICE BANNER".into(),
                    quick_plan_html: "SECRET QUICK PLAN".into(),
                },
            )
            .await
            .unwrap();
        let public_item = store
            .create_schedule_item(
                account_id,
                event.id,
                &ScheduleItemFields {
                    sort_order: 0,
                    time_label: "1:00 PM".into(),
                    title: "Board games".into(),
                    detail: String::new(),
                    tier: "public".into(),
                    segment_key: Some("board_games".into()),
                },
            )
            .await
            .unwrap();
        let private_item = store
            .create_schedule_item(
                account_id,
                event.id,
                &ScheduleItemFields {
                    sort_order: 1,
                    time_label: "11:00 PM".into(),
                    title: "PRIVATE ROOFTOP BLOCK".into(),
                    detail: String::new(),
                    tier: "private".into(),
                    segment_key: Some("rooftop".into()),
                },
            )
            .await
            .unwrap();

        let public_token = generate_token();
        store
            .create_event_link(
                account_id,
                event.id,
                None,
                &hash_token(&public_token),
                &public_token,
                "share",
                "public",
            )
            .await
            .unwrap();
        let private_token = generate_token();
        store
            .create_event_link(
                account_id,
                event.id,
                None,
                &hash_token(&private_token),
                &private_token,
                "trusted",
                "private",
            )
            .await
            .unwrap();

        (
            event.id,
            public_token,
            private_token,
            public_item,
            private_item,
        )
    }

    #[tokio::test]
    async fn public_link_hides_private_tier_and_private_link_shows_it() {
        let (app, store) = test_site_app().await;
        let (_, public_token, private_token, _, _) = seed_event(&store).await;

        // Public link: page renders, private-tier info absent.
        let (status, _, body) = get(&app, &format!("/e/{public_token}")).await;
        assert_eq!(status, StatusCode::OK);
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("Test Party"));
        assert!(body.contains("Board games"));
        assert!(
            !body.contains("1 Secret St"),
            "public link must not leak the address"
        );
        assert!(
            !body.contains("SECRET ENTRY CODE"),
            "public link must not leak entry instructions"
        );
        assert!(
            !body.contains("PRIVATE ROOFTOP BLOCK"),
            "public link must not leak private schedule items"
        );
        assert!(
            !body.contains("SECRET NOTICE BANNER") && !body.contains("SECRET QUICK PLAN"),
            "public link must not leak invite content (it carries contact info)"
        );

        // Same property over the JSON view.
        let (status, _, body) = get(&app, &format!("/api/e/{public_token}")).await;
        assert_eq!(status, StatusCode::OK);
        let view: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(view["event"]["address"].is_null());
        assert!(view["event"]["entry_instructions"].is_null());

        // Private link: everything renders.
        let (status, _, body) = get(&app, &format!("/e/{private_token}")).await;
        assert_eq!(status, StatusCode::OK);
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("1 Secret St"));
        assert!(body.contains("SECRET ENTRY CODE"));
        assert!(body.contains("PRIVATE ROOFTOP BLOCK"));
        assert!(body.contains("SECRET NOTICE BANNER"));
        assert!(body.contains("SECRET QUICK PLAN"));
    }

    #[tokio::test]
    async fn direct_link_visibility_matrix_redacts_only_at_chokepoints() {
        use crate::access::level::Level;
        use crate::auth::session::hash_token;

        let (app, store) = test_site_app().await;
        let (event_id, public_token, private_token, _, _) = seed_event(&store).await;
        let person = store.create_person(1, "Matrix Guest", "").await.unwrap();
        let person_token = "matrix-person-link";
        store
            .create_event_link(
                1,
                event_id,
                Some(person.id),
                &hash_token(person_token),
                person_token,
                "person",
                "public",
            )
            .await
            .unwrap();
        let policy = store
            .find_audience_policy(1, "event", event_id)
            .await
            .unwrap()
            .unwrap();

        for public in Level::ALL {
            store
                .set_public_level(1, policy.id, public.as_str())
                .await
                .unwrap();

            let (status, _, body) = get(&app, &format!("/e/{public_token}")).await;
            assert_eq!(status, StatusCode::OK);
            let html = String::from_utf8(body.to_vec()).unwrap();
            // Binding amendment: a direct public capability floors Summary.
            assert!(html.contains("Test Party"));
            assert_eq!(html.contains("1 Secret St"), public == Level::Full);
            assert_eq!(html.contains("SECRET ENTRY CODE"), public == Level::Full);
            assert_eq!(
                html.contains("PRIVATE ROOFTOP BLOCK"),
                public == Level::Full
            );

            let (status, _, body) = get(&app, &format!("/e/{person_token}")).await;
            assert_eq!(status, StatusCode::OK);
            let html = String::from_utf8(body.to_vec()).unwrap();
            assert!(html.contains("Matrix Guest") && html.contains("Test Party"));
            assert_eq!(html.contains("1 Secret St"), public == Level::Full);
            assert_eq!(html.contains("SECRET ENTRY CODE"), public == Level::Full);

            let (status, _, body) = get(&app, &format!("/e/{private_token}")).await;
            assert_eq!(status, StatusCode::OK);
            let html = String::from_utf8(body.to_vec()).unwrap();
            assert!(html.contains("Test Party") && html.contains("1 Secret St"));
        }
    }

    #[tokio::test]
    async fn guest_json_redacts_private_segment_ids_at_summary_and_includes_them_at_full() {
        use crate::access::level::Level;
        use crate::auth::session::hash_token;

        let (app, store) = test_site_app().await;
        let (event_id, public_token, _, public_item, private_item) = seed_event(&store).await;
        let person = store.create_person(1, "Segment Guest", "").await.unwrap();
        store
            .upsert_segment_rsvp(1, public_item, person.id, "in")
            .await
            .unwrap();
        store
            .upsert_segment_rsvp(1, private_item, person.id, "maybe")
            .await
            .unwrap();
        let public_person_token = "summary-segment-person";
        let private_person_token = "full-segment-person";
        for (raw, tier) in [
            (public_person_token, "public"),
            (private_person_token, "private"),
        ] {
            store
                .create_event_link(
                    1,
                    event_id,
                    Some(person.id),
                    &hash_token(raw),
                    raw,
                    "person",
                    tier,
                )
                .await
                .unwrap();
        }

        let (status, _, body) = get(&app, &format!("/api/e/{public_token}")).await;
        assert_eq!(status, StatusCode::OK);
        let summary_shared: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            summary_shared["segment_counts"],
            serde_json::json!([{
                "schedule_item_id": public_item,
                "in_count": 1,
                "maybe_count": 0
            }])
        );

        let (status, _, body) = get(&app, &format!("/api/e/{public_person_token}")).await;
        assert_eq!(status, StatusCode::OK);
        let summary_person: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            summary_person["person"]["segments"],
            serde_json::json!([{"schedule_item_id": public_item, "status": "in"}])
        );
        assert_eq!(
            summary_person["schedule"],
            serde_json::json!([{
                "id": public_item,
                "sort_order": 0,
                "time_label": "1:00 PM",
                "title": "Board games",
                "detail": "",
                "tier": "public",
                "segment_key": "board_games"
            }])
        );

        let (status, _, body) = get(&app, &format!("/api/e/{private_person_token}")).await;
        assert_eq!(status, StatusCode::OK);
        let full: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            full["segment_counts"],
            serde_json::json!([
                {"schedule_item_id": public_item, "in_count": 1, "maybe_count": 0},
                {"schedule_item_id": private_item, "in_count": 0, "maybe_count": 1}
            ])
        );
        assert_eq!(
            full["person"]["segments"],
            serde_json::json!([
                {"schedule_item_id": public_item, "status": "in"},
                {"schedule_item_id": private_item, "status": "maybe"}
            ])
        );
        assert_eq!(
            store
                .segment_counts(1, event_id, Level::Summary)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn person_include_hidden_is_hidden_when_browsing_but_links_apply_tier_floors() {
        use crate::access::level::Level;
        use crate::auth::session::hash_token;
        use crate::auth::viewer::Viewer;

        let (app, store) = test_site_app().await;
        let (event_id, _, _, _, _) = seed_event(&store).await;
        let person = store.create_person(1, "Hidden Guest", "").await.unwrap();
        let policy = store
            .find_audience_policy(1, "event", event_id)
            .await
            .unwrap()
            .unwrap();
        store
            .set_person_override(1, policy.id, person.id, Some("include"), Some("hidden"))
            .await
            .unwrap();
        let inputs = store
            .audience_inputs_for_event(1, event_id, Some(person.id))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            inputs
                .level_for(&Viewer::LinkHolder {
                    person_id: Some(person.id),
                    event_id,
                })
                .unwrap(),
            Level::Hidden,
            "include:hidden must suppress the event on browsing surfaces"
        );

        for (raw, tier) in [
            ("hidden-public-link", "public"),
            ("hidden-private-link", "private"),
        ] {
            store
                .create_event_link(
                    1,
                    event_id,
                    Some(person.id),
                    &hash_token(raw),
                    raw,
                    "person",
                    tier,
                )
                .await
                .unwrap();
        }
        let (status, _, body) = get(&app, "/api/e/hidden-public-link").await;
        assert_eq!(status, StatusCode::OK);
        let summary: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(summary["event"]["title"], "Test Party");
        assert!(summary["event"]["address"].is_null());
        let (status, _, body) = get(&app, "/api/e/hidden-private-link").await;
        assert_eq!(status, StatusCode::OK);
        let full: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(full["event"]["address"], "1 Secret St");
    }

    #[tokio::test]
    async fn person_circle_full_grant_beats_summary_public_level_end_to_end() {
        use crate::auth::session::hash_token;

        let (app, store) = test_site_app().await;
        let (event_id, _, _, _, _) = seed_event(&store).await;
        let person = store
            .create_person(1, "Full Circle Guest", "")
            .await
            .unwrap();
        let circle_id = store.create_circle(1, "Inner Circle").await.unwrap();
        assert_eq!(
            store
                .add_circle_member(1, circle_id, person.id)
                .await
                .unwrap(),
            1
        );
        let policy = store
            .find_audience_policy(1, "event", event_id)
            .await
            .unwrap()
            .unwrap();
        store
            .set_public_level(1, policy.id, "summary")
            .await
            .unwrap();
        store
            .set_circle_grant(1, policy.id, circle_id, Some("full"))
            .await
            .unwrap();
        let raw = "circle-full-person-link";
        store
            .create_event_link(
                1,
                event_id,
                Some(person.id),
                &hash_token(raw),
                raw,
                "person",
                "public",
            )
            .await
            .unwrap();

        let (status, _, body) = get(&app, &format!("/api/e/{raw}")).await;
        assert_eq!(status, StatusCode::OK);
        let view: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(view["event"]["address"], "1 Secret St");
        assert!(
            view["schedule"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["tier"] == "private")
        );
    }

    #[tokio::test]
    async fn person_exclude_beats_public_and_circles_while_multi_circle_uses_max() {
        use crate::auth::session::hash_token;

        let (app, store) = test_site_app().await;
        let (event_id, _, _, _, _) = seed_event(&store).await;
        let person = store.create_person(1, "Circle Guest", "").await.unwrap();
        let low = store.create_circle(1, "Acquaintances").await.unwrap();
        let high = store.create_circle(1, "Friends").await.unwrap();
        store.add_circle_member(1, low, person.id).await.unwrap();
        store.add_circle_member(1, high, person.id).await.unwrap();
        let policy = store
            .find_audience_policy(1, "event", event_id)
            .await
            .unwrap()
            .unwrap();
        store
            .set_public_level(1, policy.id, "hidden")
            .await
            .unwrap();
        store
            .set_circle_grant(1, policy.id, low, Some("busy"))
            .await
            .unwrap();
        store
            .set_circle_grant(1, policy.id, high, Some("full"))
            .await
            .unwrap();
        let raw = "circle-guest-link";
        store
            .create_event_link(
                1,
                event_id,
                Some(person.id),
                &hash_token(raw),
                raw,
                "circle",
                "public",
            )
            .await
            .unwrap();

        let (status, _, body) = get(&app, &format!("/e/{raw}")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            String::from_utf8(body.to_vec())
                .unwrap()
                .contains("1 Secret St"),
            "max circle must be Full"
        );

        store.set_public_level(1, policy.id, "full").await.unwrap();
        store
            .set_person_override(1, policy.id, person.id, Some("exclude"), None)
            .await
            .unwrap();
        let (status, _, _) = get(&app, &format!("/e/{raw}")).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "exclude must beat circle, public, and direct-link floor"
        );
    }

    #[tokio::test]
    async fn owner_session_beats_person_exclude_on_public_router() {
        use crate::auth::session::hash_token;

        let store = crate::store::Store::connect_in_memory().await;
        let (identity_id, account_id) = store
            .signup_with_password("Owner", "owner@example.com", "hash")
            .await
            .unwrap();
        let event = store
            .create_event(account_id, "owner-view", "Owner View", "2026-07-04 13:00")
            .await
            .unwrap();
        let fields = crate::store::events::EventFields {
            slug: "owner-view".into(),
            title: "Owner View".into(),
            tagline: String::new(),
            starts_at: "2026-07-04 13:00".into(),
            ends_at: None,
            timezone: "America/Los_Angeles".into(),
            status: "published".into(),
            summary: String::new(),
            area_name: "San Francisco".into(),
            address: "OWNER SECRET ADDRESS".into(),
            entry_instructions: String::new(),
            private_details: String::new(),
            notice_html: String::new(),
            quick_plan_html: String::new(),
        };
        store
            .update_event(account_id, event.id, &fields)
            .await
            .unwrap();
        let person = store
            .create_person(account_id, "Excluded", "")
            .await
            .unwrap();
        let policy = store
            .find_audience_policy(account_id, "event", event.id)
            .await
            .unwrap()
            .unwrap();
        store
            .set_person_override(account_id, policy.id, person.id, Some("exclude"), None)
            .await
            .unwrap();
        let raw = "owner-person-link";
        store
            .create_event_link(
                account_id,
                event.id,
                Some(person.id),
                &hash_token(raw),
                raw,
                "private",
                "public",
            )
            .await
            .unwrap();
        let authed = seed_session(&store, identity_id, account_id).await;
        let config = crate::config::Config::for_tests();
        let state = crate::state::AppState::new(store.clone(), super::test_auth_config(&config))
            .with_owner_account_id(Some(account_id));
        let app = super::build_site_router(state, &config);

        for public in crate::access::level::Level::ALL {
            store
                .set_public_level(account_id, policy.id, public.as_str())
                .await
                .unwrap();
            let (status, _, body) =
                get_with_cookie(&app, &format!("/e/{raw}"), &authed.cookie).await;
            assert_eq!(status, StatusCode::OK);
            assert!(
                String::from_utf8(body.to_vec())
                    .unwrap()
                    .contains("OWNER SECRET ADDRESS"),
                "owner must remain Full when public_level={}",
                public.as_str()
            );
        }
    }

    #[tokio::test]
    async fn shared_link_rsvp_creates_person_with_editable_personal_link() {
        let (app, store) = test_site_app().await;
        let (_, public_token, _, public_item, private_item) = seed_event(&store).await;

        // RSVP through the shared public link → person created, personal
        // link minted and returned.
        let submit = serde_json::json!({
            "name": "Guest A",
            "status": "going",
            "party_size": 2,
            "note": "no nuts",
            "segments": [{"schedule_item_id": public_item, "status": "in"}],
        });
        let addr = std::net::IpAddr::V4(std::net::Ipv4Addr::new(198, 51, 100, 8));
        let (status, _, body) =
            post_json_from(&app, &format!("/api/e/{public_token}/rsvp"), &submit, addr).await;
        assert_eq!(status, StatusCode::OK);
        let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(result["person_name"], "Guest A");
        let personal_url = result["personal_url"]
            .as_str()
            .expect("personal link minted");
        let personal_token = personal_url.rsplit("/e/").next().unwrap().to_string();

        // The personal link greets them and carries their saved answers.
        let (status, _, body) = get(&app, &format!("/api/e/{personal_token}")).await;
        assert_eq!(status, StatusCode::OK);
        let view: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(view["person"]["name"], "Guest A");
        assert_eq!(view["person"]["attendance"]["status"], "going");
        assert_eq!(view["person"]["attendance"]["party_size"], 2);

        // Editing through the personal link updates, not duplicates (no
        // name needed), and a second submit doesn't mint another link.
        let update = serde_json::json!({
            "name": null, "status": "maybe", "party_size": 1, "note": "", "segments": [],
        });
        let (status, _, body) = post_json_from(
            &app,
            &format!("/api/e/{personal_token}/rsvp"),
            &update,
            addr,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(result["personal_url"].is_null());
        let (_, _, body) = get(&app, &format!("/api/e/{personal_token}")).await;
        let view: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(view["person"]["attendance"]["status"], "maybe");

        // A public link cannot RSVP to a private-tier segment it can't see.
        let sneaky = serde_json::json!({
            "name": "Guest B", "status": "going", "party_size": 1, "note": "",
            "segments": [{"schedule_item_id": private_item, "status": "in"}],
        });
        let (status, _, _) =
            post_json_from(&app, &format!("/api/e/{public_token}/rsvp"), &sneaky, addr).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn personalized_link_greets_by_name_and_titles_the_invite() {
        use crate::auth::session::hash_token;
        use crate::store::event_links::personal_token;

        let (app, store) = test_site_app().await;
        let _ = seed_event(&store).await;
        let person = store.create_person(1, "Maya Chen", "").await.unwrap();
        let raw = personal_token("Maya Chen");
        store
            .create_event_link(
                1,
                1,
                Some(person.id),
                &hash_token(&raw),
                &raw,
                "invite",
                "private",
            )
            .await
            .unwrap();

        let (status, _, body) = get(&app, &format!("/e/{raw}")).await;
        assert_eq!(status, StatusCode::OK);
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("<title>Maya Chen's invite</title>"));
        assert!(body.contains("Hi Maya Chen,"));
        assert!(body.contains("you're invited!"));
        // Private tier ⇒ invite content renders.
        assert!(body.contains("SECRET NOTICE BANNER"));
    }

    #[tokio::test]
    async fn segment_flags_survive_a_guest_status_update() {
        let (_app, store) = test_site_app().await;
        let (event_id, _, _, public_item, _) = seed_event(&store).await;
        let person = store.create_person(1, "Payer", "").await.unwrap();
        store
            .upsert_attendance(1, event_id, person.id, "going", 1, "")
            .await
            .unwrap();

        store
            .set_segment_flags(
                1,
                event_id,
                "board_games",
                person.id,
                None,
                Some(true),
                Some(true),
            )
            .await
            .unwrap();

        // A later guest-side segment update must not clobber the admin's
        // paid/attended bookkeeping (the upsert only touches status).
        store
            .upsert_segment_rsvp(1, public_item, person.id, "maybe")
            .await
            .unwrap();

        let rows = store.list_attendance(1, event_id).await.unwrap();
        let payer = rows.iter().find(|r| r.person_name == "Payer").unwrap();
        assert!(
            payer.segments.contains("board_games:maybe paid ✓went"),
            "unexpected segments string: {:?}",
            payer.segments
        );
    }

    #[tokio::test]
    async fn draft_event_is_invisible_through_a_live_link() {
        let (app, store) = test_site_app().await;
        let (event_id, public_token, _, _, _) = seed_event(&store).await;
        let mut event = store.find_event(1, event_id).await.unwrap().unwrap();
        event.status = "draft".into();
        store
            .update_event(
                1,
                event_id,
                &crate::store::events::EventFields {
                    slug: event.slug,
                    title: event.title,
                    tagline: event.tagline,
                    starts_at: event.starts_at,
                    ends_at: event.ends_at,
                    timezone: event.timezone,
                    status: event.status,
                    summary: event.summary,
                    area_name: event.area_name,
                    address: event.address,
                    entry_instructions: event.entry_instructions,
                    private_details: event.private_details,
                    notice_html: event.notice_html,
                    quick_plan_html: event.quick_plan_html,
                },
            )
            .await
            .unwrap();
        let (status, _, _) = get(&app, &format!("/e/{public_token}")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn live_link_exports_tier_redacted_ics() {
        let (app, store) = test_site_app().await;
        let (_, public_token, _, _, _) = seed_event(&store).await;
        let (status, headers, body) = get(&app, &format!("/events/{public_token}/ics")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            headers[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("text/calendar")
        );
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("SUMMARY:Test Party"));
        assert!(body.contains("LOCATION:Somewhere\\, SF"));
        assert!(!body.contains("1 Secret St"));
    }

    #[tokio::test]
    async fn revoked_and_unknown_guest_links_are_indistinguishable_404s() {
        let (app, store) = test_site_app().await;
        let (_, public_token, _, _, _) = seed_event(&store).await;

        let (status, _, _) = get(&app, "/e/not-a-real-token").await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        // Revoke the shared link (link id 1 is the public one from seed).
        let links = {
            // account 1 is the seeded host account
            store.list_event_links(1, 1).await.unwrap()
        };
        let public_link = links.iter().find(|l| l.tier == "public").unwrap();
        store.revoke_event_link(1, public_link.id).await.unwrap();
        let (status, _, _) = get(&app, &format!("/e/{public_token}")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn circle_member_cross_account_ids_return_404_without_auditing() {
        let (app, store) = test_app().await;
        let owner = signup(&app, "Owner", "circle-owner@example.com", "password123").await;
        let owner_account = authed_account(&store, "circle-owner@example.com").await;
        let owner_circle = store
            .create_circle(owner_account, "Owner Circle")
            .await
            .unwrap();
        let owner_person = store
            .create_person(owner_account, "Owner Person", "")
            .await
            .unwrap();

        signup(&app, "Other", "circle-other@example.com", "password123").await;
        let other_account = authed_account(&store, "circle-other@example.com").await;
        let other_circle = store
            .create_circle(other_account, "Other Circle")
            .await
            .unwrap();
        let other_person = store
            .create_person(other_account, "Other Person", "")
            .await
            .unwrap();

        let added_before = store
            .count_audit_events("circle.member_added")
            .await
            .unwrap();
        for (circle_id, person_id) in [
            (other_circle, owner_person.id),
            (owner_circle, other_person.id),
        ] {
            let form = format!("person_id={person_id}&csrf_token={}", owner.csrf_token);
            let (status, _, _) = post_form_with_cookie(
                &app,
                &format!("/circles/{circle_id}/members"),
                &form,
                Some(&owner.cookie),
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND);
        }
        assert_eq!(
            store
                .count_audit_events("circle.member_added")
                .await
                .unwrap(),
            added_before
        );

        let removed_before = store
            .count_audit_events("circle.member_removed")
            .await
            .unwrap();
        let form = format!("csrf_token={}", owner.csrf_token);
        let (status, _, _) = post_form_with_cookie(
            &app,
            &format!("/circles/{owner_circle}/members/{}/delete", other_person.id),
            &form,
            Some(&owner.cookie),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(
            store
                .count_audit_events("circle.member_removed")
                .await
                .unwrap(),
            removed_before
        );
    }

    #[tokio::test]
    async fn circle_and_audience_admin_routes_require_csrf_and_persist() {
        let (app, store) = test_app().await;
        let Authed { cookie, csrf_token } =
            signup(&app, "Admin", "admin@example.com", "password123").await;
        let account_id = authed_account(&store, "admin@example.com").await;
        let person = store.create_person(account_id, "Guest", "").await.unwrap();
        let event = store
            .create_event(account_id, "audience-ui", "Audience UI", "2026-07-04 18:00")
            .await
            .unwrap();

        let (status, _, _) =
            post_form_with_cookie(&app, "/circles", "name=Friends", Some(&cookie)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let form = format!("name=Friends&csrf_token={csrf_token}");
        let (status, _, _) = post_form_with_cookie(&app, "/circles", &form, Some(&cookie)).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let circle = store
            .find_circle_by_name(account_id, "Friends")
            .await
            .unwrap()
            .unwrap();

        let form = format!("person_id={}&csrf_token={csrf_token}", person.id);
        let (status, _, _) = post_form_with_cookie(
            &app,
            &format!("/circles/{}/members", circle.id),
            &form,
            Some(&cookie),
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER);

        let form = format!(
            "public_level=busy&circle_{}=full&person_{}=exclude&csrf_token={csrf_token}",
            circle.id, person.id
        );
        let (status, _, _) = post_form_with_cookie(
            &app,
            &format!("/events/{}/audience", event.id),
            &form,
            Some(&cookie),
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let inputs = store
            .audience_inputs_for_event(account_id, event.id, Some(person.id))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(inputs.policy.public_level, "busy");
        assert_eq!(inputs.circle_grants[0].level, "full");
        assert_eq!(inputs.overrides[0].override_kind, "exclude");

        let (status, _, body) =
            get_with_cookie(&app, &format!("/events/{}/audience", event.id), &cookie).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            String::from_utf8(body.to_vec())
                .unwrap()
                .contains("Audience visibility")
        );
    }

    #[tokio::test]
    async fn admin_event_pages_require_login() {
        let (app, _store) = test_app().await;
        for path in ["/events", "/events/1", "/people"] {
            let (status, headers, _) = get(&app, path).await;
            assert_eq!(
                status,
                StatusCode::SEE_OTHER,
                "{path} should redirect anonymous visitors"
            );
            assert!(
                headers
                    .get(header::LOCATION)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("/login"),
                "{path} should redirect to /login"
            );
        }
    }

    #[tokio::test]
    async fn connect_existing_errors_on_unmigrated_db() {
        let path = std::env::temp_dir().join(format!(
            "events-unmigrated-{}.db",
            crate::auth::session::generate_token()
        ));
        std::fs::File::create(&path).unwrap();
        let url = format!("sqlite:{}", path.display());
        let err = match crate::store::Store::connect_existing(&url).await {
            Ok(_) => panic!("connect_existing should reject an unmigrated db"),
            Err(err) => err,
        };
        let _ = std::fs::remove_file(&path);
        assert!(
            err.to_string().contains("_sqlx_migrations")
                || err.to_string().contains("no applied migrations"),
            "unexpected error: {err:#}"
        );
    }

    async fn authed_account(store: &crate::store::Store, email: &str) -> i64 {
        let factor = store
            .find_factor_by_external("password", email)
            .await
            .unwrap()
            .unwrap();
        store
            .find_primary_membership(factor.identity_id)
            .await
            .unwrap()
            .unwrap()
            .account_id
    }

    async fn seed_admin_overview(store: &crate::store::Store, account_id: i64) -> (i64, i64, i64) {
        use crate::auth::session::{generate_token, hash_token};
        use crate::store::schedule_items::ScheduleItemFields;

        let event = store
            .create_event(account_id, "overview", "Overview Party", "2026-07-04 18:00")
            .await
            .unwrap();
        let dinner = store
            .create_schedule_item(
                account_id,
                event.id,
                &ScheduleItemFields {
                    sort_order: 1,
                    time_label: "7:00 PM".into(),
                    title: "Dinner".into(),
                    detail: String::new(),
                    tier: "private".into(),
                    segment_key: Some("dinner".into()),
                },
            )
            .await
            .unwrap();
        let person = store
            .create_person(account_id, "Guest A", "crew")
            .await
            .unwrap();
        store
            .upsert_attendance(account_id, event.id, person.id, "going", 2, "no nuts")
            .await
            .unwrap();
        store
            .upsert_segment_rsvp(account_id, dinner, person.id, "in")
            .await
            .unwrap();
        let token = generate_token();
        store
            .create_event_link(
                account_id,
                event.id,
                Some(person.id),
                &hash_token(&token),
                &token,
                "VIP",
                "private",
            )
            .await
            .unwrap();
        store
            .resolve_event_link(&hash_token(&token))
            .await
            .unwrap()
            .unwrap();
        (event.id, person.id, dinner)
    }

    #[tokio::test]
    async fn admin_overview_renders_segments_link_activity_and_qr() {
        let (app, store) = test_app().await;
        let Authed { cookie, .. } = signup(&app, "Admin", "admin@example.com", "password123").await;
        let account_id = authed_account(&store, "admin@example.com").await;
        let (event_id, _, _) = seed_admin_overview(&store, account_id).await;

        let (status, _, body) =
            get_with_cookie(&app, &format!("/events/{event_id}"), &cookie).await;
        assert_eq!(status, StatusCode::OK);
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("Dinner"));
        assert!(body.contains("segment-in-count\">1"));
        assert!(body.contains("VIP"));
        assert!(body.contains("link-uses\">1"));
        assert!(body.contains("link-last-used"));
        assert!(body.contains("<svg"), "overview should inline a QR SVG");
    }

    #[tokio::test]
    async fn person_nickname_update_persists() {
        let (app, store) = test_app().await;
        let Authed { cookie, csrf_token } =
            signup(&app, "Admin", "admin@example.com", "password123").await;
        let account_id = authed_account(&store, "admin@example.com").await;
        let person = store.create_person(account_id, "GuestA", "").await.unwrap();

        let form = format!("name=GuestA&nickname=Ace&return_to=/people&csrf_token={csrf_token}");
        let (status, _, _) = post_form_with_cookie(
            &app,
            &format!("/people/{}", person.id),
            &form,
            Some(&cookie),
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let updated = store
            .find_person(account_id, person.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.nickname, "Ace");
    }

    #[tokio::test]
    async fn admin_attendance_override_persists() {
        let (app, store) = test_app().await;
        let Authed { cookie, csrf_token } =
            signup(&app, "Admin", "admin@example.com", "password123").await;
        let account_id = authed_account(&store, "admin@example.com").await;
        let event = store
            .create_event(account_id, "override", "Override Party", "2026-07-04 18:00")
            .await
            .unwrap();
        let person = store.create_person(account_id, "GuestA", "").await.unwrap();

        let form = format!("status=maybe&party_size=3&csrf_token={csrf_token}");
        let (status, _, _) = post_form_with_cookie(
            &app,
            &format!("/events/{}/attendance/{}", event.id, person.id),
            &form,
            Some(&cookie),
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let attendance = store
            .find_attendance(account_id, event.id, person.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(attendance.status, "maybe");
        assert_eq!(attendance.party_size, 3);
    }

    #[tokio::test]
    async fn deleted_event_edit_and_schedule_routes_404() {
        let (app, store) = test_app().await;
        let Authed { cookie, csrf_token } =
            signup(&app, "Admin", "admin@example.com", "password123").await;
        let account_id = authed_account(&store, "admin@example.com").await;
        let event = store
            .create_event(
                account_id,
                "deleted-routes",
                "Deleted Routes",
                "2026-07-04 18:00",
            )
            .await
            .unwrap();
        let form = format!("csrf_token={csrf_token}");
        for path in [
            format!("/events/{}", event.id),
            format!("/events/{}/schedule", event.id),
            format!("/events/{}/schedule/999", event.id),
            format!("/events/{}/schedule/999/delete", event.id),
        ] {
            let (status, _, _) = post_form_with_cookie(&app, &path, &form, Some(&cookie)).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path} should be deleted");
        }
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

    async fn phase4_fixture() -> (axum::Router, crate::store::Store, i64, i64, String, i64) {
        use crate::auth::session::hash_token;
        use crate::store::events::EventFields;

        let store = crate::store::Store::connect_in_memory().await;
        let (event_id, _, _, _, _) = seed_event(&store).await;
        let event = store.find_event(1, event_id).await.unwrap().unwrap();
        store
            .update_event(
                1,
                event_id,
                &EventFields {
                    slug: event.slug,
                    title: event.title,
                    tagline: event.tagline,
                    starts_at: "2099-07-04 13:00".into(),
                    ends_at: event.ends_at,
                    timezone: event.timezone,
                    status: event.status,
                    summary: event.summary,
                    area_name: event.area_name,
                    address: event.address,
                    entry_instructions: event.entry_instructions,
                    private_details: event.private_details,
                    notice_html: event.notice_html,
                    quick_plan_html: event.quick_plan_html,
                },
            )
            .await
            .unwrap();
        let person = store.create_person(1, "Maya Guest", "").await.unwrap();
        let policy = store
            .find_audience_policy(1, "event", event_id)
            .await
            .unwrap()
            .unwrap();
        store
            .set_person_override(1, policy.id, person.id, Some("include"), Some("full"))
            .await
            .unwrap();
        let raw = "maya-phase4-link".to_string();
        let link_id = store
            .create_event_link(
                1,
                event_id,
                Some(person.id),
                &hash_token(&raw),
                &raw,
                "Maya",
                "private",
            )
            .await
            .unwrap();
        let config = crate::config::Config::for_tests();
        let state = crate::state::AppState::new(store.clone(), super::test_auth_config(&config))
            .with_owner_account_id(Some(1));
        (
            super::build_site_router(state, &config),
            store,
            event_id,
            person.id,
            raw,
            link_id,
        )
    }

    async fn claim_guest(app: &axum::Router, raw: &str, email: &str) -> Authed {
        let form = format!(
            "password=password123&password_confirm=password123&recovery_email={}&csrf_token=",
            email.replace('@', "%40")
        );
        let (status, headers, _) = post_form(app, &format!("/e/{raw}/claim"), &form).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let cookie = extract_cookie(&headers, "session");
        let (status, _, body) = get_with_cookie(app, "/my", &cookie).await;
        assert_eq!(status, StatusCode::OK);
        Authed {
            cookie,
            csrf_token: crate::test_util::extract_csrf_token(&body),
        }
    }

    #[tokio::test]
    async fn guest_claim_logout_login_session_rsvp_and_link_revocation_roundtrip() {
        use crate::auth::session::hash_token;
        let (app, store, event_id, person_id, raw, link_id) = phase4_fixture().await;
        assert_eq!(
            get(&app, &format!("/e/{raw}/claim")).await.0,
            StatusCode::OK
        );
        let guest = claim_guest(&app, &raw, "maya@example.com").await;
        let (_, _, my_body) = get_with_cookie(&app, "/my", &guest.cookie).await;
        assert!(
            String::from_utf8(my_body.to_vec())
                .unwrap()
                .contains("Test Party")
        );

        let rsvp = serde_json::json!({"name": null, "status": "going", "party_size": 2, "note": "session", "segments": []});
        assert_eq!(
            post_json_authed(
                &app,
                &format!("/api/my/events/{event_id}/rsvp"),
                &rsvp,
                &guest.cookie,
                None
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            post_json_authed(
                &app,
                &format!("/api/my/events/{event_id}/rsvp"),
                &rsvp,
                &guest.cookie,
                Some(&guest.csrf_token)
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(
            store
                .find_attendance(1, event_id, person_id)
                .await
                .unwrap()
                .unwrap()
                .party_size,
            2
        );

        let logout = format!("csrf_token={}", guest.csrf_token);
        assert_eq!(
            post_form_with_cookie(&app, "/logout", &logout, Some(&guest.cookie))
                .await
                .0,
            StatusCode::SEE_OTHER
        );
        let (status, headers, _) = post_form(
            &app,
            "/login",
            "email=maya%40example.com&password=password123",
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let login_cookie = extract_cookie(&headers, "session");
        assert_eq!(
            get_with_cookie(&app, "/my", &login_cookie).await.0,
            StatusCode::OK
        );

        store.revoke_event_link(1, link_id).await.unwrap();
        assert_eq!(
            get(&app, &format!("/e/{raw}")).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            post_form(
                &app,
                "/login",
                "email=maya%40example.com&password=password123"
            )
            .await
            .0,
            StatusCode::SEE_OTHER,
            "revoking the claim link must not revoke password login"
        );
        let remint = "maya-reminted-link";
        store
            .create_event_link(
                1,
                event_id,
                Some(person_id),
                &hash_token(remint),
                remint,
                "remint",
                "private",
            )
            .await
            .unwrap();
        assert_eq!(get(&app, &format!("/e/{remint}")).await.0, StatusCode::OK);
        assert_eq!(
            get(&app, &format!("/e/{remint}/claim")).await.0,
            StatusCode::NOT_FOUND,
            "already-claimed links use the documented 404 ruling"
        );
    }

    #[tokio::test]
    async fn claim_parity_mismatch_banner_and_sessioned_claim_csrf() {
        use crate::auth::session::hash_token;
        let (app, store, event_id, _, raw, _) = phase4_fixture().await;
        let guest = claim_guest(&app, &raw, "maya@example.com").await;
        assert_eq!(
            get(&app, "/e/no-such-link/claim").await.0,
            StatusCode::NOT_FOUND
        );
        let shared = "shared-claim-check";
        store
            .create_event_link(
                1,
                event_id,
                None,
                &hash_token(shared),
                shared,
                "shared",
                "public",
            )
            .await
            .unwrap();
        assert_eq!(
            get(&app, &format!("/e/{shared}/claim")).await.0,
            StatusCode::NOT_FOUND
        );

        let other = store.create_person(1, "Other Guest", "").await.unwrap();
        let other_raw = "other-person-link";
        store
            .create_event_link(
                1,
                event_id,
                Some(other.id),
                &hash_token(other_raw),
                other_raw,
                "other",
                "private",
            )
            .await
            .unwrap();
        let (_, _, body) = get_with_cookie(&app, &format!("/e/{other_raw}"), &guest.cookie).await;
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("Viewing as Other Guest; signed in as Maya Guest"));
        let form = "password=password123&password_confirm=password123&recovery_email=other%40example.com&csrf_token=";
        assert_eq!(
            post_form_with_cookie(
                &app,
                &format!("/e/{other_raw}/claim"),
                form,
                Some(&guest.cookie)
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn force_unlink_allows_reclaim_and_orphans_old_guest_session() {
        let (site, store, _, person_id, raw, _) = phase4_fixture().await;
        let old = claim_guest(&site, &raw, "maya@example.com").await;
        let owner = seed_session(&store, 1, 1).await;
        let config = crate::config::Config::for_tests();
        let admin_state =
            crate::state::AppState::new(store.clone(), super::test_auth_config(&config))
                .with_owner_account_id(Some(1));
        let admin = super::build_admin_router(admin_state, &config);
        let form = format!("csrf_token={}", owner.csrf_token);
        assert_eq!(
            post_form_with_cookie(
                &admin,
                &format!("/people/{person_id}/claim-status/unlink"),
                &form,
                Some(&owner.cookie)
            )
            .await
            .0,
            StatusCode::SEE_OTHER
        );
        assert_eq!(
            get_with_cookie(&site, "/my", &old.cookie).await.0,
            StatusCode::SEE_OTHER
        );
        assert_eq!(
            get(&site, &format!("/e/{raw}/claim")).await.0,
            StatusCode::OK
        );
        let reclaimed = claim_guest(&site, &raw, "maya-new@example.com").await;
        assert_eq!(
            get_with_cookie(&site, "/my", &reclaimed.cookie).await.0,
            StatusCode::OK
        );
        assert_eq!(
            store
                .claim_status(1, person_id)
                .await
                .unwrap()
                .unwrap()
                .factor_count,
            1
        );
    }

    #[tokio::test]
    async fn guest_login_unknown_identifier_is_audited_and_guest_scope_is_owner_scoped() {
        let (app, store, _, _, raw, _) = phase4_fixture().await;
        let before = store
            .count_audit_events("guest.login.failed")
            .await
            .unwrap();
        let dummy_before = crate::auth::password::dummy_verify_count();
        assert_eq!(
            post_form(
                &app,
                "/login",
                "email=unknown%40example.com&password=password123"
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
        assert!(crate::auth::password::dummy_verify_count() > dummy_before,
            "unknown guest identifiers must execute the dummy Argon2 verify");
        assert_eq!(
            store
                .count_audit_events("guest.login.failed")
                .await
                .unwrap(),
            before + 1
        );

        // An active binding belonging to a different account cannot become a
        // GuestScope for this site's configured owner account.
        let (_, other_account) = store
            .signup_with_password("Other", "other@example.com", "hash")
            .await
            .unwrap();
        let other_person = store
            .create_person(other_account, "Cross Account", "")
            .await
            .unwrap();
        let password_hash = crate::auth::password::hash("password123").unwrap();
        let raw_session = crate::auth::session::generate_token();
        store
            .claim_guest(
                other_account,
                other_person.id,
                "Cross Account",
                Some("cross@example.com"),
                &password_hash,
                &crate::auth::session::hash_token(&raw_session),
                "csrf",
                "9999-01-01 00:00:00",
                None,
                None,
            )
            .await
            .unwrap();
        let cookie = format!("session={raw_session}");
        assert_eq!(
            get_with_cookie(&app, "/my", &cookie).await.0,
            StatusCode::SEE_OTHER
        );
        assert_eq!(
            get(&app, &format!("/e/{raw}/claim")).await.0,
            StatusCode::OK
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
