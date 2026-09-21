# EP-019 Governed Bridge Activation and Lifecycle

Plan status: COMPLETE

## 1. Purpose / Big Picture
Connect Hydra's existing Wasmtime/WIT BridgeHost to the governed envelope
path for a bounded, production-shaped adapter lifecycle. A registered,
prebuilt adapter component must be tenant-scoped, digest-checked, grant
constrained, probed through the host, and durably represented before Hydra
reports it active. Pause and resume must use governed, typed handlers rather
than direct adapter KV mutation.

This plan closes the code-owned gap where BridgeHost is constructed but no
governed lifecycle handler is registered. It does not claim that bridge
discovery, synthesis, wiring generation, synchronization, canary promotion,
or production deployment are complete.

## 2. Scope
Add an additive tenant-scoped adapter registry and lifecycle state model;
implement safe component resolution under `HYDRA_ADAPTERS_PATH`; instantiate
and probe a prebuilt `.wasm` component through Wasmtime/WIT with named grants,
fuel, adapter KV, and the configured secret source; register typed execution
handlers for deploy, pause, and resume; expose truthful status and capability
availability; and add failure, tenant-isolation, digest, and restart tests.

## 3. Non-goals
No Node/npm tooling, new adapter ABI, raw vendor API, arbitrary filesystem
paths, caller-supplied credentials, direct Store mutation from Fabric, full
sync loop, wiring synthesis, canary/promotion workflow, bridge-generated
code, production deployment, or real production database operation. The
BridgeEngineer synthesis placeholder remains unavailable. No hard-delete or
CRM-record purge behavior is introduced.

## 4. Context and Orientation
The canonical ABI is `wit/hydra-bridge.wit`; `BridgeHost` already enforces
Wasmtime component isolation, grants, fuel, named secrets, adapter KV, and
egress. `POST /v1/bridges` currently creates a `bridges/deploy_adapter`
ActionEnvelope whose random target cannot be executed by the registry, and
pause/resume currently write adapter KV directly after a role check.
The existing `adapters/memcrm.wasm` is the deterministic local fixture. The
new registry is integration metadata, not a second CRM source of truth.

## 5. Files to Read First
`AGENTS.md`; `COMMANDS.md`; `.agent/PLANS.md`;
`.agent/EXECUTION_RULES.md`; `.agent/state/execplan-index.md`;
`ARCHITECTURE.md`; `SECURITY.md`; `TESTING.md`; `ENVIRONMENT.md`;
`DEPLOYMENT.md`; `PRODUCTION_READINESS.md`; `wit/hydra-bridge.wit`;
`crates/bridge-host/src/{grants.rs,host.rs,loader.rs}`;
`crates/bridge-host/tests/conformance.rs`; `crates/fabric/src/services.rs`;
`crates/fabric/src/rest/bridges.rs`; `crates/kernel/src/{execution_registry.rs,executor.rs,runtime_services.rs,config.rs,main.rs}`;
`crates/store/src/lib.rs`; `migrations/0004_adapter_kv_and_autonomy.sql`;
`migrations/0006_secret_grant.sql`; `adapters/memcrm.wasm`;
`.agent/specs/SPEC-002-data-model.md`; `.agent/specs/SPEC-003-api-contracts.md`;
`.agent/specs/SPEC-006-error-handling.md`; `.agent/specs/SPEC-010-nexus-interoperability.md`.

## 6. Files to Change
These are the expected changed files for EP-019:

`.agent/specs/SPEC-012-bridge-lifecycle.md`;
`.agent/execplans/EP-019-governed-bridge-activation-and-lifecycle.md`;
`.agent/state/execplan-index.md`; `scripts/check-execplan-state.sh`;
`DECISIONS.md`; `ARCHITECTURE.md`; `SECURITY.md`; `ENVIRONMENT.md`;
`DEPLOYMENT.md`; `PRODUCTION_READINESS.md`; `NEXUS_INTEGRATION.md`;
`NEXUS_INTEGRATION_AUDIT.md`; `TESTING.md`; `COMMANDS.md`; `.env.example`;
`docker/nexus.env.example`; `migrations/0015_bridge_adapters.sql`;
`.sqlx/` checked query metadata; `crates/store/src/bridge_adapters.rs`;
`crates/store/src/lib.rs`; `crates/store/tests/bridge_adapters.rs`;
`crates/bridge-host/src/lifecycle.rs`; `crates/bridge-host/src/lib.rs`;
`crates/bridge-host/Cargo.toml`;
`crates/bridge-host/tests/lifecycle.rs`; `crates/fabric/src/services.rs`;
`crates/fabric/src/rest/bridges.rs`; `crates/fabric/src/capabilities.rs`;
`crates/fabric/src/mcp.rs`; `crates/fabric/tests/bridge_lifecycle.rs`;
`crates/fabric/tests/integration_contracts.rs`; `crates/fabric/tests/mcp_contract.rs`;
`crates/kernel/src/bridge_runtime.rs`;
`crates/kernel/src/execution_registry.rs`; `crates/kernel/src/executor.rs`;
`crates/kernel/src/runtime_services.rs`; `crates/kernel/src/config.rs`;
`crates/kernel/src/main.rs`; `crates/kernel/tests/bridge_lifecycle.rs`;
`crates/kernel/tests/runtime_wiring.rs`; `crates/kernel/tests/e2e_nexus.rs`;
`crates/kernel/tests/support/fake_nexus.rs`; `NEXUS_PACKAGE_CONTRACT.md`.

