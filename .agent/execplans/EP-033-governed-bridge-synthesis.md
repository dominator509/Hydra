# EP-033 Governed Bridge Synthesis

Plan status: COMPLETE

## 1. Purpose / Big Picture

BridgeEngineer currently stops at `SynthesisNotImplemented`, while Hydra
already has a TOKENKILLER `MappingYaml` contract, bridge replay fixtures, a
durable A2A task facade, and a provider-neutral LLM router. This plan connects
those existing seams so Hydra can produce a bounded, reviewable mapping
proposal without ever executing model output or bypassing BridgeHost,
Governor, or Store.

## 2. Scope

- Add SPEC-013 for the synthesis contract.
- Add strict bounded mapping synthesis to `crates/agents` through
  `tokenkiller::Session`.
- Add a Kernel runtime service that owns the configured router and ledger sink.
- Add an authenticated A2A `bridge-synthesis` workflow backed by durable task
  state and idempotency.
- Update runtime capability inventory and docs to report Experimental only when
  a real provider chain is configured.
- Add unit, A2A, runtime, and security tests.

## 3. Non-goals

- No generated or executed Wasm, source-code patch application, or arbitrary
  filesystem access.
- No bridge activation, synchronization, conformance, canary, promotion,
  pause, resume, or CRM mutation.
- No new SQL migration, external dependency, provider SDK, credential path,
  Node/npm toolchain, or production deployment.
- No model approval, ActionEnvelope fabrication, or direct Store mutation.
- No claim that bridge production readiness or EP-010 is complete.

## 4. Context and Orientation

`crates/agents/src/bridge_engineer.rs` defines the seven-step loop but returns
at synthesis. `crates/tokenkiller/src/contracts.rs` and
`tests/fixtures/tk-corpus/bridge_mapping.json` define the existing bounded
mapping contract. `crates/kernel/src/runtime_services.rs` already owns the
real router and Store ledger sink. `crates/fabric/src/rest/a2a.rs` persists
allowlisted workflow tasks but currently enables only deterministic
`migration-assessment`. This plan connects those seams while preserving the
L3 import law and the no-direct-execution invariant.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/state/execplan-index.md`
- `ARCHITECTURE.md`
- `SECURITY.md`
- `TESTING.md`
- `NEXUS_INTEGRATION.md`
- `.agent/specs/SPEC-009-tokenkiller.md`
- `.agent/specs/SPEC-011-nexus-model-a2a-skills.md`
- `.agent/specs/SPEC-012-bridge-lifecycle.md`
- `.agent/specs/SPEC-013-bridge-synthesis.md`
- `crates/agents/src/bridge_engineer.rs`
- `crates/tokenkiller/src/{session.rs,contracts.rs,prefix.rs}`
- `crates/kernel/src/runtime_services.rs`
- `crates/fabric/src/{services.rs,rest/a2a.rs,capabilities.rs}`
- `crates/store/src/a2a_tasks.rs`
- `tests/fixtures/tk-corpus/bridge_mapping.json`

## 6. Files to Change

- `.agent/specs/SPEC-013-bridge-synthesis.md`
- `.agent/execplans/EP-033-governed-bridge-synthesis.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `crates/agents/Cargo.toml`
- `crates/agents/src/bridge_engineer.rs`
- `crates/agents/tests/bridge_engineer_loop.rs`
- `crates/kernel/src/runtime_services.rs`
- `crates/kernel/tests/runtime_wiring.rs`
- `crates/fabric/src/services.rs`
- `crates/fabric/src/rest/a2a.rs`
- `crates/fabric/tests/a2a_workflows.rs`
- `crates/fabric/tests/integration_contracts.rs`
- `ARCHITECTURE.md`
- `SECURITY.md`
- `NEXUS_INTEGRATION.md`
- `NEXUS_INTEGRATION_AUDIT.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`
- `COMMANDS.md`
- `.agent/execplans/EP-016-nexus-model-a2a-and-skills.md`
- `.agent/execplans/EP-019-governed-bridge-activation-and-lifecycle.md`

## 7. Interfaces and Contracts

