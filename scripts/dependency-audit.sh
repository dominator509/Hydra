#!/usr/bin/env sh
set -eu
[ -f AGENTS.md ] || { echo "dependency audit ERROR: run from repository root." >&2; exit 1; }
[ -f Cargo.toml ] || { echo "dependency audit ERROR: workspace not initialized. Execute EP-001 first." >&2; exit 1; }
cargo deny --version >/dev/null 2>&1 || { echo "dependency audit ERROR: cargo deny missing. Run: bash scripts/install.sh" >&2; exit 1; }
cargo deny check
echo "dependency audit: ok"
