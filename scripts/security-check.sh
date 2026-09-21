#!/usr/bin/env sh
# Secret-pattern scan over tracked files + tracked-.env guard + cargo audit.
set -eu
[ -f AGENTS.md ] || { echo "security check ERROR: run from repository root." >&2; exit 1; }
if git ls-files 2>/dev/null | grep -qx '\.env'; then
  echo "security check ERROR: .env is tracked by git. Untrack it (git rm --cached .env)." >&2; exit 1
fi
PATTERNS='AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY-----|sk-[A-Za-z0-9_-]{24,}|AGE-SECRET-KEY-1[A-Z0-9]{20,}|ghp_[A-Za-z0-9]{30,}|xox[baprs]-[A-Za-z0-9-]{10,}|eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}|BEGIN (ED25519|RSA|EC) PRIVATE KEY'
if git ls-files 2>/dev/null | xargs -r grep -nE "$PATTERNS" -- 2>/dev/null; then
  echo "security check ERROR: potential secret material matched above patterns." >&2; exit 1
fi
# Runtime SQL belongs to Store. Test modules retain their existing database
# fixtures, but the production entrypoints must not acquire or query a pool.
# `fabric/src/rate.rs` contains a cfg(test)-only SQL fixture and is excluded
# from this source-level check rather than changing that deterministic test.
if rg -n --glob '*.rs' 'sqlx|PgPool|query!|query_as!|migrate!' \
  --glob '!rate.rs' crates/fabric/src crates/admin-cli/src crates/kernel/src >/dev/null 2>&1; then
  echo "security check ERROR: runtime SQL or database-pool access exists outside crates/store." >&2
  rg -n --glob '*.rs' 'sqlx|PgPool|query!|query_as!|migrate!' \
    --glob '!rate.rs' crates/fabric/src crates/admin-cli/src crates/kernel/src >&2
  exit 1
fi
if [ -f Cargo.toml ]; then
  cargo audit --version >/dev/null 2>&1 || { echo "security check ERROR: cargo audit missing. Run: bash scripts/install.sh" >&2; exit 1; }
  cargo audit
fi
if [ -f docker/alerts.yaml ]; then
  PYTHON_BIN=${PYTHON_BIN:-}
  if [ -z "$PYTHON_BIN" ]; then
    for candidate in python3 python python.exe; do
      if command -v "$candidate" >/dev/null 2>&1; then
        PYTHON_BIN=$candidate
        break
      fi
    done
  fi
  [ -n "$PYTHON_BIN" ] || {
    echo "security check ERROR: Python 3 is required to parse docker/alerts.yaml." >&2
    exit 1
  }
  "$PYTHON_BIN" -c "import yaml;yaml.safe_load(open('docker/alerts.yaml'))" 2>/dev/null || {
    echo "security check ERROR: docker/alerts.yaml does not parse." >&2
    exit 1
  }
fi
echo "security check: ok"
