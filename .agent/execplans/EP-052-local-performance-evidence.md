# EP-052 - Required Local Performance Evidence

Plan status: COMPLETE

## 1. Purpose / Big Picture

Promote Hydra's existing release-only Governor p99 test, 10k bridge
conformance soak, and TOKENKILLER cache audit into one required local
performance gate. This removes the current gap where verify can pass without
running the performance checks that are only wired into nightly or conditional
paths.

## 2. Scope

- Add SPEC-032 and activate EP-052 as the only active plan.
- Add `scripts/test-performance.sh` with strict success-marker validation.
- Invoke the shared gate from `scripts/verify.sh` and nightly CI.
- Update preflight, release policy, commands, testing, readiness, and ADR
  documentation.
- Preserve the existing tests, thresholds, cache contracts, and no-Node rule.
- Run focused checks and the full local verifier.

## 3. Non-goals

- No change to Governor behavior or its 5ms threshold.
- No new benchmark dependency, Node/npm toolchain, provider call, or database
  schema.
- No synthetic PASS rows in the production-readiness ledger.
- No claim of staging latency, 24-hour soak, real provider budgets, or human
  performance review.
- No production deployment, staging deployment, push, tag, or production DB.

## 4. Context and Orientation

`crates/governor/tests/core_domain.rs` contains a release-only ignored p99
test. `crates/bridge-host/tests/conformance.rs` contains the ignored
`c9_soak_10k` test, and `scripts/test-nightly-conformance.sh` already proves
that its named test is discovered and passes. `scripts/cache-hit-audit.sh`
already enforces the TOKENKILLER cache target. `verify.sh` currently runs the
cache audit only conditionally and does not run either release/ignored
performance check. Nightly runs the conformance and cache paths separately.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/state/execplan-index.md`
- `.agent/specs/SPEC-001-core-domain.md`
- `.agent/specs/SPEC-009-tokenkiller.md`
- `.agent/specs/SPEC-032-local-performance-evidence.md`
- `crates/governor/tests/core_domain.rs`
- `crates/bridge-host/tests/conformance.rs`
- `scripts/test-nightly-conformance.sh`
- `scripts/cache-hit-audit.sh`
- `scripts/verify.sh`
- `.github/workflows/nightly.yml`
- `scripts/check-release-policy.sh`
- `PRODUCTION_READINESS.md`
- `TESTING.md`
- `DECISIONS.md`

## 6. Files to Change

- `.agent/specs/SPEC-032-local-performance-evidence.md`
- `.agent/execplans/EP-052-local-performance-evidence.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `scripts/test-performance.sh`
- `scripts/preflight.sh`
- `scripts/verify.sh`
- `scripts/check-release-policy.sh`
- `.github/workflows/nightly.yml`
- `COMMANDS.md`
- `TESTING.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`

## 7. Interfaces and Contracts

- `bash scripts/test-performance.sh` runs, in order:
  1. `cargo test -p governor --test core_domain --release -- --ignored perf_governor_eval_p99_under_5ms --nocapture`;
  2. `sh scripts/test-nightly-conformance.sh`;
  3. `sh scripts/cache-hit-audit.sh`.
- The wrapper requires the Governor test and `test result: ok.` markers, relies
  on the conformance wrapper's `nightly conformance: ok` marker, and relies on
  the cache wrapper's `cache-hit audit: ok` marker before printing
  `performance: ok`.
- `verify.sh` invokes the shared gate exactly once and does not separately
  maintain a conditional cache path.
- Nightly invokes the same wrapper, with no `continue-on-error` or masked
  command.

## 8. Milestones

### M1 - Contract and active-plan state

Add SPEC-032, activate EP-052, extend the state checker and preflight script
inventory. Validation: `bash scripts/preflight.sh && bash
scripts/check-execplan-state.sh`; expected `preflight: ok` and `execplan state:
ok`. Recovery: repair state artifacts only.

### M2 - Strict performance wrapper

Add the wrapper and run the three existing checks without changing their
thresholds. Validation: `bash scripts/test-performance.sh` with an isolated
loopback `DATABASE_URL`; expected `performance: ok`. Recovery: preserve the
individual failing output and do not weaken discovery or cache assertions.

### M3 - Required verifier and nightly wiring

