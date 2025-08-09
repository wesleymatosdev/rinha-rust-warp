mod nats;
mod process_payment;
mod queue;
mod routes;

use std::os::unix::fs::PermissionsExt;
use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;

use crate::process_payment::process_payment;

#[tokio::main]
async fn main() -> Result<(), Box<dyn core::error::Error>> {
    let nats_client = nats::get_nats_client().await?;

    nats::register_subscriber(nats_client.clone(), "payments", process_payment).await?;

    let routes = routes::routes(nats_client).await;

    let unix_socket_path =
        std::env::var("UNIX_SOCKET_PATH").unwrap_or_else(|_| "/tmp/rinha.sock".to_string());

    let listener = UnixListener::bind(&unix_socket_path)
        .unwrap_or_else(|_| panic!("Failed to bind to Unix socket at {}", unix_socket_path));

    // Set permissions to allow nginx to connect (0o666 = read/write for all)
    std::fs::set_permissions(&unix_socket_path, std::fs::Permissions::from_mode(0o666))?;
    // let listener = tokio::net::UnixListener::bind(unix_socket_path)?;

    let stream = UnixListenerStream::new(listener);

    routes::serve(routes, stream).await;

    Ok(())
}