No new dependency is expected. If implementation proves one necessary, stop
before adding it, record the license/audit decision, and update this list.

## 7. Interfaces and Contracts
The additive store record must include tenant ID, adapter ID, component
reference, SHA-256 digest, grant projection without secret values, descriptor
projection, lifecycle state, last error, revision, and timestamps. Unique
tenant/adapter identity and optimistic revision checks are required.

`HYDRA_ADAPTERS_PATH` is the only component root. `wiring_ref` resolves to a
normalized relative component name beneath that root; absolute paths,
traversal, symlinks, and unapproved extensions fail closed. The handler
records the exact digest and refuses a changed component on restart.

Canonical governed actions are `bridges/deploy_adapter`,
`bridges/pause_adapter`, and `bridges/resume_adapter`. External callers may
request deploy, but only the Governor and typed handler may execute it.
Pause/resume transitions are tenant-scoped, append history, and return a
receipt/status projection. Raw secret values never enter envelopes, the
registry, events, logs, or responses.

The runtime reports deploy/pause/resume available only when the lifecycle
service is fully configured. Missing adapter root, vault, component, grant,
probe, or persistence support reports unavailable and never pretends that a
bridge is active.

## 8. Milestones
M1. Add the SPEC-012 contract, additive `bridge_adapter` migration, typed
Store repository, checked SQLx metadata, and fixture tests. Read the store
and migration files above; change only the M1 files from Section 6. Validate:
`DATABASE_URL=postgres://hydra:hydra@127.0.0.1:55432/hydra cargo test -p store bridge_adapter_registry -- --nocapture` and `cargo sqlx prepare --workspace -- --all-targets`.
Expected: the focused Store tests pass and SQLx preparation exits 0. Recovery:
rollback only the un-applied additive migration in the test database and
rerun the focused tests; never touch a non-loopback database.

M2. Implement BridgeHost lifecycle assembly, component-root/digest checks,
descriptor/probe handling, and deterministic lifecycle tests with
`adapters/memcrm.wasm`. Validate:
`cargo test -p bridge-host lifecycle -- --nocapture`.
Expected: 2 lifecycle tests pass; valid activation passes, traversal/symlink/digest/grant/fuel and
probe failures fail closed, and no secret value is emitted. Recovery:
leave lifecycle availability disabled and narrow the loader contract; do not
weaken grants or accept arbitrary paths.

M3. Add typed deploy/pause/resume execution handlers, persistent state
transitions, executor/runtime construction, and capability inventory. Validate:
`DATABASE_URL=postgres://hydra:hydra@127.0.0.1:55432/hydra cargo test -p hydra-kernel bridge_lifecycle -- --nocapture`.
Expected: approved deploy probes the fixture, persists the digest and
descriptor, pause/resume are governed, duplicate or cross-tenant transitions
fail, and the real RuntimeServices builder advertises only configured
handlers. Recovery: retain `Unavailable` capability state and preserve the
envelope/audit evidence rather than adding a direct mutation path.

M4. Reconcile Fabric bridge registration/status/pause/resume with the typed
handlers and add authenticated tenant-isolation/receipt contract tests.
Validate: `DATABASE_URL=postgres://hydra:hydra@127.0.0.1:55432/hydra cargo test -p fabric bridge_lifecycle -- --nocapture`.
Expected: external requests create envelopes, status comes from the durable
registry, caller headers cannot select a tenant, and unsupported sync or
synthesis remains unavailable. Recovery: preserve the prior safe status
projection and disable only the new lifecycle capability.