- `BridgeSynthesisRequest` contains bounded `adapter_id`, `descriptor`, and
  `schema` metadata only.
- `BridgeSynthesisDraft` contains normalized mapping text, validation report,
  repair status, and redacted provider provenance; it contains no token,
  prompt, secret, customer body, Wasm bytes, or executable diff.
- `BridgeEngineer::synthesize(&Session, request)` is async and uses only the
  `bridge_mapping` TOKENKILLER route.
- Mapping adapter identity, entity, fields, size, and safe-name validation
  occurs after TOKENKILLER contract validation and before any artifact is
  returned.
- `BridgeSynthesisService` is an authenticated Fabric service seam. The
  default implementation is unavailable; Kernel supplies the real runtime
  only when a provider chain exists.
- A2A `bridge-synthesis` uses existing task idempotency and tenant scoping,
  and returns a proposal artifact only. It never creates an ActionEnvelope or
  calls the adapter lifecycle.
- Capability status is Experimental with a concrete unavailable reason when
  no provider/route exists.

## 8. Milestones

### M1 - Activate and baseline the synthesis contract

Activate EP-033, add SPEC-013, index/checker coverage, and run
`bash scripts/check-execplan-state.sh` plus `bash scripts/preflight.sh`.

Expected output: `execplan state: ok` and `preflight: ok`.

Recovery: repair only plan/index/checker state before editing runtime files.

### M2 - Implement bounded BridgeEngineer synthesis

Add the existing `tokenkiller` dependency to `agents`, implement bounded
request and normalized mapping types, fixed S0-S2 segments, `bridge_mapping`
Session invocation, strict post-contract validation, and unit/fake-router
tests. Run formatting, agent unit tests, and the focused integration suite.

Expected output: agent tests pass; malformed mapping, identity mismatch,
secret-shaped content, oversized input, and provider failure fail closed.

Recovery: retain the old synchronous placeholder for compatibility and narrow
the new async path; do not relax TOKENKILLER or mapping validation.

### M3 - Wire Kernel runtime and A2A workflow

Add the Kernel runtime service over the existing router/ledger, extend
`AppState` with the default-unavailable synthesis service, and make A2A
`bridge-synthesis` persist and resume through `working -> completed|failed`.
Update capability discovery and runtime tests.

Expected output: configured runtime reports Experimental/available, default
runtime reports disabled, and A2A tests prove discovery, tenant isolation,
idempotent retry, failure state, and no activation call.

Recovery: leave the A2A workflow unavailable and preserve durable task state
if runtime construction is not configured; never instantiate Wasmtime here.

### M4 - Reconcile security, docs, and historical truth

Update architecture, security, Nexus integration, audit, readiness, commands,
and affected historical plans. Run security, dependency, and diff checks.

Expected output: `security check: ok`, `dependency audit: ok`, and no stale
claim that BridgeEngineer synthesis is absent when the optional runtime is
configured.

Recovery: keep capability Experimental and all activation/sync/canary states
unavailable if documentation or tests expose an unsupported claim.

### M5 - Full acceptance and truthful completion

Run mandatory repository gates against isolated loopback services, review the
diff against section 6, record exact evidence, mark EP-033 COMPLETE, and
return to no active plan.

Expected output: required success markers, `verify: ok`, `execplan state: ok`,
and `git diff --check` exit 0.

Recovery: follow AGENTS.md §7; missing live provider credentials do not block
deterministic fake-router validation, and no live provider call is required.

## 9. Concrete Steps

1. Activate the plan and add the normative SPEC-013 contract.
2. Implement pure bounded request/response validation and TOKENKILLER session
   orchestration in `agents`.
3. Build the optional runtime service from the existing configured router and
   Store ledger sink.
4. Add Fabric/A2A task integration with durable idempotency and failure state.
5. Add tests and update capability/readiness/security documentation.
6. Run focused and full gates, then reconcile the state index.

## 10. Validation and Acceptance

- The state checker accepts exactly one EP-033 ACTIVE row during work and no
  ACTIVE row after completion.
