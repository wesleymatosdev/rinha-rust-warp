mod nats;
mod process_payment;
mod routes;

use sqlx::postgres::PgPoolOptions;

use crate::routes::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn core::error::Error>> {
    // Initialize the logger
    env_logger::init();
    let nats_client = nats::get_nats_client().await?;
    let jetstream_context = async_nats::jetstream::new(nats_client.clone());

    let pg_pool = PgPoolOptions::new()
        .min_connections(1)
        .connect(&std::env::var("DATABASE_URL")?)
        .await
        .expect("Failed to connect to the database");

    let payment_processor =
        process_payment::PaymentProcessor::new(pg_pool.clone(), jetstream_context.clone());

    tokio::spawn(async move { payment_processor.start().await });

    let server = Server::new(jetstream_context, pg_pool);

    server.start().await.expect("Failed to start server");

    Ok(())
}
