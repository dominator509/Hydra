#!/usr/bin/env sh
set -eu
[ -f AGENTS.md ] || { echo "build ERROR: run from repository root." >&2; exit 1; }
[ -f Cargo.toml ] || { echo "build ERROR: workspace not initialized. Execute EP-001 first." >&2; exit 1; }
# The adapter fixtures are WASM-only (built via scripts/build-adapters.sh for
# wasm32-wasip2). Exclude them from the host build: their WIT-generated
# export names break the host linker's version script.
cargo build --workspace --release --exclude adapter-memcrm --exclude adapter-suitelike
echo "build: ok"
