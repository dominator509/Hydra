# REPO_BRIEF — Hydra

## Purpose

Use this file as the fast repo-orientation note before deeper reading. It is intentionally compact and points back to the real authority surface.

## Authority Order

1. Explicit user instruction
2. `AGENTS.md`
3. The single active ExecPlan
4. Existing code and tests
5. `ARCHITECTURE.md`
6. Relevant `.agent/specs/*`

## First Reads

- `README.md`
- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- Active ExecPlan in `.agent/execplans/`
- `PROJECT_BRIEF.md`
- `DECISIONS.md` when dependency or runtime choices are in scope

## Working Loop

1. Run preflight.
2. Work one ExecPlan only.
3. Validate after each milestone.
4. Update the ExecPlan progress and decision log as you go.
5. End with `bash scripts/verify.sh` and confirm `verify: ok`.

## Important Roots

- `crates/`: Rust workspace code
- `.agent/`: plans, specs, prompts, checklists
- `.sqlx/`: committed offline query metadata for checked SQLx macros
- `vendor/sqlx/` and `vendor/sqlx-macros-core/`: repo-local Postgres-only SQLx hardening patch; touch only when SQLx build or audit work points there
- `scripts/`: allowed command wrappers
- `reference/`: informative copy-adapt implementations
- `docker/`: local services

## Local-State Rules

- `.obsidian/` is local-only.
- `.serena/project.yml` is versioned.
- `.serena/project.local.yml`, `.serena/cache/`, and `.serena/memories/` are local-only.
- `target/` and local env files stay out of commits.

## Command Hygiene

- Prefix shell commands with `rtk`.
- On this machine, Git Bash via `C:/Progra~1/Git/usr/bin/sh.exe` is the reliable path for repo shell scripts.
- Use `rtk proxy cmd /c git ...` when native git output is the cleanest Windows path.
- Re-run `cargo sqlx prepare --workspace -- --all-targets` after checked-query edits so `.sqlx/` stays authoritative for both runtime code and integration-test macros.

## When Git Looks Wrong

- Check `git status --short --branch --ignored`.
- Check `git remote -v`.
- Check `git rev-list --left-right --count origin/main...HEAD`.
- Do not assume a clean push path until branch divergence is proven.
