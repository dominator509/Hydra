# EP-000 Discovery Report

Captured on 2026-07-07 from the repository root before EP-000 file edits.

## tree

Command: `find . -maxdepth 2 -not -path './.git*' | sort`

```text
.
./.agent
./.agent/checklists
./.agent/execplans
./.agent/EXECUTION_RULES.md
./.agent/PLANS.md
./.agent/prompts
./.agent/specs
./.agent/templates
./.agents
./.obsidian
./.obsidian/app.json
./.obsidian/appearance.json
./.obsidian/core-plugins.json
./.obsidian/workspace.json
./.serena
./.serena/.gitignore
./.serena/cache
./.serena/memories
./.serena/project.local.yml
./.serena/project.yml
./AGENTS.md
./ARCHITECTURE.md
./ASSUMPTIONS.md
./COMMANDS.md
./CONTRIBUTING.md
./DECISIONS.md
./DEPLOYMENT.md
./ENVIRONMENT.md
./HOW-TO-USE.md
./MASTERPROMPT-INPUTS-FILLED.md
./OBSERVABILITY.md
./OPERATIONS.md
./PRODUCTION_READINESS.md
./PROJECT_BRIEF.md
./README.md
./reference
./reference/bridge
./reference/governor.rs
./reference/README.md
./reference/router.rs
./reference/tokenkiller
./RELEASE.md
./ROADMAP.md
./ROLLBACK.md
./scripts
./scripts/build.sh
./scripts/cache-hit-audit.sh
./scripts/dependency-audit.sh
./scripts/format-check.sh
./scripts/install.sh
./scripts/lint.sh
./scripts/preflight.sh
./scripts/production-readiness-check.sh
./scripts/security-check.sh
./scripts/smoke-test.sh
./scripts/test-e2e.sh
./scripts/test-integration.sh
./scripts/test-unit.sh
./scripts/typecheck.sh
./scripts/verify.sh
./SECURITY.md
./TESTING.md
```

## git status

Commands:
- `git status --porcelain`
- `git log --oneline -5 || true`

Observed status:
- `git status --porcelain` produced no tracked-file output; the checkout was clean before EP-000 edits.
- Git emitted ambient stderr warnings about unreadable `C:\Users\domin\.config\git\ignore`.

Recent history:

```text
f0c500d Initial-Hydra-blueprint-import
```

## toolchain versions

```text
rustc version: 1.96.1 (31fca3adb 2026-06-26)
cargo version: 1.96.1 (356927216 2026-06-26)
docker version: 29.5.3, build d1c06ef
docker compose version: v5.1.4
jq version: jq-1.8.1
rg version: ripgrep 15.1.0
wasm-tools: MISSING
cargo sqlx: MISSING
cargo audit version: cargo-audit-audit 0.22.2
cargo deny version: cargo-deny 0.19.9
```

Notes:
- `docker --version` and `docker compose version` both worked, but each emitted a warning that `C:\Users\domin\.docker\config.json` was not readable.
- `wasm-tools` is not on PATH.
- `cargo sqlx` is not installed.

## stack detection

Observed facts:
- No `Cargo.toml` exists yet at repo root or depth 2, so the Rust workspace is not initialized.
- No Node package manifests or lockfiles were found at depth 2.
- Product-code surface is currently blueprint docs, shell scripts, and informative `reference/` code only.
- The repo already carries the intended architecture and command contract for a Rust-only implementation:
  - server-rendered shell via Axum + Askama + vendored htmx
  - Postgres + NATS JetStream as stateful services
  - Wasmtime/WIT bridge runtime
  - TOKENKILLER and multi-LLM routing planned but not yet materialized in source crates

Package-manifest probe:

```text
PKG_FILES:
```

Test-surface probe:

```text
TEST_SURFACE:
./.agent/execplans/EP-007-testing-hardening.md
./.agent/specs
./.agent/templates/spec-template.md
./.agent/templates/test-case-template.md
./reference/bridge
./reference/bridge/conformance.rs
./reference/bridge/host.rs
./reference/bridge/hydra-bridge.wit
./reference/governor.rs
./reference/README.md
./reference/router.rs
./reference/tokenkiller
./reference/tokenkiller/canon.rs
./reference/tokenkiller/contracts.rs
./reference/tokenkiller/ledger.rs
./reference/tokenkiller/nukeguard.rs
./reference/tokenkiller/prefix.rs
./scripts/build.sh
./scripts/cache-hit-audit.sh
./scripts/dependency-audit.sh
./scripts/format-check.sh
./scripts/install.sh
./scripts/lint.sh
./scripts/preflight.sh
./scripts/production-readiness-check.sh
./scripts/security-check.sh
./scripts/smoke-test.sh
./scripts/test-e2e.sh
./scripts/test-integration.sh
./scripts/test-unit.sh
./scripts/typecheck.sh
./scripts/verify.sh
./TESTING.md
```

## CI detection

Command result:

```text
CI: absent
```

Interpretation:
- `.github/workflows/` does not exist yet.
- EP-001 M5 is still responsible for introducing CI.

## env detection

Root listing:

```text
.
..
.agent
.agents
.git
.gitignore
.obsidian
.serena
AGENTS.md
ARCHITECTURE.md
ASSUMPTIONS.md
COMMANDS.md
CONTRIBUTING.md
DECISIONS.md
DEPLOYMENT.md
ENVIRONMENT.md
HOW-TO-USE.md
MASTERPROMPT-INPUTS-FILLED.md
OBSERVABILITY.md
OPERATIONS.md
PRODUCTION_READINESS.md
PROJECT_BRIEF.md
README.md
RELEASE.md
ROADMAP.md
ROLLBACK.md
SECURITY.md
TESTING.md
reference
scripts
```

Env-file probe:

```text
ENV_FILES:
absent
```

Interpretation:
- No `.env` or `.env.example` exists yet.
- ENVIRONMENT.md documents the future env surface, but the actual example file still belongs to EP-001.

## risks

- The repo is still a docs/scripts/reference blueprint pack, so every Cargo-based validation script past preflight will fail until EP-001 creates the workspace.
- `wasm-tools` and `cargo sqlx` are missing; EP-001 install work will need them before later plans can validate bridge and database milestones.
- Docker is installed, but user-level Docker config is unreadable on this machine, so compose-related milestones may need a narrower follow-up check if the warning becomes functional.
- Local-only tooling directories `.agents`, `.obsidian`, and `.serena` are present in the checkout tree and can confuse discovery unless treated as non-product state.
- Git emits permission warnings for the user-level ignore file; this is noisy but did not block clean status or history inspection.

## missing info

- No active Cargo workspace exists yet, so crate layout, real dependency graph, and actual test binaries remain unmaterialized.
- No CI workflow exists yet.
- No `.env.example` exists yet.
- No runtime source code exists outside `reference/`.
