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

/// The one stylesheet (`docs/rebuild/plan.md` §KDR(D): tokens move to
/// `crates/ui/tokens.css`). The bundles link their own fingerprinted copy that
/// trunk emits from the same file; the askama pages link this one, so a token
/// edit reaches every surface and no surface keeps a private copy.
#[derive(Embed)]
#[folder = "../ui"]
#[include = "tokens.css"]
struct Tokens;

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
    Tokens,
    Starscape,
    Member,
    Org,
    Platform,
}

impl Tree {
    fn embedded(self, path: &str) -> Option<rust_embed::EmbeddedFile> {
        match self {
            Self::Static => StaticFiles::get(path),
            Self::Tokens => Tokens::get(path),
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
            Self::Tokens => PathBuf::from("crates/ui"),
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
        .route("/tokens.css", get(tokens))
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

async fn tokens(State(state): State<AppState>) -> Response {
    respond(Tree::Tokens, &state, "tokens.css")
}

/// Trunk's own `<link>` and `<script>` tags for a bundle, lifted out of the
/// `index.html` it emitted beside them.
///
/// Trunk fingerprints every file it writes, so the shell cannot name them and
/// must not try: the build output is the single statement of what a bundle
/// loads. The fragment taken is everything from the first tag trunk added to
/// the end of the head — the charset, viewport, title and pre-paint theme
/// script above it are the shell's own and are not duplicated.
///
/// An unbuilt bundle yields an empty fragment, which is the honest state: the
/// shell renders, the app does not mount, and `trunk build` is the fix.
#[must_use]
pub fn bundle_head(tree: Tree, state: &AppState) -> String {
    let Some(bytes) = read(tree, state, "index.html") else {
        return String::new();
    };
    let Ok(document) = String::from_utf8(bytes) else {
        return String::new();
    };
    head_fragment(&document).unwrap_or_default()
}

fn head_fragment(document: &str) -> Option<String> {
    let head = document.split_once("<head>")?.1.split_once("</head>")?.0;
    let start = head.find("<link").or_else(|| head.rfind("<script"))?;
    Some(head[start..].trim().to_owned())
}

fn respond(tree: Tree, state: &AppState, path: &str) -> Response {
    let Some(clean) = sanitize(path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let content_type = content_type(&clean);
    match read(tree, state, &clean) {
        Some(bytes) => ([(header::CONTENT_TYPE, content_type)], Body::from(bytes)).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// One file out of a tree, from wherever this mode reads it.
///
/// Dev reads disk first so an edit is visible on reload; release reads the
/// embedded copy first and falls back to disk for the excluded LOD tree.
#[must_use]
pub fn read(tree: Tree, state: &AppState, path: &str) -> Option<Vec<u8>> {
    let path = sanitize(path)?;
    let disk = || std::fs::read(tree.on_disk(state).join(&path)).ok();
    let embedded = || tree.embedded(&path).map(|file| file.data.into_owned());
    if state.config.mode == Mode::Dev {
        disk().or_else(embedded)
    } else {
        embedded().or_else(disk)
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
    fn trunks_own_tags_are_lifted_and_the_shells_own_head_is_not_duplicated() {
        let document = r#"<!doctype html><html><head>
            <meta charset="utf-8" />
            <title>app</title>
            <script>try { localStorage.getItem("rn-theme"); } catch (e) {}</script>
            <link rel="stylesheet" href="/app/pkg/tokens-abc.css"/>
            <script type="module">import init from '/app/pkg/rn-app-abc.js';</script>
            </head><body></body></html>"#;
        let head = head_fragment(document).expect("a head");
        assert!(head.starts_with("<link rel=\"stylesheet\""), "{head}");
        assert!(head.contains("rn-app-abc.js"));
        assert!(!head.contains("<title>"));
        assert!(!head.contains("charset"));
        assert!(
            !head.contains("rn-theme"),
            "the shell writes its own theme script"
        );
    }

    #[test]
    fn an_unbuilt_bundle_yields_nothing_rather_than_half_a_document() {
        assert_eq!(head_fragment("<html><body></body></html>"), None);
        assert_eq!(head_fragment("<head><title>x</title></head>"), None);
    }

    #[test]
    fn the_one_stylesheet_is_the_bundles_and_not_a_second_copy() {
        assert!(Tokens::get("tokens.css").is_some());
        assert!(
            Tokens::get("src/lib.rs").is_none(),
            "only the stylesheet is embedded from the ui crate"
        );
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
