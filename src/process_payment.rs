use std::time::Duration;

use async_nats::jetstream::{Context, consumer, stream::Config};
use futures_util::TryStreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::{Executor, Pool};
use std::sync::OnceLock;

use chrono::{DateTime, Utc};

fn default_timestamp() -> DateTime<Utc> {
    Utc::now()
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Payment {
    pub amount: f64,
    #[serde(rename = "correlationId")]
    pub correlation_id: String,
    #[serde(rename = "requestedAt", default = "default_timestamp")]
    pub requested_at: DateTime<Utc>,
}

impl TryFrom<bytes::Bytes> for Payment {
    type Error = anyhow::Error;

    fn try_from(value: bytes::Bytes) -> Result<Self, Self::Error> {
        // Clean the payload by removing non-breaking spaces and other problematic characters
        let cleaned_payload = match String::from_utf8_lossy(&value) {
            std::borrow::Cow::Borrowed(s) => s.to_string(),
            std::borrow::Cow::Owned(s) => s,
        };

        log::debug!("Cleaned payload: {}", cleaned_payload);

        // Remove non-breaking spaces (U+00A0) and other whitespace issues
        let cleaned_json = cleaned_payload
            .replace('\u{00A0}', " ") // Replace non-breaking space with regular space
            .replace('\u{202F}', " ") // Replace narrow no-break space
            .replace('\u{2009}', " ") // Replace thin space
            .replace('\u{200B}', ""); // Remove zero-width space

        log::debug!("Cleaned json: {}", cleaned_json);
        serde_json::from_str(&cleaned_json)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize payment: {}", e))
    }
}
#[allow(dead_code)]
static DEFAULT_PAYMENT_URL: OnceLock<String> = OnceLock::new();
#[allow(dead_code)]
static FALLBACK_PAYMENT_URL: OnceLock<String> = OnceLock::new();
#[allow(dead_code)]
pub fn get_default_payment_url() -> &'static str {
    DEFAULT_PAYMENT_URL.get_or_init(|| {
        std::env::var("DEFAULT_PAYMENT_URL").unwrap_or_else(|_| "http://localhost:8001".to_string())
    })
}
#[allow(dead_code)]
pub fn get_fallback_payment_url() -> &'static str {
    FALLBACK_PAYMENT_URL.get_or_init(|| {
        std::env::var("FALLBACK_PAYMENT_URL")
            .unwrap_or_else(|_| "http://localhost:8002".to_string())
    })
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
enum Gateway {
    Default,
    Fallback,
}

impl Gateway {
    #[allow(dead_code)]
    fn to_string(&self) -> &'static str {
        match self {
            Gateway::Default => "default",
            Gateway::Fallback => "fallback",
        }
    }

    #[allow(dead_code)]
    fn url(&self) -> &'static str {
        match self {
            Gateway::Default => get_default_payment_url(),
            Gateway::Fallback => get_fallback_payment_url(),
        }
    }
}

#[allow(dead_code)]
async fn send_payment(
    http_client: &Client,
    payment: &Payment,
    url: &str,
) -> Result<(), anyhow::Error> {
    let url = format!("{}/payments", url);
    log::debug!("Sending payment to {}: {:?}", url, payment);

    let response = http_client.post(url).json(payment).send().await?;

    if response.status().is_success() {
        log::debug!("Payment processed successfully: {:?}", payment);
        Ok(())
    } else {
        let status = response.status();
        let error_text = response.text().await?;
        Err(anyhow::anyhow!(
            "Failed to POST to /payments (status: {}): {:?}, response: {}",
            status,
            payment,
            error_text
        ))
    }
}

#[allow(dead_code)]
async fn save_payment_to_db(
    pg_pool: &Pool<sqlx::Postgres>,
    payment: &Payment,
    gateway: &Gateway,
) -> Result<(), anyhow::Error> {
    let sql = r#"
        INSERT INTO payments (correlation_id, amount, requested_at, gateway)
        VALUES ($1, $2, $3, $4)
    "#;

    let query = sqlx::query(sql)
        .bind(&payment.correlation_id)
        .bind(payment.amount)
        .bind(payment.requested_at)
        .bind(gateway.to_string());

    let result = pg_pool.execute(query).await;

    match result {
        Err(e) => {
            log::error!("Error saving payment to db: {:?}", e);
            Err(anyhow::anyhow!("Failed to save payment to database: {}", e))
        }
        Ok(_) => {
            log::info!("Payment saved to database successfully");
            Ok(())
        }
    }
}

