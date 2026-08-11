#!/usr/bin/env sh
# Integration tests: all target tests except e2e_*. Requires postgres+nats (docker compose up -d postgres nats).
set -eu
[ -f AGENTS.md ] || { echo "integration tests ERROR: run from repository root." >&2; exit 1; }
[ -f Cargo.toml ] || { echo "integration tests ERROR: workspace not initialized. Execute EP-001 first." >&2; exit 1; }
: "${DATABASE_URL:=postgres://hydra:hydra@localhost:5432/hydra}"
export DATABASE_URL
cargo test --workspace --test '*' -- --skip e2e_
echo "integration tests: ok"

# Failure/regression suites: exercise adapters with deliberate edge cases
# (rate limiting, inconsistent pagination, unicode payloads).
# These do not require Docker or live services — they run against in-process fixtures.
cargo test -p bridge-host --test conformance -- c5_rate_limit_surfaces --exact
cargo test -p bridge-host --test conformance -- c7_unicode_and_edges --exact
cargo test -p bridge-host --test conformance -- c8_grant_denial_graceful --exact
echo "failure suites: ok"
