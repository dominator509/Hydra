#!/usr/bin/env sh
# Install toolchain components and pinned cargo tools. Idempotent.
set -eu
[ -f AGENTS.md ] || { echo "install ERROR: run from repository root." >&2; exit 1; }
command -v rustup >/dev/null 2>&1 || { echo "install ERROR: rustup not found. Install from https://rustup.rs then rerun." >&2; exit 1; }
[ -d "$HOME/.cargo/bin" ] && PATH="$HOME/.cargo/bin:$PATH"
rustup component add rustfmt clippy >/dev/null 2>&1 || { echo "install ERROR: rustup component add failed." >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1; }
need_cargo_subcommand() { cargo "$1" --version >/dev/null 2>&1; }
need_cargo_subcommand audit || cargo install cargo-audit --locked --version 0.22.2
need_cargo_subcommand deny || cargo install cargo-deny --locked --version 0.19.9
need wasm-tools    || cargo install wasm-tools --locked --version 1.240.0
need_cargo_subcommand sqlx || cargo install sqlx-cli --no-default-features --features postgres --locked --version 0.8.6
need rg            || echo "install NOTE: ripgrep (rg) not found — install via OS package manager." >&2
need jq            || echo "install NOTE: jq not found — install via OS package manager." >&2
echo "install: ok"