M5. Run the complete local acceptance sequence:
`bash scripts/preflight.sh`; `bash scripts/security-check.sh`;
`bash scripts/dependency-audit.sh`; `bash scripts/verify.sh`;
`bash scripts/check-execplan-state.sh` with explicit loopback Postgres/NATS
test services where required. Expected: each required `: ok` marker,
terminal `verify: ok`, and `execplan state: ok`. Recovery: stop at the first
failure, preserve exact output, fix the smallest root cause, and never mark
staging or production readiness from local evidence.

## 9. Concrete Steps
Execute M1 through M5 strictly in order. Keep SQL in Store repositories;
keep adapter calls behind BridgeHost; keep action decisions in Governor;
keep model/agent principals unable to approve or execute directly. Use the
fixture component for all local tests and unique tenant/adapter IDs. Update
documentation and the status index after each validated milestone.

## 10. Validation and Acceptance
EP-019 is complete only when all milestones pass, the registry is additive and
tenant-scoped, component loading is root/digest/grant/fuel constrained,
deploy/pause/resume are governed and durable, runtime capability truth is
correct, unsupported sync/synthesis remains unavailable, SQLx metadata is
current, `bash scripts/verify.sh` emits `verify: ok`, and the diff is limited
to Section 6. EP-010 remains partial because staging drills, recovery proof,
human review, and release ownership are outside this local plan.

## 11. Idempotence and Recovery
Repeated deploy requests with the same tenant/adapter/digest return the
existing durable record or envelope; a different digest conflicts. State
transitions use revision checks and append history. A failed probe leaves the
record inactive and does not expose the component. Interrupted milestones
resume by rerunning preflight and the milestone validation; migrations are
additive and SQLx metadata is regenerated rather than hand-edited.

## 12. Progress
- [x] M1 - Store contract and additive persistence (`cargo test -p store bridge_adapter_registry -- --nocapture` -> 2 passed; loopback migration 15 applied; `cargo sqlx prepare --workspace -- --all-targets` exited 0; 2026-08-11)
- [x] M2 - BridgeHost lifecycle probe and artifact trust boundary (`cargo test -p bridge-host lifecycle -- --nocapture` -> 2 passed; 2026-08-11)
- [x] M3 - Governed handlers and kernel runtime wiring (`cargo test -p hydra-kernel --test bridge_lifecycle -- --nocapture` -> 1 passed; 2026-08-11)
- [x] M4 - Fabric lifecycle facade and tenant-safe contract tests (`cargo test -p fabric --test bridge_lifecycle -- --nocapture` -> 1 passed; MCP contract -> 4 passed; legacy integration contract -> 1 passed; 2026-08-11)
- [x] M5 - Full local gates and truthful reconciliation (`cargo sqlx migrate run` -> 15 migrations applied to an ephemeral test database; `preflight: ok`; `security check: ok`; `dependency audit: ok`; fake Nexus E2E -> 3 passed; `verify: ok`; 2026-08-11)

## 13. Surprises & Discoveries
The existing bridge registration envelope uses a random target and therefore
cannot pass the current execution registry's target validation. Pause/resume
also writes adapter KV directly. The existing BridgeHost and fixture adapter
are usable, so the smallest safe path is a durable adapter registry plus
typed lifecycle handlers rather than a second CRM abstraction.
The full gate also exposed a test-harness boundary: the fake Nexus auth test
was not classified as E2E and the loopback HTTP servers needed readiness
coordination. The test is now prefixed `e2e_` and both listeners wait for
acceptance. The Docker endpoint was unavailable, so M5 used an isolated
native PostgreSQL cluster and a temporary WSL NATS JetStream process instead
of weakening the real event contract.

