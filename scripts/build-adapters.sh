#!/usr/bin/env sh
set -eu
[ -f AGENTS.md ] || { echo "adapters ERROR: run from repository root." >&2; exit 1; }
[ -f Cargo.toml ] || { echo "adapters ERROR: workspace not initialized. Execute EP-001 first." >&2; exit 1; }
[ -d "$HOME/.cargo/bin" ] && PATH="$HOME/.cargo/bin:$PATH"
wasm-tools --version >/dev/null 2>&1 || { echo "adapters ERROR: wasm-tools missing. Run: bash scripts/install.sh" >&2; exit 1; }
rustup target list --installed | grep -qx 'wasm32-wasip2' || {
  echo "adapters ERROR: rustup target wasm32-wasip2 missing. Run: rustup target add wasm32-wasip2" >&2
  exit 1
}
cargo build -p adapter-memcrm --target wasm32-wasip2 --release
mkdir -p adapters
cp target/wasm32-wasip2/release/adapter_memcrm.wasm adapters/memcrm.wasm
echo "adapters: ok"
