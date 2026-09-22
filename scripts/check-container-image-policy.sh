#!/usr/bin/env sh
# Require immutable external image inputs across build, Compose, and CI.
set -eu

fail() {
  echo "container image policy ERROR: $1" >&2
  exit 1
}

check_image_lines() {
  file=$1
  [ -f "$file" ] || fail "missing $file"
  if awk '
    /^[[:space:]]+image:[[:space:]]*/ {
      if ($0 !~ /hydra\/(kernel|egress-proxy)/ && $0 !~ /@sha256:/) {
        exit 1
      }
    }
  ' "$file"; then
    :
  else
    fail "$file contains an unpinned external image"
  fi
}

check_from_lines() {
  file=$1
  [ -f "$file" ] || fail "missing $file"
  if awk '/^FROM[[:space:]]/ && $0 !~ /@sha256:/ { exit 1 }' "$file"; then
    :
  else
    fail "$file contains an unpinned base image"
  fi
}

check_package_pins() {
  file=$1
  shift
  [ -f "$file" ] || fail "missing $file"
  for pin in "$@"; do
    if ! grep -Fq -- "$pin" "$file"; then
      fail "$file is missing exact package pin: $pin"
    fi
  done
}

for file in .github/workflows/*.yml docker/compose.yaml docker/*.Dockerfile; do
  [ -f "$file" ] || continue
  if grep -Eq '(alpine:3\.20|caddy:2\.8-alpine|postgres:16-alpine|nats:2\.10-alpine|prom/prometheus:v2\.53\.0|prom/alertmanager:v0\.27\.0|rust:1\.96\.1-slim|debian:bookworm-slim)([[:space:]]|$)' "$file"; then
    fail "$file contains an unpinned external image invocation"
  fi
done

check_image_lines docker/compose.yaml
check_image_lines .github/workflows/ci.yml
check_image_lines .github/workflows/nightly.yml
check_image_lines .github/workflows/release.yml
check_from_lines docker/Dockerfile
check_from_lines docker/egress-proxy.Dockerfile
check_package_pins docker/Dockerfile \
  'pkg-config=1.8.1-4' \
  'libssl-dev=3.5.7-1~deb13u2' \
  'ca-certificates=20250419~deb12u1' \
  'curl=7.88.1-10+deb12u15'
check_package_pins docker/egress-proxy.Dockerfile 'tinyproxy=1.11.2-r0'

echo "container image policy: ok"
