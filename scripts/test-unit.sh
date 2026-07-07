#!/usr/bin/env sh
set -eu
[ -f AGENTS.md ] || { echo "unit tests ERROR: run from repository root." >&2; exit 1; }
[ -f Cargo.toml ] || { echo "unit tests ERROR: workspace not initialized. Execute EP-001 first." >&2; exit 1; }
cargo test --workspace --lib
echo "unit tests: ok"
