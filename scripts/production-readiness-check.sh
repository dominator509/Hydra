#!/usr/bin/env sh
# Launch gate (SPEC-008): verify + smoke + cache audit + drill evidence D1..D5 (<30 days) + gate table rows.
set -eu
[ -f AGENTS.md ] || { echo "production readiness ERROR: run from repository root." >&2; exit 1; }
sh scripts/verify.sh
sh scripts/smoke-test.sh
[ -f crates/tokenkiller/tests/replay_corpus.rs ] && sh scripts/cache-hit-audit.sh
NOW=$(date -u +%s)
for d in D1 D2 D3 D4 D5; do
  ROW="$(grep -E "\|[[:space:]]*$d[[:space:]]*\|" OPERATIONS.md | grep -E "PASS" | tail -1 || true)"
  [ -n "$ROW" ] || { echo "production readiness ERROR: drill $d has no PASS row in OPERATIONS.md (SPEC-008)." >&2; exit 1; }
  DATE="$(printf '%s\n' "$ROW" | grep -oE '20[0-9]{2}-[0-9]{2}-[0-9]{2}' | head -1 || true)"
  [ -n "$DATE" ] || { echo "production readiness ERROR: drill $d PASS row lacks ISO date." >&2; exit 1; }
  TS=$(date -u -d "$DATE" +%s 2>/dev/null) || { echo "production readiness ERROR: cannot parse date $DATE for $d." >&2; exit 1; }
  AGE=$(( (NOW - TS) / 86400 ))
  [ "$AGE" -le 30 ] || { echo "production readiness ERROR: drill $d evidence is $AGE days old (>30)." >&2; exit 1; }
done
for check in "production-readiness-check.sh" "Restore drill" "Rollback drill" "24h staging soak"; do
  grep -F "$check" PRODUCTION_READINESS.md | grep -vE '\|[[:space:]]*\|[[:space:]]*\|[[:space:]]*\|' >/dev/null || { echo "production readiness ERROR: launch-gate row '$check' not filled in PRODUCTION_READINESS.md." >&2; exit 1; }
done
echo "production readiness: ok"
