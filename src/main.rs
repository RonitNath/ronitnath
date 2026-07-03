mod app;
mod config;
mod error;
mod handlers;
mod openapi;
mod rate_limit;
mod security_headers;
mod state;
mod store;
mod telemetry;
#[cfg(test)]
mod test_util;
mod view;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    telemetry::init();
    app::run().await;
}
