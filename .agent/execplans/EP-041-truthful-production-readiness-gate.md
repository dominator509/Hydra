# EP-041 - Truthful Production-Readiness Gate

Plan status: COMPLETE

## 1. Purpose / Big Picture

Make the EP-010 production-readiness gate fail closed on incomplete evidence.
The current launch-table check accepts any non-empty row, which means a
`PENDING` or `BLOCKED` result can pass after the local gates and D1-D5 happen
to be present. This plan adds exact, dated, status-aware evidence validation
and executable fixture coverage without fabricating staging or human evidence.

## 2. Scope

Implement SPEC-021 as a small POSIX-shell evidence library used by the
production-readiness command. Add tests for current and synthetic ledgers,
wire the contract into preflight/full verification, and reconcile the
operator documentation and readiness status.

## 3. Non-goals

- No staging or production deployment, drill, database, provider, or human
  sign-off.
- No changing `OPERATIONS.md` or `PRODUCTION_READINESS.md` pending evidence to
  `PASS`.
- No runtime service, API, schema, migration, retention, purge, backup, or
  alerting implementation.
- No new dependency, shell dialect, or external test service.
- No change to EP-010's partial production-readiness status.
- No push, tag, merge, or release.

## 4. Context and Orientation

EP-020 through EP-040 improved local recovery, identity, execution, bridge,
scheduling, and observability behavior, while EP-010 remains blocked on
operator-owned staging evidence. `scripts/production-readiness-check.sh`
currently validates D1-D5 with status/date checks but only checks that four
launch-table strings appear in non-empty rows. SPEC-021 defines the missing
truthful boundary. The parser must remain separate from runtime authority and
must not imply that local tests are staging evidence.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/state/execplan-index.md`
- `.agent/specs/SPEC-020-scheduler-concurrency-and-observability.md`
- `scripts/production-readiness-check.sh`
- `scripts/preflight.sh`
- `scripts/verify.sh`
- `OPERATIONS.md`
- `PRODUCTION_READINESS.md`
- `TESTING.md`
- `DECISIONS.md`

## 6. Files to Change

- `.agent/specs/SPEC-021-truthful-production-readiness-gate.md`
- `.agent/execplans/EP-041-truthful-production-readiness-gate.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `scripts/readiness-evidence.sh`
- `scripts/test-readiness-evidence.sh`
- `scripts/production-readiness-check.sh`
- `scripts/preflight.sh`
- `scripts/verify.sh`
- `COMMANDS.md`
- `TESTING.md`
- `OPERATIONS.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`

## 7. Interfaces and Contracts

- `scripts/readiness-evidence.sh` exposes
  `readiness_evidence_check <operations-file> <readiness-file> <now-epoch> <max-age-days>`
  for the production wrapper and fixture tests.
- On failure the function returns non-zero and sets the shell variable
  `READINESS_EVIDENCE_ERROR` to a deterministic reason; it never edits an
  evidence file.
- Drill rows are matched by exact first-column ID, exact `PASS` status, ISO
  date, non-placeholder evidence, and non-placeholder operator.
- Launch rows are matched by exact first-column check name, exact `PASS`
  result, ISO date, and non-placeholder owner. Required rows include
  security, performance, privacy, accessibility, and observability reviews,
  not only the four legacy checks.
- All evidence dates are bounded to 0 through 30 UTC days old.
- The production wrapper retains the existing terminal signal
  `production-readiness:ok` only after every local and evidence gate passes.

## 8. Milestones

### M1 - Contract and active-plan state

Add SPEC-021 and activate EP-041 as the only active plan. Extend the state
checker for EP-041. Validate with `bash scripts/preflight.sh` and
`bash scripts/check-execplan-state.sh`; expect `preflight: ok` and
`execplan state: ok`. Recovery: correct index/plan status or section order.

### M2 - Evidence parser

Implement exact table parsing, date freshness, placeholder rejection, and
explicit PASS checks. Validate with the focused fixture script; expect
`readiness evidence: ok`. Recovery: narrow the parser to the documented table
columns and preserve fail-closed behavior.

### M3 - Gate wiring and truthful tests

Replace the loose launch-table grep with the parser, add the focused test to
preflight and full verification, and ensure the current pending ledger still
fails the production gate for the real D1 reason. Validate with the focused
script and a direct production-gate invocation up to its first expected
failure; no staging evidence is allowed.

### M4 - Documentation and decision reconciliation

