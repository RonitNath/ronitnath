mod app;
mod config;
mod error;
mod handlers;
mod state;
mod telemetry;
mod view;

#[tokio::main]
async fn main() {
    telemetry::init();
    app::run().await;
}
