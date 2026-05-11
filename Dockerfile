FROM rust:1.93-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release --features postgres

FROM debian:bookworm-slim

RUN useradd -m -u 10001 paperproof
WORKDIR /app
COPY --from=builder /app/target/release/paperproof-indexer-reference /usr/local/bin/paperproof-indexer-reference
USER paperproof
ENV RUST_LOG=paperproof_indexer_reference=info,paperproof_sdk_rs=info
EXPOSE 8787
CMD ["paperproof-indexer-reference", "serve", "--bind", "0.0.0.0:8787"]
