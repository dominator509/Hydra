# EP-036 - Governed Bridge Conformance

Plan status: COMPLETE

## 1. Purpose / Big Picture

Make the existing `bridge-conformance` A2A workflow truthful and executable
for configured digest-pinned adapters. Add a read-only BridgeHost validator
that exercises the existing WIT contract through Store-resolved tenant state,
then expose only bounded metadata and counts through the authenticated durable
A2A task boundary.

## 2. Scope

Implement SPEC-016 without changing the CRM model, the Wasmtime ABI, the
Governor mutation path, or the existing activation/synchronization handlers.
Wire a Kernel-owned conformance service into Fabric's existing A2A workflow
catalog and add deterministic fixture, tenant-isolation, and no-mutation tests.

## 3. Non-goals

- No adapter activation, pause, resume, synchronization, canary, or promotion.
- No generated code, Wasm output, model call, or provider passthrough.
- No ActionEnvelope, CRM mutation, audit/outbox event, migration, or dependency.
- No full-relist scheduler or autonomous bridge worker.
- No staging deployment or EP-010 production-readiness claim.

## 4. Context and Orientation

EP-019 provides tenant-scoped prebuilt adapter lifecycle and EP-035 provides
governed manual incremental synchronization. BridgeHost already exposes
`describe`, `probe`, `introspect_schema`, `list`, and `changes_since`; the A2A
catalog currently advertises `bridge-conformance` as unavailable. Conformance
must reuse the existing Store adapter record and host grant boundary rather
than accepting component paths, digests, grants, or tenant IDs from callers.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/specs/SPEC-012-bridge-lifecycle.md`
- `.agent/specs/SPEC-015-bridge-synchronization.md`
- `.agent/specs/SPEC-016-bridge-conformance.md`
- `crates/bridge-host/src/lifecycle.rs`
- `crates/bridge-host/src/host.rs`
- `crates/bridge-host/tests/conformance.rs`
- `crates/bridge-host/tests/lifecycle.rs`
- `crates/kernel/src/bridge_runtime.rs`
- `crates/kernel/src/runtime_services.rs`
- `crates/fabric/src/services.rs`
- `crates/fabric/src/rest/a2a.rs`
- `crates/fabric/tests/a2a_workflows.rs`
- `crates/store/src/bridge_adapters.rs`
- `ARCHITECTURE.md`
- `SECURITY.md`
- `TESTING.md`

## 6. Files to Change

Expected changed files for this plan:

- `.agent/specs/SPEC-016-bridge-conformance.md`
- `.agent/execplans/EP-036-bridge-conformance.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `COMMANDS.md`
- `crates/bridge-host/src/lifecycle.rs`
- `crates/bridge-host/src/lib.rs`
- `crates/bridge-host/tests/lifecycle.rs`
- `crates/kernel/src/bridge_runtime.rs`
- `crates/kernel/src/runtime_services.rs`
- `crates/kernel/src/main.rs`
- `crates/kernel/tests/bridge_lifecycle.rs`
- `crates/fabric/src/services.rs`
- `crates/fabric/src/lib.rs`
- `crates/fabric/src/rest/a2a.rs`
- `crates/fabric/tests/a2a_workflows.rs`
- `ARCHITECTURE.md`
- `NEXUS_INTEGRATION.md`
- `NEXUS_INTEGRATION_AUDIT.md`
- `OPERATIONS.md`
- `SECURITY.md`
- `TESTING.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`

## 7. Interfaces and Contracts

- `BridgeLifecycle::conformance` accepts a tenant, adapter identity, stored
  component reference/digest, stored grant/config, optional kind, and a limit.
- `BridgeConformanceResult` contains only digest, descriptor metadata, fuel,
  checked-kind, schema-field count, listed-record count, changed-record count,
  and a bounded deterministic report.
- `BridgeConformanceService` is a Fabric service trait with availability,
  reason, and tenant-scoped `conform` methods.
- A2A availability for `bridge-conformance` derives from that service. Its
  task artifact contains the sanitized conformance result or `conformance_failed`.
- The workflow requires `hydra.bridges.read`; it never accepts tenant,
  component, digest, grant, config, secret, or cursor authority from input.

## 8. Milestones

### M1 - Contract and plan-state activation

Add SPEC-016, activate EP-036 in the index, make the state validator cover
EP-036, and document the allowed conformance command. Validate preflight and
state with `preflight: ok` and `execplan state: ok`.

### M2 - BridgeHost read-only conformance

Add bounded request/result types and validate descriptor/probe consistency,
schema metadata, one list page, and optional incremental changes without
returning raw records or invoking mutation methods. Add fixture tests for pass,
digest mismatch, malformed data, duplicate identity, and invalid cursor.
Validate with the lifecycle test filter and expect all conformance tests pass.

### M3 - Kernel tenant-scoped runtime service

Resolve adapter records from Store, reuse the persisted grant/config and
BridgeLifecycle host boundary, and expose the conformance service only when
the configured runtime is available. Add tests for tenant isolation, missing
runtime, and no mutation side effects. Validate Kernel bridge lifecycle tests.

### M4 - Authenticated A2A wiring

