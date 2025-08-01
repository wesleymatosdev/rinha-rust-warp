use std::net::SocketAddr;
use warp::Filter;

#[tokio::main]
async fn main() {
    let payments = warp::path("payments")
        .and(warp::post())
        .and(warp::body::bytes())
        .map(|body: bytes::Bytes| {
            // Here you would process the payment data
            println!("Received payment data: {:?}", body);
            warp::reply::json(&"Payment processed successfully")
        });
    let routes = payments.with(warp::log("payments"));
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .unwrap();
    let addr: SocketAddr = ([0, 0, 0, 0], port).into();

    println!("Server running on http://{}", addr);

    warp::serve(routes).run(addr).await;
}
