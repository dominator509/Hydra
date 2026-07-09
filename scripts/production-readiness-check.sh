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

pass_gate() {
  echo "production-readiness:pass: $1"
}

# ---------------------------------------------------------------------------
# Step 1 — verify.sh
# ---------------------------------------------------------------------------
gate_verify() {
  if [ "${VERIFY_SKIP:-0}" = "1" ]; then
    echo "production-readiness:skip: verify (VERIFY_SKIP=1)"
    return 0
  fi
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
  if [ "${SMOKE_SKIP:-0}" = "1" ]; then
    echo "production-readiness:skip: smoke (SMOKE_SKIP=1)"
    return 0
  fi
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
  if [ ! -f crates/tokenkiller/tests/replay_corpus.rs ]; then
    echo "production-readiness:skip: cache-hit-audit (no replay corpus)"
    return 0
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
# Step 5 — Drill evidence D1–D5 in OPERATIONS.md
# ---------------------------------------------------------------------------
gate_drills() {
  OPS="OPERATIONS.md"
  [ -f "$OPS" ] || fail "drills — $OPS not found"
  NOW=$(date -u +%s)
  for d in D1 D2 D3 D4 D5; do
    ROW="$(grep -E "^[|]" "$OPS" | grep -E "\|[[:space:]]*$d[[:space:]]*\|" | grep -E "PASS" | tail -1 || true)"
    [ -n "$ROW" ] || fail "drill $d — no PASS row in $OPS"
    DATE="$(printf '%s\n' "$ROW" | grep -oE '20[0-9]{2}-[0-9]{2}-[0-9]{2}' | head -1 || true)"
    [ -n "$DATE" ] || fail "drill $d — PASS row lacks ISO date in $OPS"
    TS=$(date -u -d "$DATE" +%s 2>/dev/null) || fail "drill $d — cannot parse date '$DATE'"
    AGE=$(( (NOW - TS) / 86400 ))
    [ "$AGE" -le 30 ] || fail "drill $d — evidence is $AGE days old (>30, last PASS on $DATE)"
  done
  pass_gate "drills D1–D5"
}

# ---------------------------------------------------------------------------
# Step 6 — Launch-gate rows in PRODUCTION_READINESS.md
# ---------------------------------------------------------------------------
gate_launch_table() {
  PR="PRODUCTION_READINESS.md"
  [ -f "$PR" ] || fail "launch-table — $PR not found"
  for check in "production-readiness-check.sh" "Restore drill" "Rollback drill" "24h staging soak"; do
    grep -F "$check" "$PR" | grep -vE '\|[[:space:]]*\|[[:space:]]*\|[[:space:]]*\|' >/dev/null \
      || fail "launch-table — row '$check' is empty in $PR (all columns must be filled except Sign-off)"
  done
  pass_gate "launch-table"
}

# ---------------------------------------------------------------------------
# Run all gates in order
# ---------------------------------------------------------------------------
gate_verify
gate_smoke
gate_cache_audit
gate_security
gate_drills
gate_launch_table

echo "production-readiness:ok"
