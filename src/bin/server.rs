use rinha_rust_warp::{get_nats_client, routes::Server};
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn core::error::Error>> {
    // Initialize the logger
    env_logger::init();

    let nats_client = get_nats_client().await?;
    let jetstream_context = async_nats::jetstream::new(nats_client.clone());

    let pg_pool = PgPoolOptions::new()
        .min_connections(1)
        .connect(&std::env::var("DATABASE_URL")?)
        .await
        .expect("Failed to connect to the database");

    let server = Server::new(jetstream_context, pg_pool);

    log::info!("Starting server...");
    server.start().await.expect("Failed to start server");

    Ok(())
}
