FROM rust:1.96-bookworm AS builder

WORKDIR /app
COPY . .
RUN cargo build --release -p api -p mcp

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd -r -g 1001 memoryops \
    && useradd -r -u 1001 -g memoryops -d /app -s /sbin/nologin memoryops

WORKDIR /app
COPY --from=builder /app/target/release/api /usr/local/bin/api
COPY --from=builder /app/target/release/mcp /usr/local/bin/mcp
COPY config.toml /app/config.toml
COPY .gemini /app/.gemini
COPY .claude /app/.claude
RUN chown -R memoryops:memoryops /app

USER memoryops

CMD ["api"]
