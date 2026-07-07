#!/usr/bin/env sh
# Integration tests: all target tests except e2e_*. Requires postgres+nats (docker compose up -d postgres nats).
set -eu
[ -f AGENTS.md ] || { echo "integration tests ERROR: run from repository root." >&2; exit 1; }
[ -f Cargo.toml ] || { echo "integration tests ERROR: workspace not initialized. Execute EP-001 first." >&2; exit 1; }
: "${DATABASE_URL:=postgres://hydra:hydra@localhost:5432/hydra}"
export DATABASE_URL
cargo test --workspace --test '*' -- --skip e2e_
echo "integration tests: ok"
