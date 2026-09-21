# EP-043 - Ingress-Aware Smoke Validation

Plan status: COMPLETE

## 1. Purpose / Big Picture

Repair the stale public smoke assumption exposed by EP-042. The smoke script
currently fetches `/metrics` from `HYDRA_SMOKE_URL`, but public Caddy ingress
must now deny that path. Keep public health/readiness checks and make metrics
validation explicitly internal-only.

## 2. Scope

Implement SPEC-023 in `scripts/smoke-test.sh`, add deterministic fake-curl
boundary tests, wire them into preflight/full verification, and update the
environment, deployment, testing, command, readiness, and decision records.

## 3. Non-goals

- No reopening public `/metrics` or changing the Caddy boundary.
- No runtime API, database schema, migration, metrics payload, or Prometheus
  topology change.
- No public authentication, dashboard, alert receiver, staging smoke run, or
  production deployment.
- No new dependency or shell/runtime toolchain.
- No push, tag, merge, release, or production database operation.

## 4. Context and Orientation

EP-042 made Caddy return `404` for `/metrics*` while preserving internal
Prometheus scraping. `scripts/smoke-test.sh` still has a public branch that
requests `$HYDRA_SMOKE_URL/metrics`, which is now an incorrect validation
contract and would fail a real ingress smoke. The local branch starts the real
Kernel directly and should keep its metrics assertions. The public branch must
use a separate optional internal URL for metrics.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/state/execplan-index.md`
- `.agent/specs/SPEC-022-public-metrics-boundary.md`
- `scripts/smoke-test.sh`
- `scripts/test-ingress-policy.sh`
- `scripts/preflight.sh`
- `scripts/verify.sh`
- `ENVIRONMENT.md`
- `DEPLOYMENT.md`
- `TESTING.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`

## 6. Files to Change

- `.agent/specs/SPEC-023-ingress-aware-smoke-validation.md`
- `.agent/execplans/EP-043-ingress-aware-smoke-validation.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `scripts/smoke-test.sh`
- `scripts/test-smoke-boundary.sh`
- `scripts/preflight.sh`
- `scripts/verify.sh`
- `COMMANDS.md`
- `ENVIRONMENT.md`
- `TESTING.md`
- `DEPLOYMENT.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`

## 7. Interfaces and Contracts

- `HYDRA_SMOKE_URL` remains the public URL for `/healthz` and `/readyz`.
- `HYDRA_SMOKE_INTERNAL_METRICS_URL` is optional and is used only for
  internal metrics validation in the public smoke branch.
- Equal public/internal URLs fail closed.
- With no public URL, the existing direct Kernel smoke test continues to
  validate `/metrics`.
- The successful public branch prints an explicit internal-only metrics skip
  when no internal metrics URL is supplied.

## 8. Milestones

### M1 - Contract and active-plan state

Add SPEC-023, activate EP-043, extend the state checker/index, and record the
EP-042 regression. Validate with `bash scripts/preflight.sh` and
`bash scripts/check-execplan-state.sh`; expect `preflight: ok` and
`execplan state: ok`. Recovery: fix state/index before code edits.

### M2 - Smoke boundary implementation and fixtures

Update the public smoke branch and add fake-curl tests for public-only,
public-plus-internal, and equal-URL cases. Validate with
`sh scripts/test-smoke-boundary.sh`; expect `smoke boundary tests: ok`.

### M3 - Verification and documentation wiring

Add the fixture test to preflight/full verification and document the new
variable and boundary. Validate with preflight, state, focused smoke tests,
and `git diff --check`.

### M4 - Full acceptance

Run `bash scripts/verify.sh` against the documented disposable loopback
Postgres/NATS services; expect `verify: ok`. No staging or production smoke
run is authorized.

## 9. Concrete Steps

1. Confirm EP-042 is complete and no active plan exists.
2. Add SPEC-023, active EP-043, state row, transition, and checker entry.
3. Separate public health/readiness from optional internal metrics in the
   smoke script.
4. Add fake-curl tests and wire the test into required gates.
5. Update docs and ADR-0053 without claiming staging evidence.
6. Run milestone validations in order and record exact outputs below.

## 10. Validation and Acceptance

- `bash scripts/preflight.sh` -> `preflight: ok`.
- `bash scripts/check-execplan-state.sh` -> `execplan state: ok`.
- `sh scripts/test-smoke-boundary.sh` -> `smoke boundary tests: ok`.
- Public smoke never requests `$HYDRA_SMOKE_URL/metrics`.
- Optional internal metrics validation uses only
  `$HYDRA_SMOKE_INTERNAL_METRICS_URL`.
- Equal public/internal URLs fail.
- `bash scripts/verify.sh` -> `verify: ok`.
- No staging or production action occurs.

## 11. Idempotence and Recovery

No database or runtime state changes. Fixture tests create only temporary fake
executables and logs and remove them through a trap. If the public branch
fails, inspect the fake-curl log before changing the script. If full
verification is interrupted, rerun it against disposable loopback services.

## 12. Progress

- [x] M1 - Contract and active-plan state (`preflight: ok`; `execplan state: ok`)
- [x] M2 - Smoke boundary implementation and fixtures (`smoke boundary tests: ok`)
- [x] M3 - Verification and documentation wiring (preflight/state/focused tests/diff checks passed)
- [x] M4 - Full acceptance (current-state `bash scripts/verify.sh` exited 0 in 367.8s through `verify: ok`; no staging smoke run occurred)

## 13. Surprises & Discoveries

The stale public metrics request was not covered by the existing local smoke
path because `HYDRA_SMOKE_URL` is unset during repository verification. A
fake-curl fixture was required to exercise the public branch without a live
ingress service; it proved the public URL is used only for health/readiness.

## 14. Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-08-12 | Preserve `HYDRA_SMOKE_URL` for public health/readiness and add a separate internal metrics URL | EP-042 intentionally denies public metrics, so the old public smoke request is invalid. |
| 2026-08-12 | Skip public metrics explicitly when no internal URL is supplied | Public smoke must not invent a route or imply that a skipped internal check passed. |
| 2026-08-12 | Reject equal public/internal URLs | Prevents configuration from accidentally using the denied public ingress for metrics. |

## 15. Outcomes & Retrospective

Completed 2026-08-12. Public smoke validation now uses
`HYDRA_SMOKE_URL` only for `/healthz` and `/readyz`, skips metrics explicitly
when no separate internal URL is supplied, and validates metrics only through
`HYDRA_SMOKE_INTERNAL_METRICS_URL` when configured. Equal URLs fail closed;
the direct local Kernel smoke path still validates `/metrics`. Preflight,
state, focused fake-curl tests, diff checks, and the current-state full
verifier passed; `verify.sh` exited 0 in 367.8s through `verify: ok`. No
staging or production smoke run occurred, and EP-010 remains partial.
