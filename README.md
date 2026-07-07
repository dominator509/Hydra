# Hydra

HYDRA is a self-hosted, Rust-first agentic meta-CRM that combines a canonical CRM data model, a deterministic autonomy governor, TOKENKILLER-mediated LLM routing, and WASM-bridged legacy CRM integrations.

## What Is In This Repo

- A real Rust workspace bootstrapped through EP-001.
- Control-plane docs and execution rules under `.agent/`.
- Reference implementations under `reference/` for copy-and-adapt work.
- Validation scripts under `scripts/` that define the only supported command surface.

## Start Here

1. Read `AGENTS.md`.
2. Read `COMMANDS.md`.
3. Read `.agent/PLANS.md`.
4. Open the single active ExecPlan in `.agent/execplans/`.
5. Run preflight and confirm `preflight: ok`.

On this Windows machine, the reliable preflight path is:

```bash
rtk C:/Progra~1/Git/usr/bin/sh.exe -lc 'bash scripts/preflight.sh'
```

## Local Boot

When you need the full EP-003 persistence seam locally:

```bash
rtk proxy cmd /c docker compose -f docker/compose.yaml up -d postgres nats
rtk C:/Progra~1/Git/usr/bin/sh.exe -lc 'bash scripts/db-setup.sh'
rtk cargo run -p hydra-kernel
rtk C:/Progra~1/Git/usr/bin/sh.exe -lc 'bash scripts/smoke-test.sh'
```

`.sqlx/` is versioned on purpose. Keep it in sync with checked-query changes by rerunning `cargo sqlx prepare --workspace -- --all-targets` after migration or query edits so integration-test macros stay covered too.

The workspace intentionally vendors `vendor/sqlx/` and `vendor/sqlx-macros-core/` as a Postgres-only SQLx hardening layer so `cargo audit` and `cargo deny` reflect the backend Hydra actually ships.

## Execution Model

- Work exactly one ExecPlan at a time.
- Validate every milestone with the command written in the plan.
- Update the plan itself as progress is made.
- Finish with `bash scripts/verify.sh` and confirm `verify: ok`.
- Treat `ROADMAP.md` as strategy only, never as implementation authority.

## Repo Map

- `crates/cdm` and `crates/governor`: L1 core domain.
- `crates/store`: persistence boundary.
- `crates/fabric`, `crates/agents`, `crates/tokenkiller`, `crates/bridge-host`: service/runtime layers.
- `crates/shell`: server-rendered UI.
- `docker/`: local Postgres and NATS support.
- `migrations/`, `wit/`, `wiring/`, `adapters/`: schema, ABI, and integration assets.

`ARCHITECTURE.md` is the authoritative boundary and import-law document.

## Git And Command Discipline

- Prefix external shell commands with `rtk`.
- Keep commits scoped to one logical change.
- Keep `.sqlx/` tracked; it is part of the repo's offline-compile contract.
- Before pushing, prove branch state with:

```bash
rtk proxy cmd /c git status --short --branch --ignored
rtk proxy cmd /c git rev-list --left-right --count origin/main...HEAD
```

- Do not commit local workspace state from `.obsidian/`, `.serena/memories/`, `.serena/cache/`, or `.serena/project.local.yml`.

## Key Docs

- `PROJECT_BRIEF.md`: product scope and success metrics.
- `REPO_BRIEF.md`: compact repo-orientation note for Serena, Obsidian, and handoffs.
- `ARCHITECTURE.md`: layers, invariants, and allowed dependencies.
- `ENVIRONMENT.md`: env surface and local setup expectations.
- `TESTING.md`: validation matrix.
- `CONTRIBUTING.md`: branch, commit, and review expectations.

## Serena And Obsidian

- `REPO_BRIEF.md` is the durable entrypoint note for knowledge tools.
- `.serena/project.yml` is the versioned repo profile.
- `.serena/project.local.yml` is the local override surface and stays ignored.
- `.obsidian/` is intentionally local-only to avoid publishing personal vault state.

## Current Working Assumption

Foundation is in place, and implementation proceeds by advancing the current ExecPlan rather than rebuilding the blueprint pack. When in doubt, re-anchor in the active plan and rerun preflight before editing.
