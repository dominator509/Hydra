#!/usr/bin/env sh
# Required Hydra/Nexus end-to-end scenarios. Missing tests are a hard failure.
set -eu
[ -f AGENTS.md ] || { echo "e2e tests ERROR: run from repository root." >&2; exit 1; }
[ -f Cargo.toml ] || { echo "e2e tests ERROR: workspace not initialized. Execute EP-001 first." >&2; exit 1; }
TEST_FILE=crates/kernel/tests/e2e_nexus.rs
[ -f "$TEST_FILE" ] || { echo "e2e tests ERROR: required target $TEST_FILE is missing." >&2; exit 1; }
for required in e2e_nexus_round_trip e2e_cross_business_blocked; do
  grep -Eq "async fn ${required}[[:space:]]*\(" "$TEST_FILE" || {
    echo "e2e tests ERROR: required test $required is missing." >&2
    exit 1
  }
done
discovered=$(grep -Ec 'async fn e2e_[A-Za-z0-9_]+[[:space:]]*\(' "$TEST_FILE")
[ "$discovered" -ge 2 ] || { echo "e2e tests ERROR: expected at least 2 E2E tests, found $discovered." >&2; exit 1; }
cargo test -p hydra-kernel --test e2e_nexus e2e_ -- --nocapture --test-threads=1
echo "e2e tests: ok"
