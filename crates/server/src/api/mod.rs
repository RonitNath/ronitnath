//! `/api/*` — the browser-session surface, and only that.
//!
//! Every route here takes the cookie extractor and nothing else. A bearer link
//! token cannot reach this router: [`Bearer`](crate::links::Bearer) is
//! implemented for the links router's state alone, so a bearer route under
//! `/api` is a compile error rather than a review note, and `bearer_on_api` in
//! the route matrix proves the runtime half — a link token presented here is
//! simply nobody.
//!
//! Three shapes, from `docs/rebuild/plan.md` §API:
//!
//! * `POST /api/cmd/<name>` writes, once, with an idempotency key.
//! * `GET /api/q/<name>` reads, holding a store that cannot write.
//! * `WS /api/sub` pushes the row diffs those writes produced.
//!
//! `GET /api/whoami` is the fourth and is not a query: it is the chrome's DTO,
//! it takes no parameters and it is not subscribable.

pub mod cmd;
pub mod decline;
pub mod origin;
pub mod query;
pub mod whoami;

use axum::extract::{Path, RawQuery, State};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::get};

use crate::auth::session::Session;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(whoami::router())
        .merge(cmd::router())
        .route("/api/q/{name}", get(read))
        .merge(crate::sub::router())
}

async fn read(
    State(state): State<AppState>,
    session: Session,
    Path(name): Path<String>,
    RawQuery(raw): RawQuery,
) -> Response {
    let Some(named) = query::Named::parse(&name) else {
        return decline::not_found();
    };
    let params = query::Params::from_pairs(pairs(raw.as_deref()).into_iter());
    match query::read(&state.store.reads(), named, &session.principal, &params).await {
        Ok(rows) => Json(query::body(&rows)).into_response(),
        Err(error) => decline::from_kernel(&error),
    }
}

/// A query string's pairs, undecoded.
///
/// Every parameter this leg's queries take is an integer, so there is nothing
/// to percent-decode and nothing a decoder could get wrong. A query that needs
/// text takes it as a path segment or grows a decoder with its own tests.
fn pairs(raw: Option<&str>) -> Vec<(&str, &str)> {
    raw.unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| pair.split_once('=').unwrap_or((pair, "")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_string_reads_as_pairs_and_an_absent_one_reads_as_none() {
        assert_eq!(
            pairs(Some("after=40&limit=10")),
            vec![("after", "40"), ("limit", "10")]
        );
        assert_eq!(pairs(Some("flag")), vec![("flag", "")]);
        assert!(pairs(None).is_empty());
        assert!(pairs(Some("")).is_empty());
    }
}
