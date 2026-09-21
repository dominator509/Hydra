#!/usr/bin/env sh
# Full local validation sequence. Each sub-script prints its own ok line and fails loudly.
set -eu
[ -f AGENTS.md ] || { echo "verify ERROR: run from repository root." >&2; exit 1; }
sh scripts/preflight.sh
sh scripts/test-operational-tools.sh
sh scripts/test-readiness-evidence.sh
sh scripts/test-ingress-policy.sh
sh scripts/test-smoke-boundary.sh
sh scripts/check-ingress-policy.sh
sh scripts/check-release-policy.sh
sh scripts/check-egress-policy.sh
sh scripts/check-container-image-policy.sh
sh scripts/check-observability.sh
sh scripts/check-nats-policy.sh
sh scripts/lint.sh
sh scripts/format-check.sh
sh scripts/typecheck.sh
sh scripts/test-unit.sh
sh scripts/test-integration.sh
sh scripts/test-e2e.sh
sh scripts/test-shell-accessibility.sh
sh scripts/test-deployment-safety.sh
sh scripts/test-performance.sh
sh scripts/build.sh
sh scripts/security-check.sh
sh scripts/dependency-audit.sh
sh scripts/smoke-test.sh
echo "verify: ok"
