FROM rust:1.96-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS cacher
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

FROM chef AS builder
COPY . .
COPY --from=cacher /app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo
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
