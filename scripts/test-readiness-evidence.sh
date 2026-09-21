#!/usr/bin/env sh
# Fixture tests for the fail-closed EP-010 evidence contract.
set -eu

[ -f AGENTS.md ] || { echo "readiness evidence ERROR: run from repository root." >&2; exit 1; }

# shellcheck disable=SC1091
. scripts/readiness-evidence.sh

TMP_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/hydra-readiness-evidence.XXXXXX")
trap 'rm -rf "$TMP_ROOT"' EXIT HUP INT TERM
OPS="$TMP_ROOT/OPERATIONS.md"
PR="$TMP_ROOT/PRODUCTION_READINESS.md"
NOW=$(date -u +%s)
TODAY=$(date -u +%F)
OLD=$(date -u -d '31 days ago' +%F)

fail() {
  echo "readiness evidence:FAIL: $1" >&2
  exit 1
}

write_operations() {
  date_value=$1
  status=$2
  printf '%s\n' \
    '| Drill | Date | Status | Metric/Evidence | Operator |' \
    '|---|---|---|---|---|' \
    "| D1 | $date_value | $status | restore evidence | operator |" \
    "| D2 | $date_value | $status | rollback evidence | operator |" \
    "| D3 | $date_value | $status | nuke evidence | operator |" \
    "| D4 | $date_value | $status | cache evidence | operator |" \
    "| D5 | $date_value | $status | freeze evidence | operator |" > "$OPS"
}

write_readiness() {
  result=$1
  owner=$2
  printf '%s\n' \
    '| Check | Owner | Date | Result |' \
    '|---|---|---|---|' \
    "| production-readiness-check.sh | $owner | $TODAY | $result |" \
    "| Restore drill (D1) | $owner | $TODAY | $result |" \
    "| Rollback drill (D2) | $owner | $TODAY | $result |" \
    "| Nuke/cache/autonomy drills (D3-D5) | $owner | $TODAY | $result |" \
    "| 24h staging soak | $owner | $TODAY | $result |" \
    "| Security review | $owner | $TODAY | $result |" \
    "| Performance review | $owner | $TODAY | $result |" \
    "| Privacy/data review | $owner | $TODAY | $result |" \
    "| Accessibility review | $owner | $TODAY | $result |" \
    "| Observability review | $owner | $TODAY | $result |" \
    "| Sign-off | $owner | $TODAY | $result |" > "$PR"
}

expect_failure() {
  expected=$1
  if readiness_evidence_check "$OPS" "$PR" "$NOW" 30; then
    fail "expected failure containing '$expected'"
  fi
  case "$READINESS_EVIDENCE_ERROR" in
    *"$expected"*) ;;
    *) fail "failure '$READINESS_EVIDENCE_ERROR' does not contain '$expected'" ;;
  esac
}

# The checked-in state is equivalent to this: even all local drills passing
# cannot make a pending launch table pass.
write_operations "$TODAY" PASS
write_readiness PENDING TBD
expect_failure "result must be PASS"

# A complete recent fixture is accepted.
write_readiness PASS release-owner
if ! readiness_evidence_check "$OPS" "$PR" "$NOW" 30; then
  fail "valid fixture rejected: $READINESS_EVIDENCE_ERROR"
fi

# Freshness is checked for both drill and launch evidence.
write_operations "$OLD" PASS
expect_failure "evidence is"
write_operations "$TODAY" PASS
write_readiness PASS release-owner

# Status, date, and owner/evidence placeholders are not accepted.
write_operations "$TODAY" PENDING
expect_failure "no exact PASS row"
write_operations "$TODAY" PASS
write_readiness PASS TBD
expect_failure "owner is a placeholder"

# Duplicate exact launch rows fail closed rather than selecting one silently.
write_readiness PASS release-owner
printf '%s\n' "| Security review | release-owner | $TODAY | PASS |" >> "$PR"
expect_failure "exactly one row"

echo "readiness evidence: ok"
