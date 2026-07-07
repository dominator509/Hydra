#!/usr/bin/env sh
# Preflight: mechanical readiness. Safe to run anytime.
set -eu
[ -f AGENTS.md ] && [ -d .agent ] || { echo "preflight ERROR: run from repository root (AGENTS.md not found)." >&2; exit 1; }
for f in COMMANDS.md ARCHITECTURE.md .agent/PLANS.md .agent/EXECUTION_RULES.md; do
  [ -f "$f" ] || { echo "preflight ERROR: required file missing: $f" >&2; exit 1; }
done
command -v cargo >/dev/null 2>&1 || { echo "preflight ERROR: cargo not found. Run: bash scripts/install.sh" >&2; exit 1; }
command -v git >/dev/null 2>&1 || { echo "preflight ERROR: git not found." >&2; exit 1; }
for s in install lint format-check typecheck test-unit test-integration test-e2e build security-check dependency-audit smoke-test verify cache-hit-audit production-readiness-check; do
  [ -f "scripts/$s.sh" ] || { echo "preflight ERROR: scripts/$s.sh missing." >&2; exit 1; }
done
if [ ! -f Cargo.toml ]; then
  echo "preflight NOTE: workspace not initialized yet — EP-001-foundation is the expected active plan." >&2
fi
if [ -f Cargo.toml ] && [ ! -f .env ] && [ -f .env.example ]; then
  echo "preflight NOTE: .env missing (cp .env.example .env for local services)." >&2
fi
command -v docker >/dev/null 2>&1 || echo "preflight NOTE: docker not found — integration/e2e milestones will need it." >&2
echo "preflight: ok"
