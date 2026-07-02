mod app;
mod config;
mod error;
mod handlers;
mod openapi;
mod state;
mod store;
mod telemetry;
mod view;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    telemetry::init();
    app::run().await;
}
