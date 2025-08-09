pub mod nats;
pub mod process_payment;
pub mod routes;

// Re-export common types
pub use process_payment::{Payment, PaymentProcessor};
pub use routes::Server;
