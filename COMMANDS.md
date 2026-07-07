# COMMANDS.md — The Only Allowed Commands

Working directory rule: ALL commands run from the repository root. Scripts refuse to run elsewhere.
Package manager rule: cargo only. npm/pnpm/yarn/pip are forbidden in this repository.

> Coding agents must not invent commands. If a command is missing, update this file first with evidence from the repository (file path + line) in the same commit.

| Purpose | Command | Success signal |
|---|---|---|
| Preflight | `bash scripts/preflight.sh` | `preflight: ok` |
| Install | `bash scripts/install.sh` | `install: ok` |
| Lint | `bash scripts/lint.sh` | `lint: ok` |
| Format check | `bash scripts/format-check.sh` | `format check: ok` |
| Typecheck | `bash scripts/typecheck.sh` | `typecheck: ok` |
| Unit tests | `bash scripts/test-unit.sh` | `unit tests: ok` |
| Integration tests | `bash scripts/test-integration.sh` | `integration tests: ok` |
| E2E tests | `bash scripts/test-e2e.sh` | `e2e tests: ok` |
| Build | `bash scripts/build.sh` | `build: ok` |
| Security check | `bash scripts/security-check.sh` | `security check: ok` |
| Dependency audit | `bash scripts/dependency-audit.sh` | `dependency audit: ok` |
| Smoke test | `bash scripts/smoke-test.sh` | `smoke test: ok` |
| Full verification | `bash scripts/verify.sh` | `verify: ok` |
| Cache-hit audit (TOKENKILLER) | `bash scripts/cache-hit-audit.sh` | `cache-hit audit: ok (ratio=0.9XX)` |
| Production readiness | `bash scripts/production-readiness-check.sh` | `production readiness: ok` |
| Local dev (stateful services) | `docker compose up -d postgres nats` | containers healthy |
| Local dev (kernel+shell) | `cargo run -p hydra-kernel` | `hydra: listening on :8080` log line |
| Local DB setup | `bash scripts/db-setup.sh` (created in EP-003) | `db setup: ok` |
| Migrations | `cargo sqlx migrate run` (after EP-003) | `Applied N migrations` |
| Single crate check (diagnostic) | `cargo check -p <crate>` | exit 0 |
| Single test (diagnostic) | `cargo test -p <crate> <name> -- --nocapture` | exit 0 |

Underlying tool expectations (installed by scripts/install.sh): rustup toolchain 1.79+, `cargo fmt`, `cargo clippy`, `cargo audit`, `cargo deny`, `cargo sqlx` (sqlx-cli), `wasm-tools`, `docker compose`, `jq`, `curl`, `rg`.

## Forbidden commands
- Anything with `sudo` outside scripts/install.sh's documented tool installs.
- `git push --force`, history rewrites.
- `DROP DATABASE`, `TRUNCATE`, raw `psql` against non-test DBs.
- `rm -rf` outside `target/`, `/tmp`, or explicitly listed build dirs.
- Any npm/npx/node invocation.
- `curl | sh` style pipe-installs not listed in scripts/install.sh.

## Recovery instructions
- Script fails → read its stderr; each script names the failing sub-step. Apply AGENTS.md §7 bounded retry.
- sqlx compile-time query errors → `bash scripts/db-setup.sh && cargo sqlx prepare --workspace`.
- Wasmtime/adapter build fails → `wasm-tools validate adapters/<name>.wasm` for a narrower diagnostic.
- docker services unhealthy → `docker compose logs --tail=50 postgres nats`.
