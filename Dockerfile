FROM rust:1.98-bookworm AS builder

WORKDIR /app
COPY . .
RUN cargo build --release -p api -p mcp \
    && rustc /app/docker/healthcheck.rs -O -o /app/target/release/memoryops-healthcheck

FROM gcr.io/distroless/cc-debian12:nonroot AS runtime-base

ENV PATH="/usr/local/bin:/usr/bin:/bin"

COPY --from=builder --chown=nonroot:nonroot /app/target/release/api /usr/local/bin/api
COPY --from=builder --chown=nonroot:nonroot /app/target/release/mcp /usr/local/bin/mcp
COPY --from=builder --chown=nonroot:nonroot /app/target/release/memoryops-healthcheck /usr/local/bin/memoryops-healthcheck
COPY --chown=nonroot:nonroot config.toml /app/config.toml
COPY --chown=nonroot:nonroot .gemini /app/.gemini
COPY --chown=nonroot:nonroot .claude /app/.claude

WORKDIR /app
USER nonroot:nonroot

FROM runtime-base AS mcp-runtime
CMD ["/usr/local/bin/mcp"]

FROM runtime-base AS api-runtime
CMD ["/usr/local/bin/api"]
