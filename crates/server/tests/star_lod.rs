//! The regional Gaia catalog over HTTP: on disk, by byte range, never embedded.
//!
//! This is the one asset the site serves differently from every other, and the
//! difference is load-bearing: the deep-zoom explorer asks for a few kilobytes
//! at a manifest-named offset, and a server that answered 200 with the whole
//! 49 MB file — or that had it compiled into the binary — would turn one sky
//! tile into a page-stalling download. Its own test binary, and its own node,
//! so it does not contend with the route matrix for a raft port.

use std::path::PathBuf;

use axum::http::StatusCode;
use axum_test::TestServer;
use rn_site::config::{AppConfig, Mode};
use rn_site::db::{self, Migrations, Tuning};
use rn_site::state::AppState;
use tokio::sync::OnceCell;

static NODE: OnceCell<AppState> = OnceCell::const_new();

async fn server() -> TestServer {
    let state = NODE
        .get_or_init(|| async {
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
                "127.0.0.1:8197",
                "127.0.0.1:8297",
                Tuning::fast_single_node(),
                &Migrations::default(),
            )
            .await
            .expect("a single dev node");
            AppState::new(db, config)
        })
        .await
        .clone();
    TestServer::new(rn_site::router(state))
}

fn header(response: &axum_test::TestResponse, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

/// The length of the shipped catalog, read the way the server reads it.
fn catalog_len() -> u64 {
    std::fs::metadata("../../static/stars/lod/g12.bin")
        .expect("the shipped regional catalog")
        .len()
}

#[tokio::test]
async fn the_manifest_is_served_whole_and_advertises_that_ranges_work() {
    let response = server().await.get("/static/stars/lod/manifest.json").await;
    response.assert_status_ok();
    assert_eq!(
        header(&response, "content-type").as_deref(),
        Some("application/json")
    );
    assert_eq!(header(&response, "accept-ranges").as_deref(), Some("bytes"));
    // Stable URL, revalidated — the same policy every other asset gets.
    assert_eq!(
        header(&response, "cache-control").as_deref(),
        Some("no-cache, must-revalidate")
    );
    let manifest: serde_json::Value = response.json();
    assert_eq!(manifest["version"], 1);
    assert_eq!(manifest["recordBytes"], 16);
    assert_eq!(manifest["tiles"].as_array().expect("tiles").len(), 768);
}

#[tokio::test]
async fn a_tile_arrives_as_a_partial_response_of_exactly_the_bytes_asked_for() {
    let response = server()
        .await
        .get("/static/stars/lod/g12.bin")
        .add_header("range", "bytes=12-4459")
        .await;
    assert_eq!(response.status_code(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        header(&response, "content-range").as_deref(),
        Some(format!("bytes 12-4459/{}", catalog_len()).as_str())
    );
    assert_eq!(header(&response, "accept-ranges").as_deref(), Some("bytes"));
    assert_eq!(
        header(&response, "content-type").as_deref(),
        Some("application/octet-stream")
    );
    let bytes = response.as_bytes();
    assert_eq!(bytes.len(), 4_448);
    // 16-byte records: a tile that does not divide is a tile the client will
    // decode one field out of phase.
    assert_eq!(bytes.len() % 16, 0);
}

#[tokio::test]
async fn a_range_past_the_end_is_refused_and_says_how_long_the_file_is() {
    let len = catalog_len();
    let response = server()
        .await
        .get("/static/stars/lod/g12.bin")
        .add_header("range", format!("bytes={len}-{}", len + 16).as_str())
        .await;
    assert_eq!(response.status_code(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(
        header(&response, "content-range").as_deref(),
        Some(format!("bytes */{len}").as_str())
    );
    assert!(response.as_bytes().is_empty());
}

#[tokio::test]
async fn the_header_of_the_whole_file_is_still_reachable_by_range() {
    let response = server()
        .await
        .get("/static/stars/lod/g12.bin")
        .add_header("range", "bytes=0-11")
        .await;
    assert_eq!(response.status_code(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(&response.as_bytes()[..8], b"GDR3LOD1");
}

#[tokio::test]
async fn a_lod_file_the_deployment_did_not_ship_is_absent_rather_than_empty() {
    server()
        .await
        .get("/static/stars/lod/g13.bin")
        .await
        .assert_status_not_found();
}
