use rinha_rust_warp::{PaymentProcessor, get_jetstream_context};
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn core::error::Error>> {
    // Initialize the logger
    env_logger::init();

    let jetstream_context = get_jetstream_context().await?;

    let pg_pool = PgPoolOptions::new()
        .min_connections(1)
        .connect(&std::env::var("DATABASE_URL")?)
        .await
        .expect("Failed to connect to the database");

    let payment_processor = PaymentProcessor::new(pg_pool.clone(), jetstream_context.clone());

    log::info!("Starting payment processor worker...");
    payment_processor.start().await;

    Ok(())
}
