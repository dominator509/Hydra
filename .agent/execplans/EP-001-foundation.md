# EP-001 Foundation

## 1. Purpose / Big Picture
Create the Rust workspace skeleton, quality gates (fmt/clippy/audit/deny), test harness, compose file for Postgres+NATS, all scripts/ wired to real commands, and CI — so every later plan lands on green rails.

## 2. Scope
Workspace Cargo.toml + empty-but-compiling crates per ARCHITECTURE repo map; rust-toolchain.toml; deny.toml; docker/compose.yaml (postgres, nats only); .github/workflows/ci.yml running scripts/verify.sh; scripts/* replaced from placeholders to real commands; .env.example; vendored htmx placeholder dir.

## 3. Non-goals
No domain logic, no DB schema, no HTTP routes, no LLM code, no adapters. shell crate = "hello" route only? NO — no routes at all yet; kernel main prints listening line and serves /healthz only (needed by smoke).

## 4. Context and Orientation
Layer law in ARCHITECTURE.md governs crate deps. Scripts already contain the real cargo commands guarded by a loud "workspace not initialized — execute EP-001" failure; this plan creates the workspace so those guards clear, then VERIFIES each gate runs green (edit scripts only if a command proves wrong — Decision Log); integration/e2e/smoke get real-but-minimal targets (`cargo test --test smoke_healthz` style) and full meaning arrives in later plans.

## 5. Files to Read First
ARCHITECTURE.md (repo map, layer table), COMMANDS.md, ENVIRONMENT.md, scripts/*.sh, TESTING.md.

## 6. Files to Change
Cargo.toml, rust-toolchain.toml, deny.toml, crates/{kernel,cdm,governor,store,bridge-host,bridge-wit,llm-router,tokenkiller,agents,fabric,shell}/ (Cargo.toml+src/lib.rs or main.rs), docker/compose.yaml, docker/Dockerfile (stub ok), .github/workflows/ci.yml, scripts/*.sh (all 15+db-setup stub), .env.example, .gitignore, migrations/.gitkeep, wit/.gitkeep, adapters/.gitkeep, wiring/.gitkeep.

## 7. Interfaces and Contracts
kernel binary: boots config::validate() (only DATABASE_URL/NATS_URL optional at this stage—warn not fail), binds HYDRA_BIND default 127.0.0.1:8080, GET /healthz → 200 "ok", logs `hydra: listening on <addr>`. Workspace lints: `unwrap_used = "deny"` (allow in tests), rustfmt default.

## 8. Milestones
M1 Workspace compiles. Read: ARCHITECTURE map. Change: Cargo.toml + all crate stubs + toolchain file. Edits: workspace members list exactly the 11 crates; each lib.rs `//! layer Lx` doc + empty; kernel main per §7 using axum+tokio+tracing. Validation: `cargo check --workspace` then `echo check: ok`. Expected: `check: ok`. Recovery: `cargo check -p <failing>` narrow.
M2 Gates real. Change: scripts/lint.sh→cargo clippy --workspace --all-targets -- -D warnings; format-check.sh→cargo fmt --check; typecheck.sh→cargo check --workspace; test-unit.sh→cargo test --workspace --lib; build.sh→cargo build --workspace --release; security-check.sh→(gitleaks-regex grep block from reference in script comment)+cargo audit; dependency-audit.sh→cargo deny check; install.sh→rustup component add + cargo install pins per ENVIRONMENT. Validation: `bash scripts/lint.sh && bash scripts/format-check.sh && bash scripts/typecheck.sh && bash scripts/test-unit.sh`. Expected: four `: ok` lines. Recovery: fix clippy findings smallest-first; do NOT blanket-allow lints (Decision Log if any allow added).
M3 Services compose. Change: docker/compose.yaml (postgres:16 healthcheck pg_isready; nats:2.10 -js), .env.example, db-setup stub `scripts/db-setup.sh` (createdb via sqlx database create; prints `db setup: ok`). Validation: `docker compose -f docker/compose.yaml up -d && sleep 5 && docker compose -f docker/compose.yaml ps --format '{{.Name}} {{.Status}}' | grep -c healthy` → ≥1 then echo `compose: ok`. Expected: `compose: ok`. Recovery: `docker compose logs postgres nats --tail 30`.
M4 Smoke + e2e minimal. Change: crates/kernel/tests/smoke_healthz.rs (spawn app, GET /healthz==200); scripts/smoke-test.sh→curl /healthz against running kernel or `cargo test -p hydra-kernel --test smoke_healthz`; test-integration.sh→`cargo test --workspace --test '*' -- --skip e2e_`; test-e2e.sh→`cargo test --workspace -- e2e_ --ignored || true` TEMPORARY with echo note+`e2e tests: ok` ONLY IF zero e2e tests exist yet (checked via grep) — record in Decision Log; real e2e lands EP-005. Validation: `bash scripts/smoke-test.sh`. Expected: `smoke test: ok`. Recovery: kernel not running → tests spawn their own instance; ensure port 0 binding in test.
M5 CI + verify. Change: ci.yml (checkout, rust cache, services: postgres+nats containers, run `bash scripts/verify.sh`); verify.sh already sequences all scripts. Validation: `bash scripts/verify.sh`. Expected: `verify: ok`. Recovery: the failing sub-script names itself; drill into it.

## 9. Concrete Steps
Milestone order; commit after each with `EP-001: M<n> <slug>`.

## 10. Validation and Acceptance
Acceptance: verify.sh → `verify: ok`; kernel run prints listening line; `git diff --name-only` ⊆ §6; CI file lints (`actionlint` if available else yaml parse via python? NO — do not invent tools; visual check + push optional).

## 11. Idempotence and Recovery
cargo/compose idempotent; resume: run verify.sh, first failing gate = resume point.

## 12. Progress
- [ ] M1 - [ ] M2 - [ ] M3 - [ ] M4 - [ ] M5

## 13. Surprises & Discoveries
## 14. Decision Log
- 2026-07-06 - User-directed repo bootstrap added `README.md`, root `.gitignore`, and a trimmed `.serena/project.yml` before EP-001 implementation. Smallest reversible choice to document command discipline, keep local editor state out of git, and make Serena Rust-aware without changing product code or relaxing EP-001 acceptance gates.
## 15. Outcomes & Retrospective
