#!/usr/bin/env sh
set -eu
[ -f AGENTS.md ] || { echo "build ERROR: run from repository root." >&2; exit 1; }
[ -f Cargo.toml ] || { echo "build ERROR: workspace not initialized. Execute EP-001 first." >&2; exit 1; }
cargo build --workspace --release
echo "build: ok"