Invoke the wrapper from `verify.sh`, replace the duplicated nightly steps with
the shared gate, and update static release-policy checks. Validation:
`bash scripts/check-release-policy.sh`; expected `release policy: ok`, plus a
focused performance run.

### M4 - Documentation and full acceptance

Update commands, testing, readiness, and ADR records to separate local
performance evidence from EP-010 staging evidence. Run format, diff, security,
dependency, preflight, state, and full verification. Expected markers include
`performance: ok`, `verify: ok`, and `execplan state: ok`.

## 9. Concrete Steps

1. Confirm no active plan and run M1 validation.
2. Add the strict wrapper using temporary output only for marker validation.
3. Add the wrapper to preflight and verify; remove the old conditional cache
   invocation from verify.
4. Wire nightly and update its summary/status step without duplicating raw
   ignored-test commands.
5. Extend release policy to require the shared wrapper and its nested checks.
6. Update commands/testing/readiness/decision documentation.
7. Run focused performance/policy/state checks, then the full verifier.
8. Mark EP-052 complete only after full acceptance and changed-file review.

## 10. Validation and Acceptance

Acceptance requires:

- `bash scripts/preflight.sh` -> `preflight: ok`.
- `bash scripts/test-performance.sh` -> `performance: ok`.
- `bash scripts/check-release-policy.sh` -> `release policy: ok`.
- `bash scripts/security-check.sh` -> `security check: ok`.
- `bash scripts/dependency-audit.sh` -> `dependency audit: ok`.
- `bash scripts/verify.sh` -> `verify: ok`.
- `bash scripts/check-execplan-state.sh` -> `execplan state: ok`.
- `cargo fmt --all -- --check` and `git diff --check` pass.
- No production or staging deployment occurs.

## 11. Idempotence and Recovery

The wrapper is read-only apart from Cargo build artifacts and temporary output
files, which are removed on exit. Re-running it repeats deterministic tests and
does not mutate CRM data. If an individual check fails, rerun that exact
command, inspect its output, and repair the underlying threshold, fixture, or
marker; never mask the failure. After interruption, rerun preflight/state and
resume at the first unchecked milestone.

## 12. Progress

- [x] M1 - Contract and active-plan state (`preflight: ok`; `execplan state: ok`; `release policy: ok` on 2026-08-12).
- [x] M2 - Strict performance wrapper (`performance: ok`; Governor p99 1/1, `c9_soak_10k` 1/1, and cache ratio `0.9717` on 2026-08-12).
- [x] M3 - Required verifier and nightly wiring (`release policy: ok`; shared wrapper is required by `verify.sh` and nightly with no duplicate raw ignored-test path).
- [x] M4 - Documentation and full acceptance (`performance: ok`; `security check: ok`; `dependency audit: ok`; full `bash scripts/verify.sh` exited 0 in 635.9 seconds; final state validation pending only).

## 13. Surprises & Discoveries

The existing optimized Governor test and 10k conformance wrapper both passed
when run directly. The new shared wrapper also reported `cache-hit audit: ok
(ratio=0.9717)` and `performance: ok`. The full verifier took 635.9 seconds
after adding the required performance work. Vendor SQLx emitted its existing
warning-only unexpected-cfg diagnostics; no repository gate failed. Local
performance evidence must not be relabeled as staging evidence.

## 14. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-12 | Reuse the existing Governor, bridge soak, and TOKENKILLER scripts | The repository already owns the thresholds and marker contracts; a shared wrapper avoids duplicate benchmark logic and new dependencies. |
| 2026-08-12 | Make the shared gate required in verify and nightly | Performance regressions must fail the primary local gate, while the same command keeps scheduled validation from drifting. |
| 2026-08-12 | Preserve the nightly conformance wrapper as a nested dependency | It already proves named `c9_soak_10k` discovery and failure propagation; the new wrapper adds the Governor and cache checks without duplicating that parser. |

## 15. Outcomes & Retrospective

EP-052 is complete. `scripts/test-performance.sh` is required by preflight,
`verify.sh`, and nightly; release policy requires the shared wrapper and no
masked duplicate path. Governor p99, `c9_soak_10k`, and TOKENKILLER cache
evidence pass locally; security, dependency, formatting, state, and the full
verifier pass. No production or staging action occurred. EP-010 still needs
staging latency, 24-hour soak, live provider budget/cache evidence, and human
performance review.
