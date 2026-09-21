#!/usr/bin/env sh
# Required local performance evidence. No staging or production claim.
set -eu

[ -f AGENTS.md ] || { echo "performance ERROR: run from repository root." >&2; exit 1; }
[ -f scripts/test-nightly-conformance.sh ] || { echo "performance ERROR: nightly conformance wrapper missing." >&2; exit 1; }
[ -f scripts/cache-hit-audit.sh ] || { echo "performance ERROR: cache-hit audit missing." >&2; exit 1; }

OUTPUT=$(mktemp "${TMPDIR:-/tmp}/hydra-governor-performance.XXXXXX")
cleanup() {
  rm -f "$OUTPUT"
}
trap cleanup EXIT HUP INT TERM

status=0
if cargo test -p governor --test core_domain --release -- --ignored perf_governor_eval_p99_under_5ms --nocapture >"$OUTPUT" 2>&1; then
  status=0
else
  status=$?
fi
cat "$OUTPUT"

if [ "$status" -ne 0 ]; then
  echo "performance ERROR: Governor p99 command exited $status." >&2
  exit "$status"
fi
grep -Eq '^test perf_governor_eval_p99_under_5ms \.\.\. ok$' "$OUTPUT" || {
  echo "performance ERROR: Governor p99 test was not discovered and passed." >&2
  exit 1
}
grep -Eq '^test result: ok\.' "$OUTPUT" || {
  echo "performance ERROR: Governor p99 result marker is missing." >&2
  exit 1
}

sh scripts/test-nightly-conformance.sh
sh scripts/cache-hit-audit.sh
echo "performance: ok"
