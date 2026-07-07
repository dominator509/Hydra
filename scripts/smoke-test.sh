#!/usr/bin/env sh
# Smoke: against a running instance if HYDRA_SMOKE_URL set, else the in-repo smoke test.
set -eu
[ -f AGENTS.md ] || { echo "smoke test ERROR: run from repository root." >&2; exit 1; }
if [ -n "${HYDRA_SMOKE_URL:-}" ]; then
  command -v curl >/dev/null 2>&1 || { echo "smoke test ERROR: curl not found." >&2; exit 1; }
  curl -fsS "$HYDRA_SMOKE_URL/healthz" >/dev/null || { echo "smoke test ERROR: /healthz failed at $HYDRA_SMOKE_URL" >&2; exit 1; }
  curl -fsS "$HYDRA_SMOKE_URL/readyz"  >/dev/null || { echo "smoke test ERROR: /readyz failed at $HYDRA_SMOKE_URL" >&2; exit 1; }
  if [ -d crates/tokenkiller ]; then
    curl -fsS "$HYDRA_SMOKE_URL/metrics" | grep -q "tk_cache_hit_ratio" || { echo "smoke test ERROR: /metrics missing tk_cache_hit_ratio" >&2; exit 1; }
  fi
else
  [ -f Cargo.toml ] || { echo "smoke test ERROR: workspace not initialized. Execute EP-001 first." >&2; exit 1; }
  cargo test -p hydra-kernel --test smoke_healthz
fi
echo "smoke test: ok"
