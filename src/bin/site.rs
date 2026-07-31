#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    ronitnath::telemetry::init("ronitnath-site");
    ronitnath::app::run_site().await;
}
