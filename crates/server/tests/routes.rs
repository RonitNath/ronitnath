//! The HTTP surface, against a real single-node hiqlite cluster.
//!
//! One node is booted for the whole test binary and shared: the readiness
//! route has to answer for an actual raft group to be worth asserting on, and
//! booting one per test would pay hiqlite's formation delay a dozen times over
//! to learn the same thing.

use std::path::PathBuf;

use axum_test::TestServer;
use rn_site::config::{AppConfig, Mode};
use rn_site::db::{self, Migrations, Tuning};
use rn_site::http::VERSION_HEADER;
use rn_site::state::AppState;
use tokio::sync::OnceCell;

static NODE: OnceCell<AppState> = OnceCell::const_new();

async fn state() -> AppState {
    NODE.get_or_init(|| async {
        let directory = Box::leak(Box::new(
            tempfile::tempdir().expect("a temporary data directory"),
        ));
        let config = AppConfig {
            mode: Mode::Dev,
            db_path: directory.path().join("db.sqlite"),
            addr: "127.0.0.1:0".parse().expect("a loopback address"),
            static_dir: PathBuf::from("../../static"),
            id_key: Some("000102030405060708090a0b0c0d0e0f".to_string()),
        };
        let db = db::open_on(
            &config,
            "127.0.0.1:8195",
            "127.0.0.1:8295",
            Tuning::fast_single_node(),
            &Migrations::embedded::<rn_kernel::Migrations>().expect("the kernel's history"),
        )
        .await
        .expect("a single dev node");
        AppState::new(db, config)
    })
    .await
    .clone()
}

async fn server() -> TestServer {
    TestServer::new(rn_site::router(state().await))
}

#[tokio::test]
async fn liveness_answers_without_touching_the_database() {
    let response = server().await.get("/healthz").await;
    response.assert_status_ok();
    response.assert_text("ok");
}

#[tokio::test]
async fn readiness_reports_this_node_and_both_raft_groups() {
    let response = server().await.get("/readyz").await;
    response.assert_status_ok();

    let body: serde_json::Value = response.json();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["node"], "local");
    assert_eq!(body["version"], "dev");
    for group in ["sqlite", "cache"] {
        let raft = &body["raft"][group];
        assert_eq!(raft["healthy"], true, "{group} raft is not healthy");
        assert_eq!(raft["voters"], 1, "{group} raft has the wrong voter count");
        assert_eq!(raft["expected_voters"], 1);
        assert!(raft["leader"].is_number(), "{group} raft has no leader");
    }
}

#[tokio::test]
async fn the_release_identity_is_one_curl_away_from_any_url() {
    let server = server().await;
    server.get("/version").await.assert_text("dev");
    for path in ["/", "/healthz", "/readyz", "/version", "/tokens.css"] {
        let response = server.get(path).await;
        assert_eq!(
            response
                .headers()
                .get(VERSION_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("dev"),
            "{path} did not carry the version header"
        );
    }
}

#[tokio::test]
async fn documents_refuse_to_be_stored_and_assets_revalidate() {
    let server = server().await;
    let cache_control = |response: &axum_test::TestResponse| {
        response
            .headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    };
    for path in ["/", "/healthz", "/readyz", "/version"] {
        let response = server.get(path).await;
        assert_eq!(
            cache_control(&response).as_deref(),
            Some("no-store"),
            "{path}"
        );
    }
    for path in ["/tokens.css", "/favicon.ico"] {
        let response = server.get(path).await;
        assert_eq!(
            cache_control(&response).as_deref(),
            Some("no-cache, must-revalidate"),
            "{path}"
        );
    }
}

#[tokio::test]
async fn a_shipped_asset_is_served_with_its_type_and_anything_else_is_absent() {
    let server = server().await;
    for (path, content_type) in [
        ("/tokens.css", "text/css; charset=utf-8"),
        ("/static/site.css", "text/css; charset=utf-8"),
        ("/static/stars/named.json", "application/json"),
        ("/static/sky/milkyway.webp", "image/webp"),
        ("/favicon.ico", "image/x-icon"),
    ] {
        let response = server.get(path).await;
        response.assert_status_ok();
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some(content_type),
            "{path}"
        );
        assert!(!response.as_bytes().is_empty(), "{path} was empty");
    }

    for missing in [
        "/static/nothing-here.css",
        "/static/stars/nope.bin",
        "/pkg/starscape/nope.js",
        "/app/pkg/nope.js",
        "/org/pkg/nope.js",
        "/platform/pkg/nope.js",
    ] {
        server.get(missing).await.assert_status_not_found();
    }
}

#[tokio::test]
async fn an_asset_path_cannot_climb_out_of_its_tree() {
    let server = server().await;
    for attempt in [
        "/static/..%2f..%2fCargo.toml",
        "/static/%2e%2e/%2e%2e/Cargo.toml",
    ] {
        let response = server.get(attempt).await;
        assert_ne!(
            response.status_code(),
            axum::http::StatusCode::OK,
            "{attempt} was served"
        );
    }
}

#[tokio::test]
async fn the_landing_renders_in_the_theme_the_request_asks_for() {
    let server = server().await;

    let night = server.get("/").await;
    night.assert_status_ok();
    assert!(
        night
            .text()
            .contains(r#"<html lang="en" data-theme="dark">"#)
    );

    let dusk = server
        .get("/")
        .add_header("sec-ch-prefers-color-scheme", "light")
        .await;
    assert!(
        dusk.text()
            .contains(r#"<html lang="en" data-theme="light">"#)
    );

    // An explicit choice outranks what the operating system prefers.
    let chosen = server
        .get("/")
        .add_header("sec-ch-prefers-color-scheme", "light")
        .add_header("cookie", "rn_theme=dark")
        .await;
    assert!(chosen.text().contains(r#"data-theme="dark""#));
}

#[tokio::test]
async fn the_landing_mounts_the_sky_and_publishes_the_clock_it_draws_from() {
    let body = server().await.get("/").await.text();
    assert!(body.contains(r#"<canvas id="starscape""#));
    assert!(body.contains(r#"<canvas id="mini-globe""#));
    assert!(body.contains(r#"id="grounding""#));
    assert!(body.contains("/pkg/starscape/rn-starscape.js"));

    let epoch = body
        .split_once(r#"<meta name="rn-server-epoch-ms" content=""#)
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(value, _)| value.parse::<i64>().expect("a unix millisecond value"))
        .expect("the landing publishes the server clock");
    assert!(
        epoch > 1_700_000_000_000,
        "{epoch} is not a plausible clock"
    );
}

#[tokio::test]
async fn the_landing_carries_no_build_machinery_and_no_explanatory_copy() {
    let body = server().await.get("/").await.text().to_lowercase();
    for leak in [
        "generated",
        "cargo",
        "trunk",
        "wasm-bindgen",
        "target/",
        "todo",
        "lorem",
    ] {
        assert!(!body.contains(leak), "the landing leaks `{leak}`");
    }
    // No cache-busting query strings: asset URLs are stable and revalidated.
    assert!(!body.contains("?v="));
}

/// The routes S2 added are the whole of `/api`, the auth pages, the shells and
/// the claim page; their own matrix is in `surface.rs`. What this leg still
/// owns is the negative half — that nothing outside either leg answers at all.
#[tokio::test]
async fn nothing_either_leg_owns_answers() {
    let server = server().await;
    for absent in ["/manage", "/metrics", "/api/realtime", "/pkg/rn-site.js"] {
        server.get(absent).await.assert_status_not_found();
    }
}
