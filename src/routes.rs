use std::{collections::HashMap, convert::Infallible, os::unix::fs::PermissionsExt};

use async_nats::jetstream::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Pool;
use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;
use warp::{Filter, reply::Reply};

async fn payments_handler(body: bytes::Bytes, ctx: Context) -> Result<impl Reply, Infallible> {
    let _ = ctx.publish("payments", body).await.unwrap().await;
    Ok(warp::reply())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PaymentSummaryFilters {
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
}

pub async fn payment_summary(
    filters: PaymentSummaryFilters,
    pg_pool: Pool<sqlx::Postgres>,
) -> Result<impl Reply, std::convert::Infallible> {
    let sql = r#"
                        SELECT correlation_id, amount, requested_at, gateway
                        FROM payments
                        WHERE payments.requested_at >= $1 AND payments.requested_at <= $2
                    "#;

    let from = filters
        .from
        .unwrap_or_else(|| Utc::now() - chrono::Duration::days(30));
    let to = filters.to.unwrap_or_else(|| Utc::now());

    let Ok(payments) = sqlx::query_as::<_, (String, f64, DateTime<Utc>, String)>(sql)
        .bind(from)
        .bind(to)
        .fetch_all(&pg_pool)
        .await
    else {
        return Ok(warp::reply::with_status(
            "Failed to query the database",
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        )
        .into_response());
    };

    let mut summary: HashMap<String, (i32, f64)> = HashMap::new();
    for (_correlation_id, amount, _requested_at, gateway) in payments {
        let entry = summary.entry(gateway).or_insert((0, 0.0));
        entry.0 += 1; // totalRequests
        entry.1 += amount as f64; // totalAmount
    }

    let empty_json = serde_json::json!({
        "totalRequests": 0,
        "totalAmount": 0.0
    });
    let default = summary
        .get("default")
        .map(|&(total_requests, total_amount)| {
            serde_json::json!({
                "totalRequests": total_requests,
                "totalAmount": total_amount
            })
        })
        .unwrap_or(empty_json.clone());
    let fallback = summary
        .get("fallback")
        .map(|&(total_requests, total_amount)| {
            serde_json::json!({
                "totalRequests": total_requests,
                "totalAmount": total_amount
            })
        })
        .unwrap_or(empty_json);

    let response = serde_json::json!({
        "default": default,
        "fallback": fallback
    });

    Ok(warp::reply::json(&response).into_response())
}

pub async fn purge_payments(pg_pool: Pool<sqlx::Postgres>) -> Result<impl Reply, Infallible> {
    let sql = "DELETE FROM payments";
    match sqlx::query(sql).execute(&pg_pool).await {
        Ok(_) => Ok(warp::reply::with_status(
            "Payments purged",
            warp::http::StatusCode::OK,
        )),
        Err(e) => {
            log::error!("Error purging payments: {:?}", e);
            Ok(warp::reply::with_status(
                "Failed to purge payments",
                warp::http::StatusCode::INTERNAL_SERVER_ERROR,
            ))
        }
    }
}

pub async fn routes(
    nats_client: Context,
    pg_pool: Pool<sqlx::Postgres>,
) -> impl warp::Filter<Extract = impl Reply, Error = warp::Rejection> + Clone {
    let payments = warp::path("payments")
        .and(warp::post())
        .and(warp::body::bytes())
        .and(warp::any().map(move || nats_client.clone()))
        .and_then(payments_handler)
        .with(warp::log("payments"));

    let payments_summary = warp::path("payments-summary")
        .and(warp::get())
        .and(warp::query::<PaymentSummaryFilters>())
        .and(warp::any().map({
            let pg_pool = pg_pool.clone();
            move || pg_pool.clone()
        }))
        .and_then(payment_summary);

    let purge_payments = warp::path("purge-payments")
        .and(warp::post())
        .and(warp::any().map(move || pg_pool.clone()))
        .and_then(purge_payments);

    let routes = payments.or(payments_summary).or(purge_payments);

    routes
}

pub struct Server {
    jetstream_context: Context,
    pg_pool: Pool<sqlx::Postgres>,
}

impl Server {
    pub fn new(jetstream_context: Context, pg_pool: Pool<sqlx::Postgres>) -> Self {
        Server {
            jetstream_context,
            pg_pool,
        }
    }

    pub async fn start(self) -> Result<(), Box<dyn std::error::Error>> {
        let unix_socket_path =
            std::env::var("UNIX_SOCKET_PATH").unwrap_or_else(|_| "/tmp/rinha.sock".to_string());

        // Remove existing socket file if it exists
        if std::path::Path::new(&unix_socket_path).exists() {
            std::fs::remove_file(&unix_socket_path)?;
        }

        let listener = UnixListener::bind(&unix_socket_path)
            .unwrap_or_else(|_| panic!("Failed to bind to Unix socket at {}", unix_socket_path));

        // Set permissions to allow nginx to connect (0o666 = read/write for all)
        std::fs::set_permissions(&unix_socket_path, std::fs::Permissions::from_mode(0o666))?;
        // let listener = tokio::net::UnixListener::bind(unix_socket_path)?;

        let stream = UnixListenerStream::new(listener);
        let routes = routes(self.jetstream_context, self.pg_pool).await;
        log::info!("Starting server on Unix socket...");
        warp::serve(routes).run_incoming(stream).await;

        Ok(())
    }
}
