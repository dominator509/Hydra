#!/usr/bin/env sh
# Run required ignored conformance tests and prove that the soak case exists.
set -eu

output=$(mktemp "${TMPDIR:-/tmp}/hydra-nightly-conformance.XXXXXX")
cleanup() {
  rm -f "$output"
}
trap cleanup EXIT HUP INT TERM

status=0
if cargo test -p bridge-host --test conformance -- --ignored --nocapture >"$output" 2>&1; then
  status=0
else
  status=$?
fi

cat "$output"

if [ "$status" -ne 0 ]; then
  echo "nightly conformance ERROR: cargo test exited $status." >&2
  exit "$status"
fi

if ! grep -Eq 'running [1-9][0-9]* tests?' "$output"; then
  echo "nightly conformance ERROR: no ignored conformance tests were discovered." >&2
  exit 1
fi

if ! grep -Eq '^test c9_soak_10k \.\.\. (ok|ignored)$' "$output"; then
  echo "nightly conformance ERROR: required c9_soak_10k was not discovered and passed." >&2
  exit 1
fi

if ! grep -Eq 'test result: ok\.' "$output"; then
  echo "nightly conformance ERROR: cargo did not report an ok test result." >&2
  exit 1
fi

echo "nightly conformance: ok"
