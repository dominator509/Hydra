# EP-013 Governed Execution and Provenance

Plan status: COMPLETE

## 1. Purpose / Big Picture
Make every Nexus/agent mutation governable, attributable, idempotent, executable through typed handlers, verified, and actually wired in the running kernel while preserving Hydra's deterministic Governor and store boundary.

## 2. Scope
Backward-compatible invocation context; idempotency and immutable approval persistence; tenant-safe/concurrency-safe envelope transitions; tenant-aware policy provider; typed execution registry; governed external mutation path; real kernel construction of executor/bridge/TOKENKILLER/router/available agents; truthful agent/bridge capabilities; receipt and transition event preparation.

## 3. Non-goals
No canonical JetStream event envelope/ack relay (EP-014), fake full Nexus E2E/deployment topology (EP-015), arbitrary provider or SQL execution, model approval, GraphQL, A2A, skill federation, production deployment, or completion claims for bridge synthesis/transport that remain unavailable.

## 4. Context and Orientation
The current executor supports one `pipeline/move_stage/deal` path and is not compiled into the binary. `StoreEnvelopeService` owns a static demo Governor, envelope lookup is not tenant-scoped, approvals lack immutable human assertions, transitions do not emit outbox events, and external entity CRUD bypasses envelopes. BridgeEngineer, DataSteward merge, and Comms overstate useful runtime capability. SPEC-010 sections 7 and 11 through 13 control this plan.

## 5. Files to Read First
`.agent/specs/SPEC-010-nexus-interoperability.md`; `.agent/specs/SPEC-001-core-domain-and-governor.md`; `crates/governor/src/{envelope.rs,governor.rs,state.rs}`; `crates/store/src/{envelopes.rs,autonomy.rs,entities.rs,event_log.rs,outbox.rs,lib.rs}`; `crates/fabric/src/{services.rs,capabilities.rs,mcp.rs}`; `crates/kernel/src/{main.rs,executor.rs,config.rs}`; `crates/bridge-host/src/lib.rs`; `crates/agents/src/{bridge_engineer.rs,data_steward.rs,comms.rs}`; `crates/tokenkiller/src/session.rs`; `crates/llm-router/src/lib.rs`; migrations through `0008`.

## 6. Files to Change
`DECISIONS.md`; `ARCHITECTURE.md`; `COMMANDS.md` only if a new executable command is required; `NEXUS_INTEGRATION.md`; `crates/governor/src/envelope.rs`; `crates/governor/src/lib.rs`; `crates/governor/tests/state_machine.rs`; `crates/store/src/lib.rs`; `crates/store/src/envelopes.rs`; `crates/store/src/autonomy.rs`; `crates/store/src/idempotency.rs` (new); `crates/store/src/approvals.rs` (new); `migrations/0009_execution_provenance.sql` (new); `.sqlx/` metadata for changed queries; `crates/fabric/src/capabilities.rs`; `crates/fabric/src/services.rs`; `crates/fabric/src/mcp.rs`; `crates/fabric/src/rest/nexus.rs`; `crates/kernel/Cargo.toml`; `crates/kernel/src/main.rs`; `crates/kernel/src/executor.rs`; `crates/kernel/src/execution_registry.rs` (new); `crates/kernel/src/policy_provider.rs` (new); `crates/kernel/src/runtime_services.rs` (new); `crates/agents/Cargo.toml`; `crates/agents/src/data_steward.rs`; `crates/agents/src/bridge_engineer.rs`; `crates/agents/src/comms.rs`; `crates/store/tests/integration_execution_provenance.rs` (new); `crates/fabric/tests/governed_nexus_mutations.rs` (new); `crates/kernel/tests/integration_executor.rs`; `crates/kernel/tests/runtime_wiring.rs` (new); this plan and `.agent/state/execplan-index.md`.

