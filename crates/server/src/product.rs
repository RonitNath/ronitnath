//! The products' routes, and the one gate that makes them appear and vanish.
//!
//! Story 5 is that a platform screen turns a product on and its routes start
//! answering **on every node, without a restart**. Everything hard about that
//! is in one sentence: axum builds its `Router` once at boot
//! ([`crate::router`]). So the routes do not come and go — the *answer* does.
//!
//! Every product's routes are mounted always and wrapped in [`gate`], which
//! reads this node's [`ProductSet`] and answers the uniform `404` when the
//! product is off. A `404` and not a `403`, because a disabled product must be
//! indistinguishable from one this deployment never had: a `403` would confirm
//! the route exists and tell an anonymous caller what the deployment could be
//! doing.
//!
//! ## Why not rebuild the router
//!
//! The other design is an `ArcSwap<Router>` rebuilt per toggle, and it was
//! considered and rejected (requirement B5.4). It buys nothing a caller can
//! observe, and it costs a whole-router clone per toggle plus an in-flight
//! request holding the old tree with the old per-route state — a request that
//! could then be answered by a router the deployment has already replaced. The
//! gate is one atomic load and a shift, it is testable without a router at
//! all, and — because it is applied where the product's routes are merged
//! rather than route by route — a route added inside a product cannot forget
//! it.
//!
//! ## The catalogue is small on purpose
//!
//! One product today (`rn_kernel::product::CATALOGUE`), because a catalogue
//! entry states the routes it mounts and the products screen renders that as
//! "what turning this off will take away". An entry for a feature this
//! deployment has not built would make that sentence false. The mechanism is
//! what this leg owes; the second and third products are entries added beside
//! the first when their features exist.

use std::sync::Arc;

use askama::Template;
use askama_web::WebTemplate;
use axum::Router;
use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{MethodRouter, get};
use rn_kernel::product::{self, Product, ProductSet};

use crate::api::decline;
use crate::presence::theme_for;
use crate::state::AppState;

/// Every product's routes, each behind its own gate.
///
/// The state is taken here rather than at `with_state` time because the gate
/// closes over the projection this process holds — the same `Arc` the feed
/// consumer replaces, so a toggle is visible to a route that was mounted
/// before the deployment had ever heard of it.
#[must_use]
pub fn router(state: &AppState) -> Router<AppState> {
    let mut router = Router::new();
    for (index, product) in product::CATALOGUE.iter().enumerate() {
        let gated = routes(product).route_layer(axum::middleware::from_fn_with_state(
            Gate {
                index,
                products: Arc::clone(&state.products),
            },
            gate,
        ));
        router = router.merge(gated);
    }
    router
}

/// One product's routes — built from the catalogue's own `mounts` list.
///
/// The list the products screen renders and the paths the router serves are
/// therefore the same list rather than two that agree by inspection. A product
/// that declares a path this build has no handler for mounts nothing, and
/// [`every_declared_route_has_a_handler`] fails rather than leaving the screen
/// stating a route that answers 404 whichever way the product is set.
fn routes(product: &'static Product) -> Router<AppState> {
    let mut router = Router::new();
    for path in product.mounts {
        if let Some(handler) = handler(product.slug, path) {
            router = router.route(path, handler);
        }
    }
    router
}

/// What answers one declared route.
fn handler(slug: &str, path: &str) -> Option<MethodRouter<AppState>> {
    match (slug, path) {
        ("starscape", "/sky") => Some(get(sky)),
        _ => None,
    }
}

/// What a gated route needs to know: which bit, and where the bits are.
#[derive(Clone)]
struct Gate {
    index: usize,
    products: Arc<ProductSet>,
}

/// The gate. One atomic load, one shift, one comparison.
///
/// `route_layer`, so it runs only for requests this product's routes actually
/// matched — a path nobody mounted falls through to the rest of the surface
/// and gets its ordinary answer rather than this one.
async fn gate(State(gate): State<Gate>, request: Request, next: Next) -> Response {
    if gate.products.at(gate.index) {
        next.run(request).await
    } else {
        decline::not_found()
    }
}

// ------------------------------------------------------------- starscape ---

#[derive(Template, WebTemplate)]
#[template(path = "sky.html")]
struct Sky {
    theme: &'static str,
    version: String,
    /// The server's clock at render, in unix milliseconds — what makes every
    /// browser on the site draw the same instant.
    epoch_ms: i64,
}

/// `GET /sky` — the landing's sky, with nothing standing in front of it.
async fn sky(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let page = Sky {
        theme: theme_for(&headers).as_str(),
        version: state.version.to_string(),
        epoch_ms: unix_millis(),
    };
    (
        [
            ("accept-ch", "Sec-CH-Prefers-Color-Scheme"),
            ("vary", "Sec-CH-Prefers-Color-Scheme, Cookie"),
        ],
        page,
    )
        .into_response()
}

fn unix_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_declared_route_has_a_handler() {
        for product in product::CATALOGUE {
            for path in product.mounts {
                assert!(
                    handler(product.slug, path).is_some(),
                    "{} declares {path} and this build mounts nothing there",
                    product.slug
                );
            }
        }
    }

    #[test]
    fn a_path_no_product_declares_is_not_mounted_by_one() {
        assert!(handler("starscape", "/not-a-route").is_none());
        assert!(handler("events", "/sky").is_none());
    }

    #[test]
    fn the_gate_reads_the_bit_the_catalogue_gave_the_product() {
        let products = ProductSet::empty();
        let index = product::index_of("starscape").expect("the catalogue carries it");
        assert!(!products.at(index));
        products.replace(1 << index);
        assert!(products.at(index));
    }
}
