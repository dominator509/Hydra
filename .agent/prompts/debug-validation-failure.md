# Prompt: Debug a Failing Validation Command

Read AGENTS.md and COMMANDS.md. The failing context: ExecPlan [EXECPLAN_PATH], command `[FAILING_COMMAND]`.

Rules:
1. Do not rewrite unrelated code. Scope = making THIS command pass without violating specs/tests.
2. Capture the exact failing command and full error output; paste both into Surprises & Discoveries.
3. Form ONE hypothesis. Make the smallest fix consistent with it.
4. Rerun the NARROWEST relevant command first (`cargo check -p <crate>`, single test with `-- --nocapture`), then the original command.
5. Bounded retry: 2nd same-root failure → build a narrower diagnostic (minimal repro test, targeted rg/cat inspection). 3rd same-root failure → abandon the approach; record all failed hypotheses in Surprises & Discoveries; pick the simpler implementation path allowed by the spec; continue.
6. Never delete or weaken a failing test to pass. Never patch around the same error repeatedly without a new hypothesis.
7. Update the ExecPlan (Progress unchanged until validation passes; Decision Log for the fix rationale). Report per AGENTS.md §15.
