#!/usr/bin/env sh
set -eu
[ -f AGENTS.md ] || { echo "typecheck ERROR: run from repository root." >&2; exit 1; }
[ -f Cargo.toml ] || { echo "typecheck ERROR: workspace not initialized. Execute EP-001 first." >&2; exit 1; }
cargo check --workspace --all-targets
echo "typecheck: ok"