## 14. Decision Log
| Date | Decision | Rationale |
|---|---|---|
| 2026-08-11 | Start EP-019 after EP-016 completion | BridgeHost construction without a governed lifecycle handler is the highest-risk code-owned gap; staging-only EP-010 evidence remains unavailable locally |
| 2026-08-11 | Limit the first lifecycle seam to prebuilt component activation, pause, and resume | Discovery, synthesis, wiring, sync, canary, and promotion need additional contracts and must remain unavailable rather than being simulated |
| 2026-08-11 | Apply migration 0015 only to the loopback test database and use the test-symbol filter `bridge_adapter_registry` | The first query compile exposed an unapplied test migration, and the initial plural filter matched zero tests; explicit setup and a two-test filter prevent false-green M1 evidence |
| 2026-08-11 | Require the M2 `lifecycle` filter to select both tests | The first filter selected only the probe test; renaming the artifact test makes traversal/digest coverage part of the required milestone signal |
| 2026-08-11 | Keep Fabric pause/resume proposal-only until a durable active registry record exists | The legacy REST contract attempted direct KV state changes on a merely proposed registration; fail-closed NotFound behavior prevents status fabrication and the M4 fixture covers the governed durable path |
| 2026-08-11 | Project tenant bridge health from the Store registry into compact context | Capability availability alone is not a tenant status projection; the bounded status list reuses the tenant-scoped registry and existing envelope overlay without adding a CRM abstraction |
| 2026-08-11 | Classify the fake Nexus authentication check as E2E and wait for loopback listeners before issuing requests | The full integration gate exposed a connection-refused startup race, and the non-prefixed test bypassed the dedicated E2E gate; deterministic readiness and the `e2e_` prefix keep required suites truthful |
| 2026-08-11 | Validate M5 with isolated local services when Docker was unavailable | A temporary PostgreSQL 16 cluster on loopback port 55433 and an extracted NATS 2.10.7 JetStream server in WSL provided real database/broker behavior without touching the native database, changing production Compose, or adding repository dependencies |

## 15. Outcomes & Retrospective
Not complete. This section will contain only observed milestone outputs,
remaining unavailable capabilities, and production-readiness boundaries after
M1 through M5 are validated. M1 is accepted: the tenant-scoped registry,
append-only transition history, revision conflict behavior, migration, and
checked SQLx metadata passed against loopback-only Postgres.
M2 is accepted: the configured component root rejects unsafe references and
digest changes, and the real fixture component probes through Wasmtime with
grant validation.
M3 is accepted: configured runtime construction registers only the typed
bridge lifecycle handlers, approved deploy probes and persists the descriptor,
pause/resume transitions are durable and governed, and equivalent redeploy is
idempotent.
M4 is accepted: Fabric status and compact context use tenant-scoped durable
registry projections, external mutation requests remain envelope proposals,
legacy pre-activation pause/resume fails closed, and MCP/REST compatibility
contracts remain green. M5 is accepted: the isolated test database received
all 15 additive migrations, the fake Nexus authentication/cross-business/
round-trip scenarios passed 3/3 against real JetStream, the full
`verify.sh` sequence emitted `verify: ok`, and the plan-state check remains
machine-valid. Security checking remains green with three explicitly allowed
RustSec warnings from existing transitive dependencies; no new dependency was
added. EP-019 is complete. EP-010 remains partial because staging drills,
restore/rollback proof, performance/accessibility evidence, and human release
sign-off are still outside this local validation.

## Post-EP-033 Current Verification (2026-08-12)

EP-019's historical activation boundary remains correct: prebuilt Wasmtime
deployment, pause, and resume are governed only when configured. EP-033 now
implements a separate Experimental mapping-proposal seam through TOKENKILLER
and authenticated A2A tasks. It does not generate or execute Wasm, activate a
bridge, synchronize records, run conformance, canary, or promote an adapter.

## Post-EP-034 Current Verification (2026-08-12)

The lifecycle boundary now uses tenant-scoped adapter scratch state through
the additive `tenant_adapter_kv` Store table. The governed envelope tenant is
passed into BridgeHost probes, equal adapter IDs remain isolated across
tenants, and the historical unscoped `adapter_kv` Store API fails closed.
Historical rows are not reassigned because they lack tenant authority.

## Post-EP-035 Current Verification (2026-08-12)

EP-019 remains the activation boundary for prebuilt Wasmtime deploy, pause,
and resume. EP-035 adds a separate governed manual incremental-sync seam over
the existing WIT ABI, with Store-owned cursor/conflict state and no change to
EP-019's activation, grant, or tenant-isolation rules. Synchronization is
locally verified when an active configured adapter advertises `changes-since`;
scheduler, full-relist, synthesis, conformance, canary, promotion, staging,
and production evidence remain outside EP-019.

## Post-EP-051 Current Verification (2026-08-12)

EP-051 strengthens the prebuilt activation boundary without changing EP-019's
Wasmtime or Governor scope. After probe, `deploy_adapter` now runs the existing
bounded read-only BridgeHost conformance contract with the persisted tenant
grant/config and digest before entering `active`; a failure persists a
revision-checked `failed` state and a failed execution receipt. The Kernel
bridge lifecycle suite passes valid activation, conformance failure, and
invalid-runtime fail-closed coverage. Generated code, autonomous canary,
promotion, staging, and EP-010 readiness remain outside this boundary.
