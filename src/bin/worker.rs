use rinha_rust_warp::{PaymentProcessor, get_nats_client};
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn core::error::Error>> {
    // Initialize the logger
    env_logger::init();

    let nats_client = get_nats_client().await?;

    let pg_pool = PgPoolOptions::new()
        .min_connections(1)
        .connect(&std::env::var("DATABASE_URL")?)
        .await
        .expect("Failed to connect to the database");

    let payment_processor = PaymentProcessor::new(pg_pool.clone(), nats_client);

    log::info!("Starting payment processor worker...");
    payment_processor.start_from_client().await;

    Ok(())
}
