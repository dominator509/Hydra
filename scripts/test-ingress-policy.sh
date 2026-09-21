#!/usr/bin/env sh
# Fixture tests for the public Caddy metrics boundary.
set -eu

[ -f AGENTS.md ] || { echo "ingress policy tests ERROR: run from repository root." >&2; exit 1; }

TMP_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/hydra-ingress-policy.XXXXXX")
trap 'rm -rf "$TMP_ROOT"' EXIT HUP INT TERM

expect_failure() {
  fixture=$1
  expected=$2
  if HYDRA_CADDYFILE="$fixture" sh scripts/check-ingress-policy.sh >/dev/null 2>&1; then
    echo "ingress policy tests ERROR: expected '$expected' to fail" >&2
    exit 1
  fi
}

GOOD="$TMP_ROOT/good.Caddyfile"
cp docker/Caddyfile "$GOOD"
HYDRA_CADDYFILE="$GOOD" sh scripts/check-ingress-policy.sh >/dev/null

MISSING_MATCHER="$TMP_ROOT/missing-matcher.Caddyfile"
cp "$GOOD" "$MISSING_MATCHER"
sed -i '/@private_metrics path \/metrics\*/d' "$MISSING_MATCHER"
expect_failure "$MISSING_MATCHER" matcher

MISSING_DENIAL="$TMP_ROOT/missing-denial.Caddyfile"
cp "$GOOD" "$MISSING_DENIAL"
sed -i '/respond "Not Found" 404/d' "$MISSING_DENIAL"
expect_failure "$MISSING_DENIAL" denial

MISSING_PROXY_HANDLE="$TMP_ROOT/missing-proxy-handle.Caddyfile"
cp "$GOOD" "$MISSING_PROXY_HANDLE"
sed -i '/handle {/d' "$MISSING_PROXY_HANDLE"
expect_failure "$MISSING_PROXY_HANDLE" proxy

echo "ingress policy tests: ok"
