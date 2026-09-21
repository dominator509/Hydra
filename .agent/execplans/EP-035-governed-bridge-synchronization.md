# EP-035 - Governed Bridge Synchronization

Plan status: COMPLETE

## 1. Purpose / Big Picture

EP-034 made adapter scratch state tenant-scoped and fail-closed. The next
code-owned gap is that an active bridge can be probed and paused but cannot
apply its existing WIT incremental change feed to Hydra's canonical CDM.
Implement one bounded, manually invoked synchronization page. The external
request becomes a governed envelope, the Kernel invokes a typed BridgeHost
operation, and Store atomically applies canonical changes with durable cursor
and conflict state.

## 2. Scope

- Add SPEC-015 and the additive sync-state schema.
- Add tenant/adapter/kind cursor, run, and conflict repositories.
- Add canonical bridge-origin upsert and soft-delete helpers that preserve
  existing entity, event, audit, and outbox rules.
- Add a BridgeHost lifecycle wrapper for the WIT `changes-since` export.
- Add a typed `bridges/sync_adapter` execution handler and runtime registry
  wiring.
- Add the `hydra.bridges.sync` capability and governed MCP/REST proposal
  surfaces.
- Add focused tests, documentation, and truthful state-index evidence.

## 3. Non-goals

- No full-relist fallback, scheduler, autonomous agent, or provider passthrough.
- No bridge mapping synthesis or generated mapping execution.
- No hard deletes, destructive migrations, raw payload persistence, or direct
  database access from Nexus, Fabric, adapters, or agents.
- No production deployment, staging drill, live provider call, push, merge, or
  tag.

## 4. Context and Orientation

The only bridge ABI is `wit/hydra-bridge.wit`. `BridgeHost::AdapterHandle`
already exposes typed `changes_since`. `BridgeLifecycle::probe` already builds
the trusted Wasmtime `HostState` with tenant-scoped adapter KV. The registry in
`bridge_adapter` is tenant-scoped and persists the adapter descriptor after a
governed activation. `EntitiesRepo` already validates CDM bodies and writes
canonical entity events plus outbox records in the same transaction. The
existing CDM event contract already includes `SyncConflict`.

The implementation must not duplicate the CDM or bypass the Store. A failed
page must leave the durable cursor unchanged so replay is possible.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/specs/SPEC-010-nexus-interoperability.md`
- `.agent/specs/SPEC-012-bridge-lifecycle.md`
- `.agent/specs/SPEC-013-bridge-synthesis.md`
- `.agent/specs/SPEC-014-tenant-scoped-bridge-state.md`
- `wit/hydra-bridge.wit`
- `crates/bridge-host/src/host.rs`
- `crates/bridge-host/src/lifecycle.rs`
- `crates/store/src/entities.rs`
- `crates/store/src/events.rs`
- `crates/store/src/bridge_adapters.rs`
- `crates/kernel/src/bridge_runtime.rs`
- `crates/fabric/src/capabilities.rs`
- `crates/fabric/src/mcp.rs`
- `crates/fabric/src/rest/bridges.rs`
- `crates/fabric/src/services.rs`
- `TESTING.md`
- `SECURITY.md`

## 6. Files to Change

Expected changed files for this plan:

- `.agent/specs/SPEC-015-bridge-synchronization.md`
- `.agent/execplans/EP-035-governed-bridge-synchronization.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `scripts/security-check.sh`
- `scripts/check-observability.sh`
- `migrations/0020_bridge_synchronization.sql`
- `migrations/0021_bridge_adapter_config.sql`
- `crates/store/src/bridge_sync.rs`
- `crates/store/src/bridge_adapters.rs`
- `crates/store/src/events.rs`
- `crates/store/src/entities.rs`
- `crates/store/src/lib.rs`
- `crates/store/tests/bridge_sync.rs`
- `crates/bridge-host/src/lifecycle.rs`
- `crates/bridge-host/src/lib.rs`
- `crates/bridge-host/tests/lifecycle.rs`
- `crates/kernel/src/bridge_runtime.rs`
- `crates/kernel/tests/bridge_lifecycle.rs`
- `crates/fabric/src/capabilities.rs`
- `crates/fabric/src/mcp.rs`
- `crates/fabric/src/rest/nexus.rs`
- `crates/fabric/src/rest/bridges.rs`
- `crates/fabric/src/rest/mod.rs`
- `crates/fabric/src/rest/openapi.rs`
- `crates/fabric/src/services.rs`
- `crates/shell/src/routes/bridges.rs`
- `crates/fabric/tests/bridge_lifecycle.rs`
- `crates/fabric/tests/integration_contracts.rs`
- `crates/fabric/tests/mcp_contract.rs`
- `crates/fabric/tests/fixtures/mcp-tools-2025-11-25.sha256`
- `wit/hydra-bridge.wit`
- `reference/bridge/hydra-bridge.wit`
- `ARCHITECTURE.md`
- `COMMANDS.md`
- `ENVIRONMENT.md`
- `NEXUS_INTEGRATION.md`
- `NEXUS_INTEGRATION_AUDIT.md`
- `SECURITY.md`
- `TESTING.md`
- `DECISIONS.md`
- `PRODUCTION_READINESS.md`
- `.agent/execplans/EP-019-governed-bridge-activation-and-lifecycle.md`

