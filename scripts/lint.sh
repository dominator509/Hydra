#!/usr/bin/env sh
set -eu
[ -f AGENTS.md ] || { echo "lint ERROR: run from repository root." >&2; exit 1; }
[ -f Cargo.toml ] || { echo "lint ERROR: workspace not initialized. Execute .agent/execplans/EP-001-foundation.md first." >&2; exit 1; }
cargo clippy --workspace --all-targets -- -D warnings
echo "lint: ok"
