#!/usr/bin/env sh
# Local Shell semantic and no-JavaScript contract gate.
set -eu

[ -f AGENTS.md ] || { echo "shell accessibility ERROR: run from repository root." >&2; exit 1; }
cargo test -p shell --test accessibility_contract --offline -- --nocapture
echo "shell accessibility: ok"