## 7. Interfaces and Contracts

- Migration 0020 adds durable tenant/adapter/kind sync state, sync runs, and
  bounded conflict metadata. All foreign keys and queries remain tenant-safe.
- `BridgeLifecycle::sync_page` accepts a tenant, adapter identity, artifact
  identity, grant, config JSON, cursor, and bounded limit, and returns typed
  WIT changes plus the next cursor.
- `hydra.bridges.sync` maps to `bridges/sync_adapter`, requires
  `hydra.bridges.admin`, requires an idempotency key, and returns a receipt.
- REST `POST /v1/nexus/bridges/:id/sync` uses the same governed service path as MCP;
  the path accepts kind, limit, rationale, and idempotency key but never a
  Hydra tenant or cursor.
- The execution handler is registered as
  `execution-handler:bridges/sync_adapter/*` and is unavailable when the
  configured BridgeHost runtime is absent.
- Conflict events use the existing canonical `SyncConflict` payload and never
  include raw record data, secrets, or PII-shaped provider subjects.

## 8. Milestones

### M1 - Plan and additive persistence contract

Goal: activate this plan and add durable sync state without changing existing
records. Read the files in section 5. Change the plan/index/checker, SPEC-015,
and migration 0020. Validate with `bash scripts/check-execplan-state.sh` and
`bash scripts/preflight.sh`; expect `execplan state: ok` and `preflight: ok`.
Recovery: if the checker fails, restore the index/plan state consistency before
continuing; do not run application code against an unapplied schema.

### M2 - Store sync state and canonical application

Goal: implement tenant-scoped run/cursor/conflict repositories and bridge-origin
entity application. Change only the Store files/tests listed in section 6.
Validate with focused Store tests and `cargo sqlx prepare --workspace --
--all-targets`; expect all focused tests to pass and SQLx metadata to complete.
Recovery: rerun migrations in the disposable test database and inspect the
failed focused test; cursor advancement must remain transactional.

### M3 - Trusted BridgeHost and Kernel execution

Goal: invoke `changes-since` only through the existing Wasmtime host grants and
register the typed governed handler in the real runtime. Change the BridgeHost
and Kernel files/tests in section 6. Validate focused BridgeHost and Kernel
tests plus `cargo check --workspace --all-targets --offline`; expect no direct
adapter or Store bypass.
Recovery: narrow to one lifecycle or handler test; if a host error occurs,
leave the adapter state and sync cursor unchanged.

### M4 - Fabric capability and API contract

Goal: expose one canonical capability through MCP and REST proposal paths, with
stable schema and idempotency semantics. Change Fabric files/tests and update
the tool fixture digest. Validate capability/MCP/Fabric tests and OpenAPI
generation checks; expect the sync tool to be deterministic and authenticated.
Recovery: remove only the new route/descriptor wiring while preserving Store
and Kernel tests; do not expose an ungoverned mutation as a fallback.

