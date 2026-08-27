//! Static assets: the as-is tree under `static/` and each bundle's `dist/`.
//!
//! Release builds embed both with `rust-embed`, so a node serves its whole
//! frontend out of one binary and a half-copied image cannot produce a page
//! that half-works. Dev reads the same paths from disk, so editing a stylesheet
//! is a reload rather than a rebuild.
//!
//! One deliberate exception: `static/stars/lod/**` is 49 MB of regional Gaia
//! catalog, fetched by byte range and only by a deep-zoom client. Embedding it
//! would put 49 MB into every binary and every layer of the image to serve a
//! file nothing on the landing page requests. It stays on disk under
//! `static_dir` in both modes, and 404s if the deployment did not ship it.

use std::path::{Component, Path, PathBuf};

use axum::Router;
use axum::body::Body;
use axum::extract::{Path as UrlPath, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use rust_embed::Embed;

use crate::config::Mode;
use crate::state::AppState;

#[derive(Embed)]
#[folder = "../../static"]
#[exclude = "stars/lod/*"]
struct StaticFiles;

#[derive(Embed)]
#[folder = "../starscape/dist"]
struct StarscapeBundle;

#[derive(Embed)]
#[folder = "../app-member/dist"]
struct MemberBundle;

#[derive(Embed)]
#[folder = "../app-org/dist"]
struct OrgBundle;

#[derive(Embed)]
#[folder = "../app-platform/dist"]
struct PlatformBundle;

/// Which embedded set — and which on-disk directory — a URL prefix maps to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tree {
    Static,
    Starscape,
    Member,
    Org,
    Platform,
}

impl Tree {
    fn embedded(self, path: &str) -> Option<rust_embed::EmbeddedFile> {
        match self {
            Self::Static => StaticFiles::get(path),
            Self::Starscape => StarscapeBundle::get(path),
            Self::Member => MemberBundle::get(path),
            Self::Org => OrgBundle::get(path),
            Self::Platform => PlatformBundle::get(path),
        }
    }

    /// Where the same tree lives on disk. Bundle output is relative to the
    /// workspace root, which is the directory `cargo run` and `trunk` share.
    fn on_disk(self, state: &AppState) -> PathBuf {
        match self {
            Self::Static => state.config.static_dir.clone(),
            Self::Starscape => PathBuf::from("crates/starscape/dist"),
            Self::Member => PathBuf::from("crates/app-member/dist"),
            Self::Org => PathBuf::from("crates/app-org/dist"),
            Self::Platform => PathBuf::from("crates/app-platform/dist"),
        }
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/favicon.ico", get(favicon))
        .route("/static/{*path}", get(serve(Tree::Static)))
        .route("/pkg/starscape/{*path}", get(serve(Tree::Starscape)))
        .route("/app/pkg/{*path}", get(serve(Tree::Member)))
        .route("/org/pkg/{*path}", get(serve(Tree::Org)))
        .route("/platform/pkg/{*path}", get(serve(Tree::Platform)))
}

fn serve(
    tree: Tree,
) -> impl Fn(State<AppState>, UrlPath<String>) -> std::future::Ready<Response> + Clone {
    move |State(state), UrlPath(path)| std::future::ready(respond(tree, &state, &path))
}

async fn favicon(State(state): State<AppState>) -> Response {
    respond(Tree::Static, &state, "favicon.ico")
}

fn respond(tree: Tree, state: &AppState, path: &str) -> Response {
    let Some(path) = sanitize(path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let content_type = content_type(&path);

    // Dev reads disk first so an edit is visible on reload; release reads the
    // embedded copy first and falls back to disk for the excluded LOD tree.
    let disk = || std::fs::read(tree.on_disk(state).join(&path)).ok();
    let embedded = || tree.embedded(&path).map(|file| file.data.into_owned());
    let bytes = if state.config.mode == Mode::Dev {
        disk().or_else(embedded)
    } else {
        embedded().or_else(disk)
    };

    match bytes {
        Some(bytes) => ([(header::CONTENT_TYPE, content_type)], Body::from(bytes)).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Reject anything that is not a plain relative path. axum already refuses a
/// literal `..` segment, but the disk arm joins onto a configured directory and
/// deserves its own guard rather than trust in a router detail.
fn sanitize(path: &str) -> Option<String> {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        return None;
    }
    let candidate = Path::new(path);
    candidate
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
        .then(|| path.to_string())
}

/// Content types for what this site actually ships. An unknown extension is
/// `application/octet-stream`: guessing is how a `.bin` becomes `text/plain`
/// and arrives at the browser re-encoded.
#[must_use]
pub fn content_type(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, ext)| ext) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("ico") => "image/x-icon",
        Some("ttf") => "font/ttf",
        Some("woff2") => "font/woff2",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_extension_has_a_content_type_a_browser_accepts() {
        assert_eq!(content_type("pkg/rn-starscape_bg.wasm"), "application/wasm");
        assert_eq!(content_type("stars/named.json"), "application/json");
        assert_eq!(content_type("sky/milkyway.webp"), "image/webp");
        assert_eq!(content_type("fonts/agency-bold.ttf"), "font/ttf");
        assert_eq!(content_type("tokens.css"), "text/css; charset=utf-8");
    }

    #[test]
    fn an_unknown_extension_is_never_guessed_into_a_text_type() {
        assert_eq!(content_type("stars/bright.bin"), "application/octet-stream");
        assert_eq!(content_type("NOTICE"), "application/octet-stream");
    }

    #[test]
    fn traversal_and_absolute_paths_never_reach_the_filesystem() {
        assert_eq!(
            sanitize("stars/bright.bin").as_deref(),
            Some("stars/bright.bin")
        );
        assert_eq!(
            sanitize("/stars/bright.bin").as_deref(),
            Some("stars/bright.bin")
        );
        assert!(sanitize("../../etc/passwd").is_none());
        assert!(sanitize("stars/../../secret").is_none());
        assert!(sanitize("").is_none());
        assert!(sanitize("/").is_none());
    }

    #[test]
    fn the_landing_assets_are_embedded_and_the_lod_catalog_deliberately_is_not() {
        assert!(StaticFiles::get("stars/bright.bin").is_some());
        assert!(StaticFiles::get("cities/cities.bin").is_some());
        assert!(StaticFiles::get("sky/milkyway.webp").is_some());
        assert!(
            StaticFiles::get("stars/lod/g12.bin").is_none(),
            "49 MB of regional catalog must not be compiled into the binary"
        );
    }
}
