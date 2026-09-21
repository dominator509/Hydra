#!/usr/bin/env sh
# Fixture tests for public versus internal smoke validation.
set -eu

[ -f AGENTS.md ] || { echo "smoke boundary tests ERROR: run from repository root." >&2; exit 1; }

TMP_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/hydra-smoke-boundary.XXXXXX")
trap 'rm -rf "$TMP_ROOT"' EXIT HUP INT TERM
FAKE_BIN="$TMP_ROOT/bin"
mkdir -p "$FAKE_BIN"
LOG="$TMP_ROOT/curl.log"

printf '%s\n' \
  '#!/usr/bin/env sh' \
  'printf "%s\\n" "$*" >> "$SMOKE_CURL_LOG"' \
  'printf "%s\\n" "tk_cache_hit_ratio 0.99 hydra_requests_total 1 hydra_envelopes_total 1"' > "$FAKE_BIN/curl"
chmod +x "$FAKE_BIN/curl"

run_smoke() {
  SMOKE_CURL_LOG="$LOG" PATH="$FAKE_BIN:$PATH" "$@"
}

: > "$LOG"
PUBLIC_OUTPUT="$TMP_ROOT/public.out"
run_smoke env HYDRA_SMOKE_URL=http://public.example sh scripts/smoke-test.sh > "$PUBLIC_OUTPUT"
if grep -F "http://public.example/metrics" "$LOG" >/dev/null; then
  echo "smoke boundary tests ERROR: public smoke requested /metrics" >&2
  exit 1
fi
grep -F "public metrics skipped (metrics are internal-only)" "$PUBLIC_OUTPUT" >/dev/null

: > "$LOG"
INTERNAL_OUTPUT="$TMP_ROOT/internal.out"
run_smoke env HYDRA_SMOKE_URL=http://public.example HYDRA_SMOKE_INTERNAL_METRICS_URL=http://kernel:8080/metrics sh scripts/smoke-test.sh > "$INTERNAL_OUTPUT"
grep -F "http://kernel:8080/metrics" "$LOG" >/dev/null
grep -F -- "--connect-timeout 5 --max-time 30" "$LOG" >/dev/null
if grep -F "http://public.example/metrics" "$LOG" >/dev/null; then
  echo "smoke boundary tests ERROR: internal metrics check used public URL" >&2
  exit 1
fi

if run_smoke env HYDRA_SMOKE_URL=http://public.example HYDRA_SMOKE_INTERNAL_METRICS_URL=http://public.example sh scripts/smoke-test.sh >/dev/null 2>&1; then
  echo "smoke boundary tests ERROR: equal public/internal URLs were accepted" >&2
  exit 1
fi

echo "smoke boundary tests: ok"
