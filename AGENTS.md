# AGENTS.md — Control Plane for Coding Agents (HYDRA)

## 1. Mission
Implement HYDRA to production readiness by executing ExecPlans in `.agent/execplans/` exactly, one at a time, with machine-verifiable validation at every milestone.

## 2. Source-of-truth priority (highest wins)
1. Current explicit user instruction
2. This AGENTS.md
3. The single active ExecPlan
4. Existing repository code and tests
5. ARCHITECTURE.md
6. The relevant SPEC in `.agent/specs/`
7. ROADMAP.md (never implement from it directly)
`reference/` code is INFORMATIVE (copy-and-adapt source); specs are NORMATIVE.

## 3. Required workflow
1. Read AGENTS.md → COMMANDS.md → `.agent/PLANS.md` → the active ExecPlan (only one may be active; see `.agent/EXECUTION_RULES.md`).
2. Run `bash scripts/preflight.sh`; expect `preflight: ok`.
3. Complete milestones IN ORDER. After each: run its validation command, confirm expected output, tick Progress, append Decision Log entries.
4. Continue autonomously to plan completion. Do not ask the user for next steps. Stop only under STOP conditions (§4).
5. Finish with the Definition of Done (§14) and Final response (§15).

## 4. STOP conditions (the ONLY reasons to stop)
- Missing required secret, credential, paid service, or external account (e.g., DEEPSEEK_API_KEY absent and the milestone requires live-provider validation).
- Any action that may destroy user or production data (drops, truncations, purges outside test DBs).
- Legal / security / financial judgment not already specified in SECURITY.md or the constitution.
- Materially different user-visible behavior choices not resolved by the spec.
- Required tests cannot run after the documented recovery attempts in §7.
- Production deployment or irreversible migration without explicit permission.
When stopping, report: exact blocker, evidence (file/terminal output), smallest decision needed, recommended default.

## 5. Anti-drift rules
- Touch only files listed in the active ExecPlan "Files to Change". Extra files require a Decision Log justification.
- No broad refactors, renames, reorganizations, dependency swaps, or style rewrites unless the ExecPlan demands them.
- Non-goals are binding. `git diff --name-only` is compared against Expected Changed Files at final review.

## 6. Anti-hallucination rules
- Do not invent package APIs, command names, environment variables, database tables, routes, config keys, WIT symbols, or cargo features.
- Confirm every name by reading repository files first (`rg`, `cat`). Use only commands from COMMANDS.md.
- Copy hard code from `reference/` and adapt; do not re-derive it from memory.
- Record every assumption in the ExecPlan Decision Log and, if durable, ASSUMPTIONS.md.

## 7. Anti-fixation rules (bounded retry)
1st failure: read the exact error; smallest targeted fix.
2nd same-root failure: build/run a NARROWER diagnostic (single test, `cargo check -p <crate>`, minimal repro).
3rd same-root failure: stop that approach, write failed hypotheses to Surprises & Discoveries, choose the simpler implementation path consistent with the spec, continue.
Never patch blindly around the same error. Never delete a failing test to pass.

## 8. Dependency rules
Check existing workspace deps first (`cargo tree -p <crate>`). Prefer std / existing deps. New crate additions require: necessity note in Decision Log + DECISIONS.md entry + `cargo deny check` green. Pin versions. Forbidden: any npm/Node dependency; native plugin systems bypassing Wasmtime.

## 9. File creation rules
Follow ARCHITECTURE.md repo map. New crates only when an ExecPlan says so. Generated artifacts (wasm, sqlx-data) go to their designated dirs. Never commit secrets, .env, target/, *.wasm build junk outside `adapters/`.

## 10. Testing rules
Every milestone has a validation command; run it. New behavior ships with tests per TESTING.md matrix. `bash scripts/verify.sh` must print `verify: ok` before an ExecPlan is Complete. Tests use ephemeral Postgres schemas (sqlx test) — never a shared/prod DB.

## 11. Documentation update rules
If behavior, commands, env vars, or schema change: update COMMANDS.md / ENVIRONMENT.md / ARCHITECTURE.md / relevant SPEC in the same milestone. Stale docs = failing acceptance.

## 12. Security rules
See SECURITY.md. Non-negotiables: secrets by vault name only; adapters sandboxed with grants; PII→`private`-tagged providers only (structural gate); no LLM in the Governor path; append-only audit; parameterized SQL only.

## 13. Production data rules
There is no production data in this repo. Any command targeting a non-test database, or any `DROP/TRUNCATE/DELETE` without `WHERE tenant_id = :test`, is a STOP condition.

## 14. Definition of done (per ExecPlan)
- All acceptance criteria pass; all validation commands pass with expected outputs.
- ExecPlan Progress fully ticked; Surprises/Decisions/Outcomes filled.
- `git diff --name-only` ⊆ Expected Changed Files (or justified).
- `bash scripts/verify.sh` → `verify: ok`.
- Remaining risks documented in the ExecPlan Outcomes.

## 15. Final response requirements
Report: ExecPlan completed; changed files; commands run + results; acceptance criteria status; decisions made; assumptions confirmed/changed; remaining risks; whether production-readiness criteria (if applicable) passed.

> Do not ask the user for next steps. Proceed autonomously through the active ExecPlan unless a STOP condition applies.
