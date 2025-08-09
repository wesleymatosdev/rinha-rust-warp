pub mod process_payment;
pub mod routes;
use async_nats::Client;

pub async fn get_nats_client() -> Result<Client, Box<dyn std::error::Error>> {
    let nats_url =
        std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());
    let nats_client = async_nats::connect(nats_url).await?;
    Ok(nats_client)
}

// Re-export common types
pub use process_payment::{Payment, PaymentProcessor};
pub use routes::Server;
