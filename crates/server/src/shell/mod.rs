//! `/app`, `/org`, `/platform` — the tier shells.
//!
//! The tier *is* the app (`docs/kernel/index.html` §Surfaces): there is no
//! role-routed `/`, no navigation filtered by capability, and a bundle you
//! cannot hold is never sent. So the check happens here, before a byte of
//! wasm leaves the node, and the three answers are the ones
//! `docs/rebuild/plan.md` §API fixes:
//!
//! | | anonymous | member without the tier | with it |
//! |---|---|---|---|
//! | `/app` | `302 /auth?next=` | — | the shell |
//! | `/org` | `302 /auth?next=` | `404` | the shell |
//! | `/platform` | `404` | `404` | the shell |
//!
//! `/platform` 404s for the anonymous visitor too, and that asymmetry is
//! deliberate: redirecting them to sign in would confirm the route exists,
//! and the audience for that page is one person who knows where it is.
//!
//! ## Where the script tags come from
//!
//! Trunk fingerprints its output (`rn-app-215c02033e75c2bc.js`), so the shell
//! cannot name the files. It reads the `index.html` trunk emitted beside them
//! and re-uses trunk's own tags verbatim — the bundle's build output is the
//! single source of what a bundle loads, and a rebuild that renames everything
//! needs no edit here.

use askama::Template;
use askama_web::WebTemplate;
use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use rn_api::Tier;
use rn_kernel::Principal;

use crate::api::{decline, whoami};
use crate::assets::{self, Tree};
use crate::auth::session::Visitor;
use crate::presence::theme_for;
use crate::state::AppState;

/// One tier's shell: its mount path, its bundle, and how a principal without
/// it is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellRoute {
    /// The tier the principal must hold.
    pub tier: Tier,
    /// The path it is served at.
    pub path: &'static str,
    /// Which bundle's `dist/` the document loads.
    pub tree: Tree,
    /// Whether an anonymous visitor is sent to sign in, or told nothing.
    pub redirects_anonymous: bool,
}

/// The three shells, in tier order.
pub const SHELLS: &[ShellRoute] = &[
    ShellRoute {
        tier: Tier::Member,
        path: "/app",
        tree: Tree::Member,
        redirects_anonymous: true,
    },
    ShellRoute {
        tier: Tier::Org,
        path: "/org",
        tree: Tree::Org,
        redirects_anonymous: true,
    },
    ShellRoute {
        tier: Tier::Platform,
        path: "/platform",
        tree: Tree::Platform,
        redirects_anonymous: false,
    },
];

pub fn router() -> Router<AppState> {
    let mut router = Router::new();
    for shell in SHELLS {
        // The bundle owns its own routing below the mount point, so every
        // sub-path serves the same document and the SPA takes it from there.
        let handler =
            move |state: State<AppState>, visitor: Visitor, headers: HeaderMap, uri: Uri| {
                serve(*shell, state, visitor, headers, uri)
            };
        router = router
            .route(shell.path, get(handler))
            .route(&format!("{}/{{*rest}}", shell.path), get(handler));
    }
    router
}

#[derive(Template, WebTemplate)]
#[template(path = "shell.html")]
struct Shell {
    theme: &'static str,
    version: String,
    /// The tier's name, for the document title and `data-tier`.
    tier: &'static str,
    /// Trunk's own `<link>` and `<script>` tags for this bundle.
    head: String,
}

async fn serve(
    shell: ShellRoute,
    State(state): State<AppState>,
    visitor: Visitor,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let Principal::Member { .. } = visitor.principal else {
        return if shell.redirects_anonymous {
            to_sign_in(&uri)
        } else {
            decline::not_found()
        };
    };

    let tiers = match whoami::tiers_of(&state, &visitor.principal).await {
        Ok(tiers) => tiers,
        Err(error) => return decline::from_kernel(&error),
    };
    if !tiers.contains(&shell.tier) {
        // Authenticated but unauthorised is a 404, not a 403: a 403 would
        // confirm that the tier exists and that this reader is simply not in
        // it, which is the fact worth withholding.
        return decline::not_found();
    }

    Shell {
        theme: theme_for(&headers).as_str(),
        version: state.version.to_string(),
        tier: tier_name(shell.tier),
        head: assets::bundle_head(shell.tree, &state),
    }
    .into_response()
}

const fn tier_name(tier: Tier) -> &'static str {
    match tier {
        Tier::Member => "app",
        Tier::Org => "org",
        Tier::Platform => "platform",
    }
}

/// Send an anonymous visitor to sign in, and back to where they were aiming.
fn to_sign_in(uri: &Uri) -> Response {
    // The path this request actually asked for, which `next` then re-validates
    // — a router match is not a licence to reflect the URI into a header.
    let next = crate::auth::next::validate(Some(uri.path()));
    (
        StatusCode::FOUND,
        [(header::LOCATION, format!("/auth?next={next}"))],
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tier_has_exactly_one_shell_and_one_bundle() {
        assert_eq!(SHELLS.len(), 3);
        for tier in [Tier::Member, Tier::Org, Tier::Platform] {
            assert_eq!(
                SHELLS.iter().filter(|shell| shell.tier == tier).count(),
                1,
                "{tier:?}"
            );
        }
        let mut trees: Vec<Tree> = SHELLS.iter().map(|shell| shell.tree).collect();
        trees.dedup();
        assert_eq!(trees.len(), 3, "two shells share a bundle");
    }

    #[test]
    fn the_platform_shell_tells_an_anonymous_visitor_nothing() {
        let platform = SHELLS
            .iter()
            .find(|shell| shell.tier == Tier::Platform)
            .expect("a platform shell");
        assert!(!platform.redirects_anonymous);
        for open in SHELLS.iter().filter(|shell| shell.tier != Tier::Platform) {
            assert!(open.redirects_anonymous, "{}", open.path);
        }
    }

    #[test]
    fn a_shell_path_is_a_single_rooted_segment() {
        for shell in SHELLS {
            assert!(shell.path.starts_with('/'), "{}", shell.path);
            assert_eq!(shell.path.matches('/').count(), 1, "{}", shell.path);
            assert!(crate::auth::next::is_same_origin_path(shell.path));
        }
    }
}
