//! OpenAPI document root.
//!
//! Paths and schemas are collected automatically as handlers are registered
//! via `OpenApiRouter::routes(routes!(...))` in [`crate::app`] — this struct
//! only needs to supply the top-level document info.

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(info(title = "stage_2", description = "Demo API surface for the stage_2 hardened template"))]
pub struct ApiDoc;