### M5 - Full local verification and truthful closeout

Goal: update docs and close the plan only after all required gates pass. Change
the documented files in section 6. Validate `cargo fmt --all -- --check`,
`bash scripts/security-check.sh`, `bash scripts/dependency-audit.sh`,
`bash scripts/preflight.sh`, `bash scripts/check-execplan-state.sh`, and
`bash scripts/verify.sh`; expect each required success marker including
`verify: ok`. Recovery: document exact evidence and leave EP-035 ACTIVE if a
required gate cannot run; never replace a missing service with a warning-only
pass.

## 9. Concrete Steps

1. Add the additive migration with tenant/adapter/kind uniqueness, run lease
   state, bounded cursor/error fields, and conflict metadata. No raw payload
   column is allowed.
2. Add `BridgeSyncRepo` to `Store`, with validated identifiers, atomic start,
   finish, fail, cursor read, and conflict insertion methods.
3. Add Store-owned bridge-origin upsert and soft-delete helpers. Reuse the
   existing entity unique index and canonical event/outbox transaction path.
4. Add lifecycle request/result types and instantiate the adapter with the
   same tenant-scoped KV, grant, secret, egress, fuel, and artifact checks as
   `probe` before calling `changes_since`.
5. Add the Kernel handler. Check active registry state and descriptor capability,
   start a run, apply every page change, emit conflicts on failure, advance the
   cursor only after a complete page, and return a sanitized receipt.
6. Add capability metadata and MCP/REST proposal parsing. Reuse the existing
   external authorization, invocation context, Governor, and idempotency path.
7. Add tests for tenant isolation, concurrent runs, replay, cursor non-advance
   on conflict, soft delete, capability availability, and real runtime handler
   registration.
8. Update docs and append the EP-035 transition to the plan index.

## 10. Validation and Acceptance

Required outputs:

- `bash scripts/check-execplan-state.sh` -> `execplan state: ok`
- `bash scripts/preflight.sh` -> `preflight: ok`
- focused Store, BridgeHost, Kernel, and Fabric tests pass
- `cargo sqlx prepare --workspace -- --all-targets` completes successfully
- `cargo fmt --all -- --check` exits 0
- `bash scripts/security-check.sh` -> `security check: ok`
- `bash scripts/dependency-audit.sh` -> `dependency audit: ok`
- `bash scripts/verify.sh` -> `verify: ok`
- no production deployment, push, tag, or non-test database operation occurs

The final diff must be reviewed against section 6. Any additional generated
SQLx files are recorded as generated artifacts in the Decision Log rather than
silently omitted.

## 11. Idempotence and Recovery

Migrations are additive and run once through SQLx. Repeating an equivalent
request uses the existing external idempotency boundary. Replaying a failed
page reuses origin identity and does not advance a cursor twice. A run lease
that is active remains visible; no automatic replay of an ambiguous in-flight
run is performed by this plan. Conflict rows are append-only metadata and may
be reviewed without mutating the canonical entity.

## 12. Progress

- [x] M1 - Plan and additive persistence contract
- [x] M2 - Store sync state and canonical application
- [x] M3 - Trusted BridgeHost and Kernel execution (BridgeHost sync page 3/3; Kernel lifecycle 1/1; workspace test check passed on 2026-08-12)
- [x] M4 - Fabric capability and API contract (authenticated REST sync proposal 2/2; MCP contract 4/4; snapshot digest refreshed; OpenAPI route added on 2026-08-12)
- [x] M5 - Full local verification and truthful closeout (preflight, policy
  gates, lint, format, typecheck, workspace tests, security, dependency audit,
  and full `verify.sh` exited 0 against disposable loopback services on
  2026-08-12; Docker was unavailable and no production action occurred)

## 13. Surprises & Discoveries

- The WIT ABI and BridgeHost already expose incremental changes, so this plan
  can remain additive instead of changing the adapter ABI.
- The entity table already has the tenant/origin/origin_ref uniqueness index,
  which is sufficient for bridge-origin identity without a new CRM table.
- The existing canonical event contract already has `SyncConflict`; the plan
  adds durable conflict metadata rather than changing event payload shape.