## 7. Interfaces and Contracts
`InvocationContext` uses serde defaults and excludes secrets/PII. Idempotency key scope is tenant+origin+key+capability/action with canonical request hash. Approval assertion is immutable and human-delegated. Handler registry rejects duplicates and binds capability/domain/action/kind/schema/target/risk/reversal/execute/verify/compensate. External mutation always returns an envelope/receipt. Tenant-scoped transitions append history and audit/outbox atomically. Policy reads current persisted tenant cells through safe caching/invalidation.

## 8. Milestones
M1 Domain provenance and persistence. Add invocation defaults, migration, idempotency/approval repos, tenant-safe envelope reads and optimistic transitions. Validation: `cargo test -p store integration_execution_provenance -- --nocapture`. Expected: equivalent retry, conflict, tenant isolation, immutable approval, and transition concurrency tests pass. Recovery: separate pure serialization/hash tests from transactional races; fix constraints forward only.

M2 Typed registry and governed Fabric mutation. Build registry, connect capability availability, route MCP/REST proposals through it and Governor, deny direct external CRUD/approval fabrication. Validation: `cargo test -p fabric governed_nexus_mutations -- --nocapture`. Expected: unsupported/duplicate handler, direct-store prevention, autonomy and approval tests pass. Recovery: narrow to one capability end-to-end; do not add a generic payload escape hatch.

M3 Kernel runtime wiring and policy refresh. Compile/register executor in binary, construct tenant-aware policy provider, envelope service, handler registry, BridgeHost when configured, TOKENKILLER/router when configured, relay, and truthfully available agents. Validation: `cargo test -p hydra-kernel runtime_wiring -- --nocapture`. Expected: real construction path and persisted policy refresh tests pass. Recovery: feature/config absence must mark a capability unavailable, not silently install a demo provider.

M4 Handler execution, verification, and approvals. Wire stage change plus supported bridge lifecycle handlers; verify target state; require valid approval; append receipt/transition records. Validation: `cargo test -p hydra-kernel integration_executor -- --nocapture`. Expected: queue/high-autonomy execute/approval/tenant-safe/unsupported cases pass. Recovery: handler failure leaves a deterministic failed/queued state and no direct mutation; isolate one handler.

M5 Agent capability truth, metadata, and full gates. DataSteward emits governed proposals, BridgeEngineer/Comms advertise actual status, refresh SQLx metadata/docs and run verification. Validation: `bash scripts/test-integration.sh` then `bash scripts/verify.sh`. Expected: `integration tests: ok`, `failure suites: ok`, and `verify: ok` without masking. Recovery: any false capability is marked unavailable; do not fabricate implementation to satisfy discovery.

## 9. Concrete Steps
Execute M1-M5 in order. Persist provenance before exposing execution. Register handlers before advertising availability. Build production runtime without `demo_governor`; test-only demos remain behind explicit test construction. Update Progress/Decision Log immediately and activate EP-014 only after all gates pass.

## 10. Validation and Acceptance
All user-required EP-013 tests pass: idempotency, conflict, unsupported/duplicate handlers, no direct store, autonomy outcomes, approval/four-eyes, correlation fields, real kernel executor construction, policy refresh, tenant-safe lookup, and truthful bridge/agent capabilities. Full verification is green and no model/agent or Nexus code can directly mutate/approve.

## 11. Idempotence and Recovery
Migration is additive. Registry construction is deterministic. Idempotency returns existing durable references on retry. Optimistic transitions retry only bounded serialization/concurrency errors and never skip states. Runtime config disablement is reversible. If interrupted, inspect durable test fixtures and Progress; do not delete envelopes or audit rows.

## 12. Progress
- [x] M1 - Invocation, idempotency, approvals, tenant-safe transitions (`cargo test -p store integration_execution_provenance -- --nocapture`: 4 passed, 2026-08-11)
- [x] M2 - Typed registry and governed external mutation (`cargo test -p fabric governed_nexus_mutations -- --nocapture`: 2 passed; `cargo test -p hydra-kernel execution_registry -- --nocapture`: 2 passed, 2026-08-11)
- [x] M3 - Real kernel wiring and policy refresh (`cargo test -p hydra-kernel runtime_wiring -- --nocapture`: 3 passed; `cargo check -p hydra-kernel --all-targets`: 0 errors, 2026-08-11)
- [x] M4 - Typed handlers, verification, and approval enforcement (`cargo test -p hydra-kernel integration_executor -- --nocapture`: 4 passed; focused Store/Fabric/runtime regressions: 9 passed, 2026-08-11)
- [x] M5 - Agent truth, metadata, integration, and verify (`cargo test -p agents -- --nocapture`: 21 passed; `cargo sqlx prepare --check --workspace -- --all-targets`: exit 0; `bash scripts/test-integration.sh`: `integration tests: ok`, `failure suites: ok`; `bash scripts/verify.sh`: `verify: ok`, 2026-08-11)