Wire the service into AppState, agent-card availability, workflow validation,
durable task execution, failure mapping, and fake-service A2A tests. Validate
conformance discovery, successful metadata-only artifact, retry idempotency,
cross-tenant denial, and provider/adapter failure redaction.

### M5 - Documentation and full verification

Update operational/security/architecture/Nexus/readiness docs and the Decision
Log. Run format, focused tests, SQLx/typecheck, preflight, security,
dependency, state, and full verification. Leave EP-036 active if any required
gate fails; do not treat unavailable Docker or staging evidence as passed.

## 9. Concrete Steps

1. Implement host validation with explicit bounds and sanitized error mapping.
2. Ensure the host creates no Store writes and does not call adapter mutation
   exports during conformance.
3. Add the Kernel service around the existing tenant-scoped adapter registry.
4. Extend the A2A service trait and workflow dispatcher without accepting new
   authority fields.
5. Add focused tests before updating the plan progress checkboxes.
6. Update docs and append the EP-036 transition only after M5 passes.

## 10. Validation and Acceptance

Required outputs:

- `bash scripts/check-execplan-state.sh` -> `execplan state: ok`
- `bash scripts/preflight.sh` -> `preflight: ok`
- BridgeHost lifecycle conformance tests pass.
- Kernel bridge runtime conformance tests pass.
- Fabric A2A workflow tests pass.
- `cargo fmt --all -- --check` exits 0.
- `bash scripts/security-check.sh` -> `security check: ok`.
- `bash scripts/dependency-audit.sh` -> `dependency audit: ok`.
- `bash scripts/verify.sh` -> `verify: ok`.
- No raw record values, secrets, or mutation calls appear in conformance
  artifacts or tests.
- No production deployment, push, tag, or non-test database operation occurs.

## 11. Idempotence and Recovery

Conformance reads the immutable adapter registration and does not persist a
run. Repeating a request repeats only bounded adapter reads. A2A task
idempotency remains Store-owned; equivalent message IDs return the existing
task and conflicting reuse fails. If a host call fails, the task becomes
failed with a fixed redacted error and no retry silently activates or mutates
the adapter.

## 12. Progress

- [x] M1 - Contract and plan-state activation
- [x] M2 - BridgeHost read-only conformance
- [x] M3 - Kernel tenant-scoped runtime service
- [x] M4 - Authenticated A2A wiring
- [x] M5 - Documentation and full verification

## 13. Surprises & Discoveries

Record exact findings here as milestones execute. Do not convert an unavailable
runtime, Docker absence, or missing external credential into a passing result.

- 2026-08-12 - The fixture adapter declares kind `party`, not the example
  `Contact` in the initial input illustration; tests use the declared fixture
  kind and the runtime still accepts any declared adapter kind.
- 2026-08-12 - The first implementation moved checked-kind ownership and the
  incremental flag into owned locals before the host call; this avoided holding
  borrowed request data across the async boundary and kept the result free of
  caller-owned references.
- 2026-08-12 - Focused validation initially lacked a disposable
  `DATABASE_URL`; rerunning against loopback PostgreSQL was required evidence,
  not a reason to weaken the Store-backed boundary.

## 14. Decision Log

- 2026-08-12 - Selected read-only conformance as the next seam because the
  existing A2A catalog advertises it but the typed runtime is absent. Reuse
  existing BridgeHost calls and Store adapter records instead of creating a
  second bridge abstraction or a new persistence table.
- 2026-08-12 - Conformance will not persist a run or emit an event because it
  performs no canonical CRM mutation; durable task state remains the A2A
  workflow audit boundary.
- 2026-08-12 - The deterministic report explicitly says
  `changes-since:not-declared` when an adapter does not advertise incremental
  sync; a report must not claim a read that did not execute.
- 2026-08-12 - Added direct validator tests for malformed JSON, duplicate
  identity, and control-character cursors so the bounded page contract is
  tested independently of the currently empty fixture dataset.
- 2026-08-12 - The first full verifier invocation reached the event-bridge
  integration suite but used its default `127.0.0.1:4222` while the disposable
  NATS relay listened on `[::1]:4222`; the exact suite passed after setting
  `NATS_URL=nats://[::1]:4222`. This is documented environment evidence, not a
  test suppression or product behavior change.

## 15. Outcomes & Retrospective

EP-036 completed 2026-08-12. Focused BridgeHost validator and fixture tests,
Kernel tenant-isolation/no-mutation runtime coverage, authenticated Fabric
A2A success/failure/idempotency tests, formatting, preflight, state,
security/dependency audits, and the full `bash scripts/verify.sh` passed. The
full verifier emitted `preflight: ok`, `unit tests: ok`, `integration tests:
ok`, `failure suites: ok`, `e2e tests: ok`, `security check: ok`, `dependency
audit: ok`, and `verify: ok` after the disposable NATS IPv6 endpoint was
explicitly supplied.

The implementation returns only digest, descriptor metadata, counts, fuel,
and a bounded report. It does not call adapter mutation exports, write Store,
CDM, audit, event, or outbox state, or claim provider reliability beyond the
bounded read contract. Runtime absence and cross-tenant/inactive/digest-invalid
lookups fail closed. EP-010 remains PARTIAL; staging, recovery, soak, security,
performance, accessibility, observability, and human-sign-off evidence remain
open. No production deployment, push, tag, or non-test database operation
occurred.
