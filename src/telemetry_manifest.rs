use telemetry::TelemetryManifest;

/// Product-owned telemetry vocabulary. Keep these bounded and add every
/// generated product route/operation to the application arrays below.
pub const PRODUCT_HTTP_ROUTES: &[&str] = &[];
pub const PRODUCT_DB_OPERATIONS: &[&str] = &[];

const APPLICATION_HTTP_ROUTES: &[&str] = &[
    "/livez",
    "/healthz",
    "/readyz",
    "/metrics",
    "/",
    "/static/{*path}",
];
const APPLICATION_DB_OPERATIONS: &[&str] = &[];

pub const fn manifest() -> TelemetryManifest<'static> {
    let _product_manifest = (PRODUCT_HTTP_ROUTES, PRODUCT_DB_OPERATIONS);
    TelemetryManifest::new(APPLICATION_HTTP_ROUTES, APPLICATION_DB_OPERATIONS)
}
