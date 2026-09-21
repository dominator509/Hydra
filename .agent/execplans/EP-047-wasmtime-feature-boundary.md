# EP-047 Wasmtime Feature Boundary

Plan status: COMPLETE

## 1. Purpose / Big Picture

Remove an avoidable supply-chain edge from Hydra's Wasmtime runtime by
disabling Wasmtime's broad default feature set and naming only the features
required by the existing async Component Model/WASI adapter ABI. Preserve the
remaining upstream `age` maintenance warning as an explicit residual rather
than changing the vault cryptography boundary.

## 2. Scope

- Activate the one-plan state for EP-047.
- Add SPEC-027 and a mandatory dependency feature-graph check.
- Narrow the direct Wasmtime feature set without changing WIT, adapter grants,
  runtime construction, or component behavior.
- Verify the all-target dependency graph, audit/deny policy, bridge tests, and
  the full repository gate.
- Reconcile supply-chain documentation and EP-010 residuals honestly.

## 3. Non-goals

- No Wasmtime or age version upgrade.
- No adapter ABI, WIT, bridge capability, or runtime behavior redesign.
- No advisory ignore, direct forcing dependency, or dependency replacement.
- No production deployment, registry action, tag, push, or production DB use.
- No claim that local dependency evidence closes EP-010 staging or human gates.

## 4. Context and Orientation

