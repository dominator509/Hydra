#!/usr/bin/env sh
# EP-010 Production Readiness Gate
# Runs the full validation pipeline and checks drill evidence.
# Exit 0 only if ALL gates pass; first failure on stderr + exit 1.
set -eu

GATE_NAME="production-readiness-check"
ROOT_CHECK="AGENTS.md"
[ -f "$ROOT_CHECK" ] || { echo "production-readiness:ERROR: run from repository root." >&2; exit 1; }

fail() {
  echo "production-readiness:FAIL: $1" >&2
  exit 1
}

require_test_database() {
  test_database_url="${HYDRA_TEST_DATABASE_URL:-}"
  [ -n "$test_database_url" ] || fail "database - HYDRA_TEST_DATABASE_URL must be explicitly set to a disposable loopback Postgres URL"
  case "$test_database_url" in
    postgres://*@127.0.0.1:*/*|postgres://*@localhost:*/*|postgresql://*@127.0.0.1:*/*|postgresql://*@localhost:*/*)
      ;;
    *)
      fail "database - HYDRA_TEST_DATABASE_URL must target a loopback Postgres host"
      ;;
  esac
  # Prevent any inherited DATABASE_URL from changing the verifier's target.
  DATABASE_URL="$test_database_url"
  export DATABASE_URL HYDRA_TEST_DATABASE_URL
}

require_test_nats() {
  test_nats_url="${NATS_URL:-}"
  [ -n "$test_nats_url" ] || fail "NATS - NATS_URL must be explicitly set to a disposable loopback broker URL"
  case "$test_nats_url" in
    *[[:space:]]*|*@*)
      fail "NATS - NATS_URL must not contain whitespace or embedded credentials"
      ;;
  esac

  old_ifs=$IFS
  IFS=,
  set -- $test_nats_url
  IFS=$old_ifs
  [ "$#" -gt 0 ] || fail "NATS - NATS_URL must contain at least one endpoint"
  for endpoint do
    case "$endpoint" in
      "nats://127.0.0.1:"*|"nats://localhost:"*|"nats://[::1]:"*|"tls://127.0.0.1:"*|"tls://localhost:"*|"tls://[::1]:"*)
        ;;
      *)
        fail "NATS - NATS_URL endpoints must target loopback hosts"
        ;;
    esac
  done
}

pass_gate() {
  echo "production-readiness:pass: $1"
}

# ---------------------------------------------------------------------------
# Step 1 — verify.sh
# ---------------------------------------------------------------------------
gate_verify() {
  if [ ! -f scripts/verify.sh ]; then
    fail "verify — scripts/verify.sh not found"
  fi
  sh scripts/verify.sh || fail "verify — scripts/verify.sh exited non-zero"
  pass_gate "verify"
}

# ---------------------------------------------------------------------------
# Step 2 — smoke-test.sh
# ---------------------------------------------------------------------------
gate_smoke() {
  if [ ! -f scripts/smoke-test.sh ]; then
    fail "smoke — scripts/smoke-test.sh not found"
  fi
  sh scripts/smoke-test.sh || fail "smoke — scripts/smoke-test.sh exited non-zero"
  pass_gate "smoke"
}

# ---------------------------------------------------------------------------
# Step 3 — cache-hit-audit.sh (conditional on replay corpus)
# ---------------------------------------------------------------------------
gate_cache_audit() {
  if [ "${CACHE_AUDIT_SKIP:-0}" = "1" ]; then
    echo "production-readiness:skip: cache-hit-audit (CACHE_AUDIT_SKIP=1)"
    return 0
  fi
  if [ ! -f crates/tokenkiller/tests/replay_corpus.rs ]; then
    fail "cache-hit-audit — replay corpus is required"
  fi
  if [ ! -f scripts/cache-hit-audit.sh ]; then
    fail "cache-hit-audit — scripts/cache-hit-audit.sh not found"
  fi
  sh scripts/cache-hit-audit.sh || fail "cache-hit-audit — scripts/cache-hit-audit.sh exited non-zero"
  pass_gate "cache-hit-audit"
}

# ---------------------------------------------------------------------------
# Step 4 — security-check.sh
# ---------------------------------------------------------------------------
gate_security() {
  if [ ! -f scripts/security-check.sh ]; then
    fail "security — scripts/security-check.sh not found"
  fi
  sh scripts/security-check.sh || fail "security — scripts/security-check.sh exited non-zero"
  pass_gate "security"
}

# ---------------------------------------------------------------------------
# Step 5 — Exact drill and launch evidence in the checked-in ledgers
# ---------------------------------------------------------------------------
gate_evidence() {
  [ -f scripts/readiness-evidence.sh ] || fail "evidence — scripts/readiness-evidence.sh not found"
  # shellcheck disable=SC1091
  . scripts/readiness-evidence.sh
  if readiness_evidence_check OPERATIONS.md PRODUCTION_READINESS.md "$(date -u +%s)" 30; then
    :
  else
    fail "$READINESS_EVIDENCE_ERROR"
  fi
  pass_gate "drills D1-D5 and launch-table"
}

# ---------------------------------------------------------------------------
# Run all gates in order
# ---------------------------------------------------------------------------
require_test_database
require_test_nats
gate_verify
gate_smoke
gate_cache_audit
gate_security
gate_evidence

echo "production-readiness:ok"
