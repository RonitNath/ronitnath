#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    ronitnath::telemetry::init();
    ronitnath::app::run_site().await;
}
