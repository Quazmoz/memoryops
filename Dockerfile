FROM rust:1.88-bookworm AS builder

WORKDIR /app
COPY . .
RUN cargo build --release -p api -p mcp

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/api /usr/local/bin/api
COPY --from=builder /app/target/release/mcp /usr/local/bin/mcp
COPY config.toml /app/config.toml

CMD ["mcp"]