## 14. Decision Log

- 2026-08-12 - Chose a governed manual page over a background scheduler. This
  is the smallest executable seam that proves end-to-end synchronization while
  preserving explicit autonomy and avoiding an untested worker lifecycle.
- 2026-08-12 - Chose to reject caller cursors. Cursor authority belongs to the
  tenant-scoped Store state so retries cannot skip or cross tenant history.
- 2026-08-12 - Chose conflict parking with no raw payload persistence. This
  preserves reviewability without duplicating provider data or storing PII in
  event/outbox metadata.
- 2026-08-12 - M1 passed: the state checker returned `execplan state: ok` and
  preflight returned `preflight: ok`; migration and Store implementation remain
  intentionally pending for M2.
- 2026-08-12 - M2 passed: migration 0020, SQLx metadata refresh, Store compile,
  and three focused transactional sync tests passed against disposable
  PostgreSQL on loopback port 55436.
- 2026-08-12 - Discovered that activation accepted adapter configuration but
  the registry retained only the grant. Added a separate additive config column
  so later synchronization invokes the adapter with the same validated config;
  legacy rows default to an empty object.
- 2026-08-12 - Exposed REST synchronization under `/v1/nexus/bridges/:id/sync`
  rather than the local `/v1/bridges` namespace. The adapter ID is path-bound,
  and the handler delegates to the same authenticated capability and durable
  external-idempotency path used by MCP.
- 2026-08-12 - Refreshed checked SQLx metadata with
  `cargo sqlx prepare --workspace -- --all-targets`; the generated `.sqlx`
  entries are expected repository artifacts for the additive queries.
- 2026-08-12 - The security gate reported a missing `python3` executable as
  an invalid alert YAML file under Git Bash. Hardened the existing wrapper to
  select `python3`, `python`, or `python.exe` and to distinguish a missing
  parser from malformed YAML; no new dependency was added.
- 2026-08-12 - Applied the same explicit Python command selection to the
  observability policy gate because the full verifier invokes it before the
  required success marker on Windows Git Bash.
- 2026-08-12 - The first full verifier reached clippy and reported only the
  two synchronization functions whose explicit arguments describe the atomic
  Store transaction contract. Added the repository's existing narrowly scoped
  `clippy::too_many_arguments` allowance; no behavior or API was weakened.
- 2026-08-12 - The next lint pass found three new test `unwrap()` calls in the
  synchronization assertions. Replaced them with descriptive `expect()` calls
  so missing durable rows fail with actionable diagnostics.
- 2026-08-12 - The full verifier first exposed stale capability-count and tool-
  ordering assertions in the Fabric integration contract after the canonical
  sync capability was added. Updated the contract to assert the 13-entry
  deterministic order (`hydra.envelopes.get` before `hydra.envelopes.list`).
- 2026-08-12 - M5 passed: the narrowed Fabric integration contract passed 1/1;
  `cargo fmt --all -- --check`, `git diff --check`, state validation, and
  preflight passed; the full `scripts/verify.sh` exited 0 in 665.9 seconds
  using disposable PostgreSQL on port 55436 and local NATS JetStream on
  `[::1]:4222`. No production deployment, push, tag, or production database
  operation occurred.

## 15. Outcomes & Retrospective

EP-035 is complete. Focused Store, BridgeHost, Kernel, Fabric REST, Fabric
MCP, and Fabric integration tests passed; checked SQLx metadata was refreshed;
format, diff, state, preflight, security, dependency, and full verification
gates passed locally. The full verifier used only disposable loopback
PostgreSQL/NATS services and Docker was unavailable, so no image or Compose
validation is claimed here.

The delivered seam is intentionally bounded: one authenticated, governed,
manual incremental page with a tenant-scoped cursor, conflict parking,
canonical entity application, and durable event/outbox behavior. Scheduler,
full relist, conformance, synthesis activation, canary, promotion, and
staging operator evidence remain future work. EP-010 remains PARTIAL because
its independent staging, recovery, soak, security, performance,
accessibility, and human-sign-off evidence is still absent.
