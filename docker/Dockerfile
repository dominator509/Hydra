# docker/Dockerfile — Multi-stage build for hydra-kernel
# Stage 1: Builder
FROM rust:1.96.1-slim AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

# Build the kernel binary
RUN cargo build --release -p hydra-kernel

# Build adapters (wasm)
RUN bash scripts/build-adapters.sh

# Stage 2: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -u 1000 -m -d /app hydra

WORKDIR /app

# Copy kernel binary
COPY --from=builder /app/target/release/hydra-kernel /app/hydra-kernel

# Copy adapters (wasm artifacts)
COPY --from=builder /app/adapters /app/adapters

# Copy SQL migrations
COPY --from=builder /app/migrations /app/migrations

# Copy static assets (if any)
COPY --from=builder /app/static /app/static

# Copy WIT world files
COPY --from=builder /app/wit /app/wit

# Non-root user
USER hydra

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://localhost:8080/healthz || exit 1

ENV HYDRA_BIND=0.0.0.0:8080

ENTRYPOINT ["/app/hydra-kernel"]
