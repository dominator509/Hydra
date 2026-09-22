#!/usr/bin/env sh
# Keep the encrypted-vault dependency pinned and free of the retired macro edge.
set -eu

[ -f AGENTS.md ] || {
  echo "age vault dependency policy ERROR: run from repository root." >&2
  exit 1
}

EXPECTED='age = { version = "=0.12.1", default-features = false }'
grep -Fqx "$EXPECTED" Cargo.toml || {
  echo "age vault dependency policy ERROR: age pin or feature boundary drifted." >&2
  exit 1
}

GRAPH=$(cargo tree --workspace --locked --target all)
case "$GRAPH" in
  *"age v0.12.1"*) ;;
  *)
    echo "age vault dependency policy ERROR: age 0.12.1 is absent from the locked graph." >&2
    exit 1
    ;;
esac
case "$GRAPH" in
  *"age v0.11."*|*"proc-macro-error2 v"*|*"i18n-embed-fl v0.9."*)
    echo "age vault dependency policy ERROR: retired age macro dependency path is present." >&2
    exit 1
    ;;
esac

echo "age vault dependency policy: ok"
