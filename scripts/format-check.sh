#!/usr/bin/env sh
set -eu
[ -f AGENTS.md ] || { echo "format check ERROR: run from repository root." >&2; exit 1; }
[ -f Cargo.toml ] || { echo "format check ERROR: workspace not initialized. Execute EP-001 first." >&2; exit 1; }
cargo fmt --all --check
echo "format check: ok"
