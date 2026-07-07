# Prompt: Final Review of an ExecPlan

Read AGENTS.md, COMMANDS.md, and [EXECPLAN_PATH].

1. Run `bash scripts/verify.sh` — require `verify: ok`.
2. If the plan touches deploy/ops (EP-009/EP-010), also run `bash scripts/production-readiness-check.sh`.
3. Run `git diff --name-only` (against the plan's start ref if recorded, else main). Compare with Files to Change; every extra file needs a Decision Log justification — otherwise the review FAILS.
4. Walk Validation and Acceptance: re-execute each acceptance command; record actual outputs next to expected.
5. Confirm docs-updated rule (AGENTS §11) for any behavior/env/schema change in the diff.
6. Fill Outcomes & Retrospective: what shipped, deviations, risks, follow-ups.
7. Produce the final report: ExecPlan completed?, changed files, commands+results, acceptance status per criterion, decisions, assumption changes, remaining risks, production-readiness status if applicable.
Do not fix new feature work during review; file follow-ups instead (smallest-change rule).