- `agents` never imports `llm-router`, Store, Wasmtime, or credentials.
- Every model call is made through TOKENKILLER and records ledger/provenance.
- Invalid or unsafe mapping output cannot become an executable artifact.
- A2A `bridge-synthesis` is authenticated, tenant-scoped, idempotent, bounded,
  correlation-preserving, and proposal-only.
- Retry returns the same task; conflicting message reuse fails.
- Agent/model output cannot approve, activate, or mutate a bridge.
- Standalone runtime remains unchanged when no provider is configured.
- Full local gates pass; EP-010 remains partial for staging and production
  bridge evidence.

## 11. Idempotence and Recovery

A2A task idempotency is delegated to the existing tenant/message/request-hash
Store contract. Mapping normalization is deterministic and repeated provider
responses produce byte-stable proposal artifacts. If synthesis fails after the
task reaches `working`, the task transitions to `failed` with a bounded,
redacted error artifact and is never treated as an activated bridge. A
restart can inspect the durable task but does not replay model work
automatically or execute an adapter.

## 12. Progress

- [x] M1 - Activate and baseline the synthesis contract (`execplan state: ok`; `preflight: ok`)
- [x] M2 - Implement bounded BridgeEngineer synthesis (agent target passed 28 tests)
- [x] M3 - Wire Kernel runtime and A2A workflow (runtime wiring passed 7 tests; A2A target passed 4 tests against disposable Postgres)
- [x] M4 - Reconcile security, docs, and historical truth (ADR-0043, SPEC-013, security/docs/commands updated)
- [x] M5 - Full acceptance and truthful completion (`verify: ok`; `execplan state: ok`; `git diff --check`)

## 13. Surprises & Discoveries

- The repository already contains deterministic `bridge_mapping` replay
  fixtures and a MappingYaml contract, so synthesis can be implemented as a
  proposal seam without inventing a provider API or Wasm generator.
- The existing A2A task table supports `failed` states and CAS transitions,
  which is sufficient for durable proposal failure without a migration.

## 14. Decision Log

| Date | Context | Decision | Why |
|---|---|---|---|
| 2026-08-12 | BridgeEngineer stops at synthesis despite an existing TK mapping contract | Implement mapping proposal synthesis, not code generation or activation | It closes the real model seam while preserving Wasmtime and Governor boundaries |
| 2026-08-12 | Agents must not import `llm-router` directly | Pass a TOKENKILLER Session from Kernel-owned runtime construction | Preserves the explicit architecture call path and keeps provider credentials out of agents |

## 15. Outcomes & Retrospective

M1-M4 are complete. The async BridgeEngineer path validates bounded metadata,
uses only the TOKENKILLER `bridge_mapping` route, records ledger/provenance,
normalizes safe MappingYaml output, and never returns executable artifacts.
Kernel reports Experimental only with a configured provider chain; standalone
mode reports synthesis disabled. Fabric/A2A persists proposal completion or a
redacted failed task with existing tenant/idempotency/correlation guarantees.

Focused evidence: `cargo test -p agents --offline` passed 28 tests;
`cargo test -p hydra-kernel --test runtime_wiring --offline` passed 7 tests
against a disposable loopback Postgres cluster; and
`cargo test -p fabric --test a2a_workflows --offline` passed 4 tests against
the same class of disposable cluster. `bash scripts/security-check.sh` and
`bash scripts/dependency-audit.sh` both passed. The first full verifier
attempt correctly failed because the disposable database had not yet been
migrated; after applying all additive migrations, the next attempt exposed
and fixed a denied `clippy::type-complexity` return shape in
`runtime_services.rs`. A later full attempt reached all gates but used the
default IPv4 NATS address while the available JetStream listener was bound to
IPv6 loopback; the focused event-bridge and smoke targets passed when pointed
at `nats://[::1]:4222`. The captured final full run against the migrated
loopback Postgres and that local JetStream listener ended with
`smoke test: ok`, `cache-hit audit: ok (ratio=0.9717)`, and `verify: ok`.
`bash scripts/check-execplan-state.sh` returned `execplan state: ok`, and
`git diff --check` returned exit 0. No live provider, staging deployment, or
production database was used; the disposable Postgres cluster was stopped
and removed after validation.
