FROM rust:1.96-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p hydra-kernel

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/hydra-kernel /usr/local/bin/hydra-kernel
ENV HYDRA_BIND=0.0.0.0:8080
EXPOSE 8080
CMD ["hydra-kernel"]
