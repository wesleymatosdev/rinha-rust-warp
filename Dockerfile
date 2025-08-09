FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder 
COPY --from=planner /app/recipe.json recipe.json
# Cache dependencies
RUN cargo chef cook --release --recipe-path recipe.json

# Build application
COPY src src
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release --bin server --bin worker

# We do not need the Rust toolchain to run the binary!
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y pkg-config libssl-dev
WORKDIR /app
COPY --from=builder /app/target/release/server ./server
COPY --from=builder /app/target/release/worker ./worker
# Default to server, but can be overridden
CMD ["./server"]