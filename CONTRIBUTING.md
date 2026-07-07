# CONTRIBUTING.md

Setup: ENVIRONMENT.md steps 1–5; confirm `bash scripts/preflight.sh` → `preflight: ok`.
Branches: trunk-based; short-lived `feat/EP-XXX-slug`; PR into main.
Coding standards: rustfmt (enforced), clippy pedantic-lite set in workspace lints, no `unwrap()` outside tests, error types per crate, module docs on every public item touched.
Tests: per TESTING.md matrix; PR fails if verify.sh fails.
Docs: same-milestone rule (AGENTS §11).
Commits: conventional-ish `EP-004: add PII gate to router`; one logical change per commit.
PR checklist: [ ] active-ExecPlan link [ ] expected-files match diff [ ] verify green [ ] docs updated [ ] security checklist [ ] Decision Log updated.
Review checklist: layer-import law, INV/TK invariants, tests assert behavior, no scope creep.
Agent-specific: agents follow AGENTS.md; humans reviewing agent PRs check the ExecPlan Progress/Decision Log were genuinely updated, not rubber-stamped.
