use std::error::Error;

use async_nats::Client;
use futures_util::StreamExt;

pub async fn get_nats_client() -> Result<Client, Box<dyn std::error::Error>> {
    let nats_url =
        std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());
    let nats_client = async_nats::connect(nats_url).await?;
    Ok(nats_client)
}

pub async fn register_subscriber<F, Fut>(
    nats_client: Client,
    subject: &'static str,
    handler: F,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: Fn(bytes::Bytes) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<(), Box<dyn Error>>> + Send + 'static,
{
    let mut sub = nats_client.subscribe(subject).await?;
    tokio::spawn(async move {
        while let Some(msg) = sub.next().await {
            if let Err(err) = handler(msg.payload).await {
                eprintln!("Error processing message: {:?}", err);
            }
        }
    });
    Ok(())
}