## 13. Surprises & Discoveries
- 2026-08-11: Activated only after EP-012's dependency, security, focused MCP, database-backed Fabric, SQLx metadata, and full verification gates passed.
- 2026-08-11: The vendored SQLx build does not export `query_scalar!`; the approval repository uses the existing supported `query_as!` pattern without changing SQL semantics.
- 2026-08-11: Moving envelope transitions into async persistence required strengthening the pure `Clock` trait with `Send + Sync`; all existing stateless clocks already satisfy the contract.
- 2026-08-11: Linking the real Wasmtime BridgeHost into `hydra-kernel` exceeded the first 120-second test-build limit; the bounded narrower `cargo check -p hydra-kernel --all-targets` completed in 192.6 seconds with zero errors, and the warmed exact M3 test then completed in 16.3 seconds.
- 2026-08-11: The existing non-test concierge implementation used a deterministic fake `PingRouter`. Kernel wiring now selects a configured real `llm-router` plus TOKENKILLER session, or reports the capability disabled when no provider is configured; fake providers remain test-only.
- 2026-08-11: BridgeHost construction and bridge lifecycle execution are separate capabilities. The Wasmtime/WIT host constructs successfully, but no durable adapter lifecycle handler exists, so deployment/pause/resume execution remains explicitly unavailable rather than being registered as a working command.
- 2026-08-11: The integration failure-suite command passed three positional libtest filters and suppressed the resulting command failure with `|| true`. Each intended test passes independently; the gate now invokes each exact test without suppression.
- 2026-08-11: The MCP schema snapshot changed after EP-013 added the typed governed proposal output and execution-handler availability link. The canonical nine-name surface and no-approval rule are unchanged; the schema-validating test produced and now pins digest `c091f87ac742c8ca2b602cefe4d81541d9bba95eb6c2a6016d2082fbb522a3dc`.

