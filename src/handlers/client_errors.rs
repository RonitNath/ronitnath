//! Receives client-side JS errors so they land in the same log as everything
//! else — see `ts/src/lib/beacon.ts` for the sender.
//!
//! No auth or rate limiting: fine for a dev template, but add both before
//! putting this behind a public internet-facing deployment.

use axum::Json;
use axum::http::StatusCode;
use serde::Deserialize;
use ts_rs::TS;
use utoipa::ToSchema;

const MAX_FIELD_LEN: usize = 2000;

#[derive(Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct ClientErrorReport {
    pub message: String,
    pub source: String,
    pub line: u32,
    pub col: u32,
    pub stack: String,
}

fn truncate(s: &str) -> &str {
    match s.char_indices().nth(MAX_FIELD_LEN) {
        Some((byte_index, _)) => &s[..byte_index],
        None => s,
    }
}

#[utoipa::path(
    post,
    path = "/api/client-errors",
    tag = "observability",
    request_body = ClientErrorReport,
    responses((status = 204, description = "Error recorded"))
)]
pub async fn report(Json(report): Json<ClientErrorReport>) -> StatusCode {
    // No `target:` override here — it must stay under the crate module
    // path so the default `RUST_LOG` filter (`ronitnath=debug,...`) doesn't
    // silently drop it. Grep the log for "client error" to find these.
    tracing::warn!(
        message = truncate(&report.message),
        source = truncate(&report.source),
        line = report.line,
        col = report.col,
        stack = truncate(&report.stack),
        "client error"
    );
    StatusCode::NO_CONTENT
}
