#!/usr/bin/env sh
# Full local validation sequence. Each sub-script prints its own ok line and fails loudly.
set -eu
[ -f AGENTS.md ] || { echo "verify ERROR: run from repository root." >&2; exit 1; }
sh scripts/preflight.sh
sh scripts/lint.sh
sh scripts/format-check.sh
sh scripts/typecheck.sh
sh scripts/test-unit.sh
sh scripts/test-integration.sh
sh scripts/test-e2e.sh
sh scripts/build.sh
sh scripts/security-check.sh
sh scripts/dependency-audit.sh
sh scripts/smoke-test.sh
if [ -d crates/tokenkiller ]; then sh scripts/cache-hit-audit.sh; fi
echo "verify: ok"
