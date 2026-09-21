# EP-051 - Governed Bridge Activation Conformance Gate

Plan status: COMPLETE

## 1. Purpose / Big Picture

Close the remaining code-owned safety gap in prebuilt bridge activation. The
existing deploy handler verifies a component with `probe`, but it can enter
`active` without exercising the read-side schema/list/change contract that is
already available through `BridgeLifecycle::conformance`. This plan inserts
that bounded read-only gate before activation and makes failure durable.

## 2. Scope

- Add SPEC-031 and activate EP-051 as the only active plan.
- Run the existing tenant-scoped BridgeHost conformance boundary during new
  adapter activation.
- Persist conformance failure as a failed adapter transition and fail the
  governed execution.
- Add deterministic Kernel coverage for pass, fail-closed activation, and
  idempotent redeploy compatibility.
- Reconcile bridge, security, operations, readiness, decision, and historical
  plan documentation.
- Run the full local verifier without production or staging action.

## 3. Non-goals

- No generated Wasm, LLM bridge code generation, or provider passthrough.
- No new CRM abstraction, migration, SQL outside Store, or dependency.
- No autonomous canary, promotion, rollback, or write-back workflow.
- No change to the Wasmtime/WIT ABI, Governor semantics, or tenant binding.
- No production deployment, staging drill, push, tag, or production database.

## 4. Context and Orientation

EP-019 provides governed prebuilt deploy/pause/resume handlers. EP-036
provides a read-only authenticated conformance service, and the BridgeHost
already validates describe/probe/schema/list/changes-since through Wasmtime.
The current `DeployAdapterHandler` probes the component and immediately
transitions `activating` to `active`; this plan reuses the conformance method
with the same persisted grant/config and digest before that transition.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/state/execplan-index.md`
- `.agent/specs/SPEC-012-bridge-lifecycle.md`
- `.agent/specs/SPEC-016-bridge-conformance.md`
- `.agent/specs/SPEC-031-bridge-activation-conformance-gate.md`
- `crates/bridge-host/src/lifecycle.rs`
- `crates/kernel/src/bridge_runtime.rs`
- `crates/kernel/tests/bridge_lifecycle.rs`
- `crates/store/src/bridge_adapters.rs`
- `ARCHITECTURE.md`
- `NEXUS_INTEGRATION.md`
- `NEXUS_INTEGRATION_AUDIT.md`
- `OPERATIONS.md`
- `SECURITY.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`

## 6. Files to Change

- `.agent/specs/SPEC-031-bridge-activation-conformance-gate.md`
- `.agent/execplans/EP-051-bridge-activation-conformance-gate.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `crates/kernel/src/bridge_runtime.rs`
- `crates/kernel/tests/bridge_lifecycle.rs`
- `ARCHITECTURE.md`
- `NEXUS_INTEGRATION.md`
- `NEXUS_INTEGRATION_AUDIT.md`
- `OPERATIONS.md`
- `SECURITY.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`
- `.agent/execplans/EP-019-governed-bridge-activation-and-lifecycle.md`

## 7. Interfaces and Contracts

- `DeployAdapterHandler::execute` loads the digest-pinned artifact, persists
  `activating`, probes it, runs `BridgeLifecycle::conformance` with a fixed
  limit of 25 and no caller-selected kind, then transitions to `active` only
  after all checks pass.
- Conformance failure uses the existing `BridgeAdapterTransition` with
  `activating` as the expected state and `failed` as the new state. The event
  name is `activation_conformance_failed`; the stored error is bounded and
  control-character-free.
- Conformance output contributes only bounded metadata to the activation
  receipt and transition event. Raw records, secrets, prompts, and full
  customer data are never returned or logged.
- Existing active redeploy idempotence and component/grant identity checks
  remain unchanged.

## 8. Milestones

### M1 - Contract and active-plan state

Add SPEC-031, activate EP-051 in the index, extend the state checker, and
record the preflight/state baseline. Validation: `bash scripts/preflight.sh &&
bash scripts/check-execplan-state.sh`; expected output includes `preflight:
ok` and `execplan state: ok`. Recovery: repair only plan/index consistency
before touching runtime code.

### M2 - Runtime conformance gate

Update the deploy handler to run bounded conformance before activation and to
persist a fail-closed transition when it fails. Validation:
`cargo test -p hydra-kernel --test bridge_lifecycle --locked --offline --
--nocapture`; expected result is all named bridge lifecycle tests passing.
Recovery: keep activation fail closed and use the existing probe-only path
only in a test fixture while repairing the conformance call.

### M3 - Regression tests and documentation

