#!/usr/bin/env sh
# Smoke: against a running instance if HYDRA_SMOKE_URL set, else the in-repo smoke test.
set -eu
[ -f AGENTS.md ] || { echo "smoke test ERROR: run from repository root." >&2; exit 1; }
if [ -n "${HYDRA_SMOKE_URL:-}" ]; then
  command -v curl >/dev/null 2>&1 || { echo "smoke test ERROR: curl not found." >&2; exit 1; }
  # A stalled ingress must fail the gate, not hold deployment validation open indefinitely.
  curl --connect-timeout 5 --max-time 30 -fsS "$HYDRA_SMOKE_URL/healthz" >/dev/null || { echo "smoke test ERROR: /healthz failed at $HYDRA_SMOKE_URL" >&2; exit 1; }
  curl --connect-timeout 5 --max-time 30 -fsS "$HYDRA_SMOKE_URL/readyz"  >/dev/null || { echo "smoke test ERROR: /readyz failed at $HYDRA_SMOKE_URL" >&2; exit 1; }

  if [ -n "${HYDRA_SMOKE_INTERNAL_METRICS_URL:-}" ]; then
    [ "$HYDRA_SMOKE_INTERNAL_METRICS_URL" != "$HYDRA_SMOKE_URL" ] || {
      echo "smoke test ERROR: internal metrics URL must differ from the public smoke URL" >&2
      exit 1
    }
    METRICS=$(curl --connect-timeout 5 --max-time 30 -fsS "$HYDRA_SMOKE_INTERNAL_METRICS_URL") || { echo "smoke test ERROR: internal metrics failed at $HYDRA_SMOKE_INTERNAL_METRICS_URL" >&2; exit 1; }
    echo "$METRICS" | grep -q "tk_cache_hit_ratio" || { echo "smoke test ERROR: internal metrics missing tk_cache_hit_ratio" >&2; exit 1; }
    echo "$METRICS" | grep -q "hydra_requests_total" || { echo "smoke test ERROR: internal metrics missing hydra_requests_total" >&2; exit 1; }
    echo "$METRICS" | grep -q "hydra_envelopes_total" || { echo "smoke test ERROR: internal metrics missing hydra_envelopes_total" >&2; exit 1; }
  else
    echo "smoke test: public metrics skipped (metrics are internal-only)"
  fi
else
  [ -f Cargo.toml ] || { echo "smoke test ERROR: workspace not initialized. Execute EP-001 first." >&2; exit 1; }
  cargo test -p hydra-kernel --test smoke_healthz
fi
echo "smoke test: ok"
