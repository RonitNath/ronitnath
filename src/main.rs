mod app;
mod build_stamp;
mod features;

mod telemetry_manifest;

use std::error::Error;

use http_runtime::{
    RuntimeConfig, acquire_listener, bind_listener, init_tracing, serve_public_and_operator,
};
use telemetry::Metrics;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    init_tracing()?;
    let config = RuntimeConfig::from_env()?;
    let metrics = Metrics::from_manifest(telemetry_manifest::manifest())?;
    let application = app::build(&config, metrics, std::path::Path::new("static/public")).await?;
    let listener = acquire_listener(config.bind_addr()).await?;
    let operator_listener = bind_listener(config.operator_bind_addr()).await?;
    let serve_result = serve_public_and_operator(
        listener,
        application.public_router(),
        operator_listener,
        application.operator_router(),
    )
    .await;
    application.shutdown().await;
    serve_result?;
    Ok(())
}
