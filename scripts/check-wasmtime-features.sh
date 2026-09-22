#!/usr/bin/env sh
# Keep the Wasmtime adapter boundary explicit and free of unused profiling deps.
set -eu

[ -f AGENTS.md ] || {
  echo "wasmtime feature policy ERROR: run from repository root." >&2
  exit 1
}

EXPECTED='wasmtime = { version = "=36.0.14", default-features = false, features = ["async", "component-model", "cranelift", "runtime", "std"] }'
grep -Fqx "$EXPECTED" Cargo.toml || {
  echo "wasmtime feature policy ERROR: direct Wasmtime feature boundary drifted." >&2
  exit 1
}

GRAPH=$(cargo tree --workspace --locked --target all)
case "$GRAPH" in
  *"fxhash v"*|*"fxprof-processed-profile v"*)
    echo "wasmtime feature policy ERROR: profiling dependency path is present." >&2
    exit 1
    ;;
esac

echo "wasmtime feature policy: ok"
