use tokio_stream::wrappers::UnixListenerStream;
use warp::Filter;

async fn payments_handler(
    body: bytes::Bytes,
    nats_client: async_nats::Client,
) -> Result<impl warp::Reply, warp::Rejection> {
    nats_client.publish("payments", body).await.unwrap();
    Ok(warp::reply())
}

pub async fn routes(
    nats_client: async_nats::Client,
) -> impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let payments = warp::path("payments")
        .and(warp::post())
        .and(warp::body::bytes())
        .and(warp::any().map(move || nats_client.clone()))
        .and_then(payments_handler);
    let routes = payments.with(warp::log("payments"));

    routes
}

pub async fn serve(
    routes: impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection>
    + Clone
    + Send
    + 'static
    + Sync,
    unix_listener: UnixListenerStream,
) {
    warp::serve(routes).run_incoming(unix_listener).await
}