Document the exact evidence contract and local-only boundary in COMMANDS,
TESTING, OPERATIONS, and PRODUCTION_READINESS, and append ADR-0051. Validate
with preflight, state, and `git diff --check`; expect all required `ok`
signals.

### M5 - Full acceptance

Run the focused evidence suite, all policy gates, and
`bash scripts/verify.sh`; expect `verify: ok`. Review changed files against
this plan. No production action is authorized.

## 9. Concrete Steps

1. Confirm EP-040 is complete and no active plan exists.
2. Add the accepted SPEC-021, active EP-041, index row, transition, and state
   checker entry.
3. Implement the reusable parser and fixture tests with deterministic failures
   for pending, stale, malformed, and duplicate evidence.
4. Wire the parser into the production gate, preflight, and verify scripts.
5. Update command, testing, operations, readiness, and decision documents.
6. Run milestone validations in order and record exact outputs below.

## 10. Validation and Acceptance

- `bash scripts/preflight.sh` -> `preflight: ok`.
- `bash scripts/check-execplan-state.sh` -> `execplan state: ok`.
- `bash scripts/test-readiness-evidence.sh` -> `readiness evidence: ok`.
- A pending launch table fails despite D1-D5 PASS fixture rows.
- A valid recent PASS fixture succeeds.
- Stale, malformed, duplicate, future, and placeholder rows fail closed.
- The checked-in ledger remains blocked because D1-D5 have no PASS evidence.
- `bash scripts/verify.sh` -> `verify: ok`.
- No production or staging action occurs.

## 11. Idempotence and Recovery

The plan changes no database schema and is safe to rerun. The focused test
creates and removes only temporary files under the system temp directory. If
the parser fails, rerun the focused fixture script before changing the gate.
If full verification is interrupted, rerun it with the documented disposable
Postgres/NATS environment; do not convert local results into launch evidence.

## 12. Progress

- [x] M1 - Contract and active-plan state (`preflight: ok`; `execplan state: ok`)
- [x] M2 - Evidence parser (`sh scripts/test-readiness-evidence.sh` -> `readiness evidence: ok`; parser bounded to the first drill evidence table)
- [x] M3 - Gate wiring and truthful tests (focused fixtures pass; checked-in ledger fails closed without D1 evidence; full readiness gate first exposed and then avoided a procedure-example row)
- [x] M4 - Documentation and decision reconciliation (`preflight: ok`; `execplan state: ok`; `git diff --check` passed)
- [x] M5 - Full acceptance (current-state `bash scripts/verify.sh` exited 0 in 297.2s through `verify: ok`; full readiness command correctly failed at D1 evidence)

## 13. Surprises & Discoveries

The Windows PowerShell wrapper expands unescaped POSIX `$(date ...)` and
`$VAR` expressions before Git Bash receives them; the documented scripts
themselves run correctly, while direct diagnostics through the wrapper must
avoid unescaped shell substitutions. The first full readiness-gate run found
the later illustrative D1 procedure row (`<ISO date>`) rather than the live
table. The parser is now explicitly bounded to the first evidence table, and
the real ledger fails at missing D1 PASS evidence.

## 14. Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-08-12 | Require explicit PASS, recent date, owner, and evidence for every launch row | The existing non-empty grep accepts PENDING/BLOCKED text and is false-green. |
| 2026-08-12 | Keep staging and human evidence absent in the checked-in ledger | AGENTS.md forbids fabricated readiness evidence and EP-010 remains partial. |
| 2026-08-12 | Use a dependency-free sourced POSIX-shell library plus fixture tests | The existing gate is shell-based; this avoids a new toolchain and shares one parser between production and tests. |
| 2026-08-12 | Validate the real ledger through the parser with no staging invocation | The checked-in `OPERATIONS.md` intentionally lacks D1 PASS evidence; a fail-closed result is the only truthful local outcome. |

## 15. Outcomes & Retrospective

Completed 2026-08-12. The production-readiness gate now shares one exact,
fail-closed parser for the EP-010 drill and final launch tables. It rejects
placeholder, pending, stale, future, malformed, duplicate, and incomplete
rows; it also ignores illustrative procedure examples outside the first live
drill evidence table. Fixture tests passed, the current ledger remained
blocked at missing D1 PASS evidence, and the current-state full verifier
exited 0 in 297.2s through `verify: ok`. Preflight, ExecPlan state, and diff
checks passed. No staging or production evidence was fabricated, and EP-010
remains partial.