#[allow(dead_code)]
async fn process_payment(
    http_client: &Client,
    pg_pool: &Pool<sqlx::Postgres>,
    payment: &Payment,
    backoff_rule: &[Duration],
) -> Result<(), anyhow::Error> {
    for duration in backoff_rule {
        match send_payment(http_client, payment, Gateway::Default.url()).await {
            Ok(_) => {
                log::debug!("Payment processed successfully with default gateway");
                save_payment_to_db(pg_pool, payment, &Gateway::Default).await?;
                return Ok(());
            }
            Err(e) => {
                log::warn!(
                    "Failed to process payment with default gateway ({}), using retry strategy. Error: {:?}",
                    payment.correlation_id,
                    e
                );
                tokio::time::sleep(duration.to_owned()).await;
            }
        }

        // Try fallback gateway
        match send_payment(http_client, payment, Gateway::Fallback.url()).await {
            Ok(_) => {
                log::debug!("Payment processed successfully with fallback gateway");
                save_payment_to_db(pg_pool, payment, &Gateway::Fallback).await?;
                return Ok(());
            }
            Err(e) => {
                log::error!(
                    "Failed to process payment with fallback gateway ({}). Error: {:?}",
                    payment.correlation_id,
                    e
                );
            }
        }
    }

    Err(anyhow::anyhow!(
        "Failed to process payment after retries: {}",
        payment.correlation_id
    ))
}

#[allow(dead_code)]
async fn get_stream(ctx: &Context) -> consumer::pull::Stream {
    let stream = ctx
        .create_stream(Config {
            name: "payments".to_string(),
            subjects: vec!["payments".to_string()],
            retention: async_nats::jetstream::stream::RetentionPolicy::Interest,
            ..Default::default()
        })
        .await
        .expect("Failed to create or get stream");

    let consumer = stream
        .get_or_create_consumer(
            "payment-processor",
            consumer::pull::Config {
                durable_name: Some("payment-processor".to_string()),
                max_deliver: 3,
                ack_policy: consumer::AckPolicy::Explicit,
                replay_policy: consumer::ReplayPolicy::Instant,
                ..Default::default()
            },
        )
        .await
        .expect("Failed to get or create consumer");

    consumer
        .messages()
        .await
        .expect("Failed to get messages from consumer")
}

#[allow(dead_code)]
pub struct PaymentProcessor {
    http_client: Client,
    pg_pool: Pool<sqlx::Postgres>,
    jetstream_context: Context,
    backoff_rule: Vec<Duration>,
}

impl PaymentProcessor {
    #[allow(dead_code)]
    pub fn new(pg_pool: Pool<sqlx::Postgres>, jetstream_context: Context) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");
        let backoff_rule = vec![
            Duration::from_millis(250),
            Duration::from_millis(500),
            Duration::from_secs(2),
        ];

        PaymentProcessor {
            http_client,
            pg_pool,
            jetstream_context,
            backoff_rule,
        }
    }

    #[allow(dead_code)]
    pub async fn start(self) {
        get_stream(&self.jetstream_context)
            .await
            .try_for_each_concurrent(
                std::env::var("MAX_CONCURRENT_MESSAGES")
                    .unwrap_or_else(|_| "200".into())
                    .parse()
                    .unwrap_or(200),
                async |msg| {
                    msg.ack().await.expect("Failed to acknowledge message");
                    let pg_pool = self.pg_pool.clone();
                    let jetstream_context = self.jetstream_context.clone();
                    let http_client = self.http_client.clone();
                    let backoff_rule = self.backoff_rule.clone();

                    log::info!("Received payload message: {:?}", msg.payload);
                    let payment = match Payment::try_from(msg.payload.clone()) {
                        Ok(payment) => payment,
                        Err(e) => {
                            log::error!("Failed to deserialize payment: {:?}", e);
                            return Ok(());
                        }
                    };

                    match process_payment(&http_client, &pg_pool, &payment, &backoff_rule).await {
                        Ok(_) => {
                            log::debug!("Payment processed successfully");
                        }
                        Err(_) => {
                            log::warn!("Requeing payment: {}", payment.correlation_id);
                            jetstream_context
                                .publish(msg.subject.clone(), msg.payload.clone())
                                .await
                                .expect("Failed to requeue message");
                        }
                    };

                    Ok(())
                },
            )
            .await
            .map_err(|e| {
                log::error!("Error processing payments: {:?}", e);
                anyhow::anyhow!("Error processing payments: {}", e)
            })
            .expect("Failed to stream messages")
    }
}
