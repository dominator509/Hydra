# docker/Dockerfile — Multi-stage build for hydra-kernel
# Stage 1: Builder
FROM rust:1.96.1-slim@sha256:31ee7fc65186be7e0e0ccb3f2ca305f14e4739e7642a1ae65753aa5d7b874523 AS builder

ARG WASM_TOOLS_VERSION=1.240.0

# Keep release-image package inputs explicit. Refreshing these pins is a
# deliberate release change, not an incidental package-index update.
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config=1.8.1-4 libssl-dev=3.5.6-1~deb13u2 \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add wasm32-wasip2 \
    && cargo install wasm-tools --locked --version "${WASM_TOOLS_VERSION}"

WORKDIR /app
COPY . .

ENV SQLX_OFFLINE=true

# Build the kernel and owner-controlled vault tooling
RUN cargo build --release -p hydra-kernel
RUN cargo build --release -p hydra-vault

# Build adapters (wasm)
RUN bash scripts/build-adapters.sh

# Stage 2: Runtime
FROM debian:bookworm-slim@sha256:abd67ffcfa541b485a3dff59865ab629aa048a6c613e639d36e7456b0b229241

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates=20250419~deb12u1 curl=7.88.1-10+deb12u15 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -u 1000 -m -d /app hydra

WORKDIR /app

# Copy kernel binary
COPY --from=builder /app/target/release/hydra-kernel /app/hydra-kernel
COPY --from=builder /app/target/release/hydra-vault /app/hydra-vault

# Copy adapters (wasm artifacts)
COPY --from=builder /app/adapters /app/adapters

# Copy SQL migrations
COPY --from=builder /app/migrations /app/migrations

# Copy WIT world files
COPY --from=builder /app/wit /app/wit

# Non-root user
USER hydra

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://localhost:8080/healthz || exit 1

ENV HYDRA_BIND=0.0.0.0:8080

ENTRYPOINT ["/app/hydra-kernel"]
