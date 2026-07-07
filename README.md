# Hydra

HYDRA is an agentic meta-CRM blueprint aimed at unifying CRM data, guarded automation, and token-disciplined multi-LLM workflows in a self-hosted Rust system.

## Current State

This checkout currently contains the HYDRA blueprint pack and control-plane documents. The repository is expected to begin from `.agent/execplans/EP-001-foundation.md`; `scripts/preflight.sh` currently reports `workspace not initialized yet`, and that note is expected until EP-001 creates the Rust workspace skeleton.

## Start Here

- `AGENTS.md` defines the repo control plane and stop conditions.
- `COMMANDS.md` is the only allowed command surface.
- `PROJECT_BRIEF.md` captures product scope, outcomes, and success metrics.
- `ARCHITECTURE.md` defines layer rules, invariants, and the intended repo map.
- `.agent/execplans/EP-001-foundation.md` is the foundation plan preflight points to for first implementation work.

## Working Rules

1. Prefix shell commands with `rtk`.
2. On Windows, prefer Git Bash for the repo scripts. Example: `rtk 'C:/Program Files/Git/usr/bin/bash.exe' scripts/preflight.sh`
3. Work one ExecPlan at a time and update the plan itself as progress is made.
4. Before declaring a plan complete, run `bash scripts/verify.sh` and confirm `verify: ok`.
5. Treat `ROADMAP.md` as strategy only; implementation authority lives in the active ExecPlan plus the repo docs above it.

## Common Commands

- `bash scripts/preflight.sh`
- `bash scripts/install.sh`
- `bash scripts/verify.sh`
- `bash scripts/production-readiness-check.sh`

See `COMMANDS.md` for the full command list and required success strings.

## Repo Constraints

- Cargo only. No Node, npm, or pnpm.
- Only files named by the active ExecPlan should change unless the plan's Decision Log records why.
- `reference/` is informative copy-adapt source, not normative product code.

## Serena And Obsidian

- `PROJECT_BRIEF.md` is the durable orientation note for knowledge tools.
- `.serena/project.yml` keeps Serena focused on Rust plus the repo's control documents.
- `.obsidian/` is user-local workspace state and should stay out of commits.