## 14. Decision Log
| Date | Decision | Rationale |
|---|---|---|
| 2026-08-10 | Queue behind EP-012 | Execution must consume the authenticated principal/binding/capability contract |
| 2026-08-11 | Activate after EP-012 completion | The authenticated principal, binding-derived tenant, scope authorization, capability registry, and MCP/REST boundary are now executable and fully verified |
| 2026-08-11 | Persist invocation context in envelope documents and each transition, with immutable tenant-scoped idempotency and approval assertions | Old documents deserialize through serde defaults; durable retry, four-eyes, and provenance claims are enforced by additive schema constraints and repositories |
| 2026-08-11 | Treat `crates/governor/tests/core_domain.rs`, `crates/store/tests/integration_envelopes.rs`, and `crates/kernel/tests/integration_executor.rs` as required existing-constructor fixtures | Adding `InvocationContext` is backward-compatible for stored JSON but Rust struct literals must be updated to compile; this is narrower than creating parallel test helpers |
| 2026-08-11 | Keep the capability registry in Fabric and the typed execution-handler registry in kernel | Fabric owns protocol/schema discovery; kernel owns runtime orchestration. Runtime capability keys connect them without reversing the six-layer dependency direction |
| 2026-08-11 | Expose only `POST /v1/nexus/proposals/stage-change` for the initial REST command | The route has the same fixed schema and dispatcher as `hydra.crm.propose_action`; it cannot construct arbitrary envelopes or call entity mutation services |
| 2026-08-11 | Make proposal envelope, idempotency record, and initial Governor transition one transaction | Concurrent equivalent retries resolve to one envelope, conflicting hashes return a stable conflict, and no orphan duplicate action survives |
| 2026-08-11 | Add migration `0010_autonomy_policy_revision.sql` and key the Governor cache by exact per-tenant revision | Mutable autonomy cells must affect later decisions without rebuilding one immutable process-wide Governor; the additive trigger provides deterministic invalidation without time-based stale windows |
| 2026-08-11 | Construct BridgeHost at kernel boot, register the real executor worker, and expose explicit runtime component availability | Construction failure now fails boot; unimplemented BridgeEngineer synthesis and Comms transport remain unavailable instead of being advertised as executable |
| 2026-08-11 | Require `OPENAI_COMPAT_MODEL` whenever `OPENAI_COMPAT_BASE_URL` is configured | The LLM router must not invent a provider model identifier; startup validation fails closed before any provider request |
| 2026-08-11 | Add migration `0011_execution_receipts.sql` and commit the final envelope transition, immutable receipt, audit event, and outbox record in one store transaction | A verified or failed handler result must be durable and tenant-scoped; one receipt per envelope prevents duplicate logical execution evidence |
| 2026-08-11 | Expose external approval only as typed REST `POST /v1/nexus/envelopes/{id}/approval`; expose no MCP approval tool | A human-delegated principal with the approval scope and accepted `acr` may approve or reject; model and agent principals retain no approval surface |
| 2026-08-11 | Re-evaluate current persisted policy and Constitution before issuing a sealed post-human-approval token | Human approval satisfies the second gate for L2/L3 but cannot override a newly manual-only policy or a constitutional block |
| 2026-08-11 | Include additive migrations `0010` and `0011`, execution receipts, agent descriptors, provider configuration docs, refreshed `.sqlx` metadata, and the integration gate in M3-M5 scope | The explicit plan outcomes require mutable persisted policy, durable verified receipts, truthful agent/provider availability, checked queries, and an unmasked acceptance command; omitting these files would make the plan's own validation claims false |
| 2026-08-11 | Refresh the MCP 2025-11-25 contract digest only after validating the unchanged tool names and intentional proposal schema/runtime-link delta | Snapshot drift remains a hard failure while EP-013's version-compatible additive capability metadata is accepted explicitly rather than silently |

## 15. Outcomes & Retrospective
Complete. Migration `0009_execution_provenance.sql` adds backward-compatible invocation context, idempotency, immutable approvals, and tenant-safe transition persistence; `0010_autonomy_policy_revision.sql` provides exact persisted-policy cache invalidation; `0011_execution_receipts.sql` records append-only verified/failed execution outcomes. All migrations were applied only to the isolated development database.

The external mutation seam now accepts one typed provider-neutral stage-change capability, creates an attributable idempotent ActionEnvelope, evaluates current tenant policy, queues or dispatches through the typed registry, verifies the resulting CDM state, and atomically records receipt/audit/outbox evidence. A distinct MFA-authenticated human-delegated principal is required for queued approval; no MCP approval tool exists and agents cannot create approval assertions. The production kernel constructs and supervises the executor, persisted Governor provider, Wasmtime BridgeHost, and a configured real TOKENKILLER/LLM route or an explicit disabled state. DataSteward, BridgeEngineer, Comms, and bridge lifecycle availability now match implemented behavior.

Acceptance passed: Store provenance 4 tests, Fabric governed mutation 2 tests, registry 2 tests, runtime wiring 3 tests, executor 4 tests, agents 21 tests, full Fabric 129 tests, SQLx metadata check, fail-fast integration/failure suites, and full `verify.sh` with `verify: ok`. Remaining limitations are explicit: only `pipeline/move_stage/deal` executes, bridge lifecycle and Comms transport remain unavailable, DataSteward merge remains proposal-only, and EP-014 still owns the canonical event/JetStream contract. Security tooling remained green but reported non-failing dependency warnings for unmaintained `fxhash` and unsound `event-listener`; these remain production-readiness risks rather than hidden successes. No provider request, production database access, production deployment, push, merge, or tag occurred.
