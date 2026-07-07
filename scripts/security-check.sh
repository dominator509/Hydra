#!/usr/bin/env sh
# Secret-pattern scan over tracked files + tracked-.env guard + cargo audit.
set -eu
[ -f AGENTS.md ] || { echo "security check ERROR: run from repository root." >&2; exit 1; }
if git ls-files 2>/dev/null | grep -qx '\.env'; then
  echo "security check ERROR: .env is tracked by git. Untrack it (git rm --cached .env)." >&2; exit 1
fi
PATTERNS='AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY-----|sk-[A-Za-z0-9_-]{24,}|AGE-SECRET-KEY-1[A-Z0-9]{20,}|ghp_[A-Za-z0-9]{30,}|xox[baprs]-[A-Za-z0-9-]{10,}'
if git ls-files 2>/dev/null | xargs -r grep -nE "$PATTERNS" -- 2>/dev/null; then
  echo "security check ERROR: potential secret material matched above patterns." >&2; exit 1
fi
if [ -f Cargo.toml ]; then
  cargo audit --version >/dev/null 2>&1 || { echo "security check ERROR: cargo audit missing. Run: bash scripts/install.sh" >&2; exit 1; }
  cargo audit
fi
if [ -f docker/alerts.yaml ]; then
  python3 -c "import yaml;yaml.safe_load(open('docker/alerts.yaml'))" 2>/dev/null || { echo "security check ERROR: docker/alerts.yaml does not parse." >&2; exit 1; }
fi
echo "security check: ok"