EP-045 removed the unsound `event-listener` release from the resolved graph.
The remaining `fxhash` warning is pulled by `fxprof-processed-profile`, which
is enabled by the direct Wasmtime default feature set even though Hydra does
not use profiling. The `proc-macro-error2` warning is an unconditional
upstream edge of the pinned `age` release and cannot be removed by a feature
flag. The Wasmtime/WIT boundary is security-sensitive, so the smallest safe
change is an explicit feature declaration followed by real bridge conformance
and all-target dependency validation.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/state/execplan-index.md`
- `Cargo.toml`
- `Cargo.lock`
- `deny.toml`
- `crates/bridge-host/Cargo.toml`
- `crates/bridge-host/src/host.rs`
- `crates/bridge-host/src/lib.rs`
- `wit/hydra-bridge.wit`
- `scripts/security-check.sh`
- `scripts/dependency-audit.sh`
- `SECURITY.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`
- `.agent/specs/SPEC-027-wasmtime-feature-boundary.md`

## 6. Files to Change

- `Cargo.toml`
- `Cargo.lock`
- `scripts/check-execplan-state.sh`
- `scripts/check-wasmtime-features.sh`
- `scripts/dependency-audit.sh`
- `COMMANDS.md`
- `SECURITY.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`
- `.agent/specs/SPEC-027-wasmtime-feature-boundary.md`
- `.agent/execplans/EP-047-wasmtime-feature-boundary.md`
- `.agent/state/execplan-index.md`

## 7. Interfaces and Contracts

- `wasmtime` remains exactly pinned at `36.0.13`, with
  `default-features = false` and explicit `async`, `component-model`,
  `cranelift`, `runtime`, and `std` features.
- `wasmtime-wasi` remains exactly pinned at `36.0.13`.
- `scripts/check-wasmtime-features.sh` fails if the manifest omits the
  explicit boundary or if the locked all-target graph contains `fxhash`.
- `scripts/dependency-audit.sh` runs the feature-boundary check before
  `cargo deny check` and preserves the existing `dependency audit: ok`
  terminal marker.
- The WIT ABI, component loading, grants, and runtime error contracts remain
  unchanged.

## 8. Milestones

### M1 - Activate the contract and state

Add SPEC-027, EP-047, the index row/transition, and state-checker coverage.
Run `bash scripts/preflight.sh` and `bash scripts/check-execplan-state.sh`.
Expected: `preflight: ok` and `execplan state: ok`. Recovery: fix plan
structure or state mismatch before changing dependencies.

### M2 - Narrow the Wasmtime graph

Set the direct Wasmtime dependency to explicit minimum runtime features and
refresh the locked graph. Run
`cargo tree --locked --target all -e features --offline` and
`cargo check -p bridge-host --tests --locked --offline`.
Expected: no `fxprof-processed-profile`/`fxhash` edge and a successful bridge
host check. Recovery: restore the prior feature declaration and isolate the
missing feature with a narrower check; do not disable the adapter runtime.

### M3 - Make the boundary mandatory and document residuals

Add the static/graph check, wire it into dependency audit, and update
commands, security, readiness, and ADR documentation. Run
`bash scripts/check-wasmtime-features.sh`,
`bash scripts/security-check.sh`, and `bash scripts/dependency-audit.sh`.
Expected: `wasmtime feature policy: ok`, `security check: ok`, and
`dependency audit: ok`. Recovery: keep the plan active and correct the exact
graph or policy failure; never add an ignore to obtain green output.

### M4 - Exercise the preserved adapter boundary

Run `cargo test -p bridge-host --locked --offline -- --nocapture` and
`cargo test -p hydra-kernel bridge_lifecycle --locked --offline -- --nocapture`.
Expected: existing component, grant, lifecycle, and malformed-configuration
tests pass with no behavior change. Recovery: inspect the first compile or
test failure and keep the feature set explicit.

### M5 - Full acceptance and truthful closeout

Run the unchanged full verifier against disposable loopback services with
resource-safe compiler settings. Expected: `verify: ok`, a clean state check,
and no production action. Recovery: record exact failure evidence, fix only
the bounded root cause, and leave EP-047 active if acceptance is incomplete.

## 9. Concrete Steps

1. Confirm EP-046 is complete and no plan is active.
2. Add SPEC-027 and activate EP-047 in the authoritative index.
3. Extend the state checker for EP-047 and validate plan structure.
4. Change only the direct Wasmtime feature declaration and refresh Cargo.lock.
5. Add the mandatory feature-graph gate and wire it into dependency audit.
6. Run focused dependency and bridge validation after each boundary change.
7. Update `COMMANDS.md`, `SECURITY.md`, `PRODUCTION_READINESS.md`, and
   `DECISIONS.md` with the exact evidence and remaining `age` residual.
8. Run the full verifier and reconcile EP-047 to COMPLETE only after every
   acceptance signal passes.

## 10. Validation and Acceptance

EP-047 is accepted only when:

- the state checker finds one valid EP-047 plan and no second active plan;
- the manifest explicitly disables Wasmtime defaults and names required
  features;
- the locked all-target graph contains no `fxhash` edge;
- the WIT adapter and Kernel bridge lifecycle tests pass;
- `cargo audit` reports no vulnerabilities and only the documented upstream
  `proc-macro-error2` maintenance warning if still present;
- `cargo deny check`, security, dependency, preflight, and full verification
  pass without masking;
- documentation distinguishes removed avoidable warnings from retained
  upstream residuals; and
- no production deployment or production database operation occurs.

## 11. Idempotence and Recovery

The manifest edit is idempotent and the graph gate is read-only. Re-running
Cargo resolution must preserve the exact pinned versions and the no-`fxhash`
property. If the reduced feature set cannot compile or conformance changes,
restore only the explicit feature list, record the missing capability, and
leave EP-047 ACTIVE rather than weakening sandbox or ABI behavior.

## 12. Progress

- [x] M1 - Contract and active-plan state (`preflight: ok`; `execplan state: ok`, 2026-08-12)
- [x] M2 - Wasmtime feature graph (`bash scripts/check-wasmtime-features.sh` -> `wasmtime feature policy: ok`; `cargo check -p bridge-host --tests --locked --offline` -> exit 0, 2026-08-12)
- [x] M3 - Mandatory gate and documentation (`wasmtime feature policy: ok`; `security check: ok`; `dependency audit: ok`, 2026-08-12)
- [x] M4 - Adapter boundary regression tests (`cargo test -p bridge-host --test lifecycle` -> 5 passed with disposable `DATABASE_URL`; `cargo test -p hydra-kernel bridge_lifecycle` -> 1 passed, 2026-08-12)
- [x] M5 - Full acceptance and closeout (`bash scripts/verify.sh` exited 0 in 1,381.5 seconds through terminal `verify: ok`; `execplan state: ok`; `git diff --check` passed; locked graph contains no profiling/fxhash path, 2026-08-12)

## 13. Surprises & Discoveries

- Initial discovery: `fxhash` is reachable through Wasmtime's default
  `profiling` feature and not through Hydra's requested Component Model alone.
- Initial discovery: the pinned `age` release declares its i18n dependencies
  unconditionally, so `proc-macro-error2` cannot be removed by an age feature
  toggle.
- No feature change is accepted until the existing Wasmtime component tests
  and all-target dependency graph are both green.

## 14. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-12 | Select explicit Wasmtime defaults as the next code-owned seam | Removes an avoidable unmaintained dependency without changing the ABI or adding a new authority surface. |
| 2026-08-12 | Preserve `age` and document `proc-macro-error2` | The vault cryptographic boundary is stable and the remaining edge is upstream/unconditional; replacement is broader risk than this plan permits. |
| 2026-08-12 | Activate EP-047 only after the state validator passed | The one-active-plan invariant and required preflight gate remain authoritative before dependency edits. |
| 2026-08-12 | Keep the reduced graph after full adapter and repository verification | BridgeHost lifecycle `5/5`, Kernel bridge lifecycle `1/1`, and the full verifier passed; the existing WIT ABI and sandbox boundary remain intact. |

## 15. Outcomes & Retrospective

EP-047 is complete. The direct Wasmtime dependency now disables unused
defaults and explicitly names the runtime features required by Hydra's async
Component Model/WASI adapter boundary. The locked all-target graph contains
no `fxhash` or `fxprof-processed-profile` path, while the existing BridgeHost
and Kernel lifecycle suites and the full verifier pass. The pinned age
library's upstream `proc-macro-error2` maintenance warning remains visible,
and EP-010 staging, recovery, operational, and human readiness evidence remain
outside this local plan.
