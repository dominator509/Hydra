# .agent/PLANS.md — Canonical ExecPlan Standard

An ExecPlan is a self-contained implementation document for one feature or system change. A new agent with no prior conversation must be able to continue from the ExecPlan alone.

## Required sections (every ExecPlan, in order)
1. Purpose / Big Picture  2. Scope  3. Non-goals  4. Context and Orientation  5. Files to Read First  6. Files to Change (== Expected Changed Files)  7. Interfaces and Contracts  8. Milestones  9. Concrete Steps  10. Validation and Acceptance  11. Idempotence and Recovery  12. Progress  13. Surprises & Discoveries  14. Decision Log  15. Outcomes & Retrospective

## Execution rules
One active plan; milestones strictly in order; validate after every milestone with the exact command and expected output written in the plan; update Progress + Decision Log immediately after each milestone; continue autonomously; STOP only per AGENTS.md §4.

## Milestone rules
Each milestone defines: goal, files to read, files to change, exact edits expected, validation command, expected result, recovery instruction. A milestone without a runnable validation command is invalid — fix the plan first (and note it in Decision Log).

## Validation & acceptance rules
Acceptance criteria are observable command outputs or HTTP responses, never vibes. Final acceptance always includes `bash scripts/verify.sh` → `verify: ok` and diff-vs-expected-files review.

## Idempotence & recovery rules
Every plan states how to resume after interruption at any milestone (usually: rerun preflight, rerun the milestone's validation to detect state, continue). Scripts and migrations must be safe to re-run.

## Progress rules
Checkbox list mirrors milestones 1:1. Tick only after validation passed. Never pre-tick.

## Decision Log rules
Every choice between alternatives, every assumption, every deviation from the plan gets a dated entry: context → decision → why smallest/reversible.

## Completion rules
All boxes ticked; Outcomes & Retrospective filled (what worked, what surprised, follow-ups); AGENTS.md §14 Definition of Done satisfied.
