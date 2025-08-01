mod nats;
mod process_payment;
mod routes;

use std::net::SocketAddr;

use crate::process_payment::process_payment;

#[tokio::main]
async fn main() -> Result<(), Box<dyn core::error::Error>> {
    let nats_client = nats::get_nats_client().await?;

    nats::register_subscriber(nats_client.clone(), "payments", process_payment).await?;

    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .unwrap();
    let addr: SocketAddr = ([0, 0, 0, 0], port).into();

    println!("Server running on http://{}", addr);

    let routes = routes::routes(nats_client).await;

    routes::serve(routes, addr).await;

    Ok(())
}
