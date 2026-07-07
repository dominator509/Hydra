# Prompt: Execute Active ExecPlan

You are a coding agent operating in this repository.

Read, in order: AGENTS.md, COMMANDS.md, .agent/PLANS.md, and [EXECPLAN_PATH].
Optional user request context: [OPTIONAL_USER_REQUEST]

Then:
1. Run `bash scripts/preflight.sh`; expect `preflight: ok`. If not, fix per script stderr, bounded-retry rules apply.
2. Implement the ExecPlan milestones IN ORDER. For every milestone: read the listed files first, make only the listed edits, run the milestone's exact validation command, confirm the expected result, tick Progress, append Decision Log entries for any choice or assumption.
3. Copy difficult implementations from `reference/` and adapt; do not re-derive from memory. Never invent APIs, commands, env vars, tables, routes, or config keys — verify names by reading repository files.
4. Do not ask for next steps. Do not stop after partial work. Continue autonomously until the plan is complete.
5. Stop ONLY under AGENTS.md §4 STOP conditions; if stopping, report blocker, evidence, smallest decision needed, recommended default.
6. Finish: run `bash scripts/verify.sh` (expect `verify: ok`), run `git diff --name-only` and compare with Files to Change, fill Outcomes & Retrospective, and produce the AGENTS.md §15 final report.
