#!/usr/bin/env sh
# E2E: tests named e2e_*. Pre-EP-005 temporary allowance is NOISY (stderr) per EP-001 M4 Decision Log rule.
set -eu
[ -f AGENTS.md ] || { echo "e2e tests ERROR: run from repository root." >&2; exit 1; }
[ -f Cargo.toml ] || { echo "e2e tests ERROR: workspace not initialized. Execute EP-001 first." >&2; exit 1; }
if [ -d crates ] && grep -rq "fn e2e_" crates 2>/dev/null; then
  cargo test --workspace e2e_
else
  echo "e2e WARNING: no e2e_ tests exist yet; allowance expires at EP-005 (must be recorded in the active ExecPlan Decision Log)." >&2
fi
echo "e2e tests: ok"