Add a fixture-backed failure case whose probe succeeds but conformance fails;
assert `failed` state, transition history, no active state, and redacted
failure details. Preserve valid activation and idempotent redeploy tests.
Update the bridge/security/operations/readiness and historical plan notes.
Validation: the focused Kernel suite plus `git diff --check` and
`cargo fmt --all -- --check`.

### M4 - Full acceptance and closeout

Run security/dependency checks, the full verifier, and the state checker.
Expected outputs: `security check: ok`, `dependency audit: ok`, `verify: ok`,
and `execplan state: ok`. Review changed files against this section and mark
EP-051 complete only after every acceptance command passes.

## 9. Concrete Steps

1. Confirm no other plan is active and run M1 validation.
2. Read the existing `Grant`, `ConformanceRequest`, and transition APIs.
3. Clone the validated grant only for the second read-only host invocation.
4. Call conformance after probe and before the active transition.
5. Persist `activating -> failed` on conformance error; preserve the original
   error only after sanitization and bounded truncation.
6. Include conformance metadata in the successful receipt without raw data.
7. Add pass/failure/idempotence assertions to the real Kernel bridge suite.
8. Update documentation and append the decision/history evidence.
9. Run the focused gates, then the full verifier and state checker.

## 10. Validation and Acceptance

Acceptance requires:

- `bash scripts/preflight.sh` -> `preflight: ok`.
- `bash scripts/check-execplan-state.sh` -> `execplan state: ok`.
- Focused Kernel bridge lifecycle tests pass, including the conformance
  failure path.
- `cargo fmt --all -- --check` and `git diff --check` pass.
- `bash scripts/security-check.sh` -> `security check: ok`.
- `bash scripts/dependency-audit.sh` -> `dependency audit: ok`.
- `bash scripts/verify.sh` -> `verify: ok`.
- No production deployment, staging drill, push, tag, or production database
  operation occurs.

## 11. Idempotence and Recovery

The existing digest/grant identity and active redeploy rules remain the
idempotency boundary. A failed conformance attempt leaves a durable `failed`
row and a transition history entry; retry requires a new governed envelope,
which can reuse the same adapter identity only after the component and grant
match. If a transition loses a race, the Store revision check fails closed.
After interruption, rerun preflight/state, inspect the plan progress and
adapter transition history, then resume at the first unchecked milestone.

## 12. Progress

- [x] M1 - Contract and active-plan state (`preflight: ok`; `execplan state: ok` on 2026-08-12).
- [x] M2 - Runtime conformance gate (focused bridge lifecycle suite passed 2/2 before the new failure fixture and 3/3 after it).
- [x] M3 - Regression tests and documentation (focused bridge lifecycle suite passed 3/3; formatter applied; documentation and ADR updates added).
- [x] M4 - Full acceptance and closeout (`security check: ok`; `dependency audit: ok`; full `bash scripts/verify.sh` exited 0 in 818.9 seconds through the repository's terminal verifier path; `execplan state: ok` on 2026-08-12).

## 13. Surprises & Discoveries

The state checker accepts the new plan and the repository currently has no
`.env`; the latter remains only a local-services setup note and does not affect
the contract or this plan's focused fixture validation. The first failure-path
assertion expected two adapter-history rows, but Store correctly retains the
initial `registered` row, so the verified failure history contains three rows:
registered, activating, and activation_conformance_failed. Do not convert an
external or staging gap into a local pass.

## 14. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-12 | Reuse `BridgeLifecycle::conformance` instead of inventing a canary API | It already exercises the canonical Wasmtime/WIT read contract and is the smallest reversible activation hardening change. |
| 2026-08-12 | Keep the conformance limit fixed at 25 and choose the first declared kind | Activation must remain bounded and caller-independent; a future canary/promotion plan can add a separate contract. |
| 2026-08-12 | Preserve the initial registration row in failure-history assertions | Store appends registration and every revision-checked transition; the test now verifies the complete audit sequence rather than assuming activation starts at revision one. |

## 15. Outcomes & Retrospective

EP-051 is complete. The existing prebuilt Wasmtime deploy path now performs a
bounded read-only conformance check after probe and before activation. Focused
Kernel bridge lifecycle coverage passed 3/3, including a probe-only fixture
that persists `failed` rather than entering `active`. Formatting, diff,
security, dependency, preflight, state, and the full verifier passed; the full
verifier exited 0 in 818.9 seconds through its terminal path. No migration,
dependency, production deployment, staging drill, push, tag, or production
database operation occurred. Generated adapter code, autonomous canary,
promotion, and EP-010 operator/human readiness evidence remain outside this
plan.
