#!/usr/bin/env sh
set -eu
[ -f AGENTS.md ] || { echo "db setup ERROR: run from repository root." >&2; exit 1; }
[ -f Cargo.toml ] || { echo "db setup ERROR: workspace not initialized. Execute EP-001 first." >&2; exit 1; }
[ -d "$HOME/.cargo/bin" ] && PATH="$HOME/.cargo/bin:$PATH"
cargo sqlx --version >/dev/null 2>&1 || { echo "db setup ERROR: cargo sqlx missing. Run: bash scripts/install.sh" >&2; exit 1; }
: "${DATABASE_URL:=postgres://hydra:hydra@localhost:5432/hydra}"
export DATABASE_URL
cargo sqlx database create
cargo sqlx migrate run
echo "db setup: ok"
