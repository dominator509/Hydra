# EP-039 - Governed Bridge Synchronization Scheduling

Plan status: COMPLETE

## 1. Purpose / Big Picture

Implement the optional scheduled synchronization seam required to move Hydra
from manually governed bridge synchronization toward production operations,
without introducing an autonomous mutation path. An owner-created durable
schedule will lease one due slot, create the existing governed sync envelope,
and let normal Governor/Executor/outbox behavior decide and execute it.

## 2. Scope

Add SPEC-019, additive schedule persistence, Store lease operations, an
internal governed proposal seam with durable provenance/idempotency, a
supervised Kernel scheduler worker, confirmation-gated owner CLI operations,
readiness/configuration truth, focused tests, and documentation.

## 3. Non-goals

- No direct bridge-host or CRM Store mutation from the scheduler.
- No automatic envelope approval, retry loop, provider passthrough, or new CRM abstraction.
- No schedule HTTP endpoint or caller-supplied tenant authority.
- No migration rollback, purge, hard delete, production deployment, push, or tag.
- No change to manual MCP/REST synchronization compatibility.
- No claim of EP-010 staging, multi-replica, restore, soak, or human sign-off evidence.

## 4. Context and Orientation

EP-035 and EP-037 provide one authenticated, Governor-gated manual sync path
for incremental and bounded full-relist adapters. `bridge_sync_run` already
prevents concurrent execution, but nothing creates a proposal on a durable
cadence. `RuntimeServices` and the Kernel supervisor already own supervised
Executor/relay workers. `hydra-admin` already provides confirmation-gated
owner mutations. Reuse these seams; do not create a second scheduler or CRM
abstraction.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/specs/SPEC-015-bridge-synchronization.md`
- `.agent/specs/SPEC-017-bridge-full-relist.md`
- `.agent/specs/SPEC-019-governed-bridge-scheduling.md`
- `crates/store/src/bridge_adapters.rs`
- `crates/store/src/bridge_sync.rs`
- `crates/store/src/idempotency.rs`
- `crates/fabric/src/services.rs`
- `crates/fabric/src/capabilities.rs`
- `crates/kernel/src/main.rs`
- `crates/kernel/src/runtime_services.rs`
- `crates/kernel/src/supervisor.rs`
- `crates/admin-cli/src/main.rs`
- `migrations/0020_bridge_synchronization.sql`
- `OPERATIONS.md`
- `ENVIRONMENT.md`

## 6. Files to Change (== Expected Changed Files)

- `.agent/specs/SPEC-019-governed-bridge-scheduling.md`
- `.agent/execplans/EP-039-governed-bridge-scheduling.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `migrations/0022_bridge_sync_schedule.sql`
- `crates/store/src/bridge_schedules.rs`
- `crates/store/src/lib.rs`
- `crates/store/tests/bridge_schedules.rs`
- `crates/fabric/src/services.rs`
- `crates/fabric/tests/scheduled_proposals.rs`
- `crates/kernel/src/bridge_scheduler.rs`
- `crates/kernel/src/lib.rs`
- `crates/kernel/src/config.rs`
- `crates/kernel/src/main.rs`
- `crates/kernel/src/supervisor.rs`
- `crates/kernel/tests/bridge_scheduler.rs`
- `crates/admin-cli/src/main.rs`
- `.env.example`
- `COMMANDS.md`
- `ENVIRONMENT.md`
- `OPERATIONS.md`
- `NEXUS_INTEGRATION.md`
- `ARCHITECTURE.md`
- `TESTING.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`

## 7. Interfaces and Contracts

- `BridgeSchedulesRepo` owns all schedule SQL, validation, due-row lease
  claims, completion, and owner mutations.
- `StoreEnvelopeService::propose_scheduled_bridge_sync` is the only scheduler proposal
  seam. It validates the capability binding, persists `InvocationContext`,
  resolves idempotency, evaluates the tenant Governor, and dispatches only a
  Governor-issued execute token.
- `BridgeSyncScheduler::run` polls only when explicitly enabled, claims a
  bounded batch, calls the internal proposal seam, and completes each lease.
- The Kernel supervisor treats an enabled scheduler worker like relay and
  Executor: unexpected exit requests coordinated shutdown; shutdown is bounded.
- The scheduler does not appear as an available capability when the sync
  handler is unavailable. Owner CLI operations remain local and confirmation
  gated.

## 8. Milestones

### M1 - Contract and active-plan state

Add SPEC-019, activate this plan, and extend the state checker to EP-039.
Validate with `bash scripts/preflight.sh` and
`bash scripts/check-execplan-state.sh`; expect `preflight: ok` and
`execplan state: ok`. Recovery: fix plan/index status or section order before
implementation.

### M2 - Durable schedule and lease Store boundary

Add migration `0022_bridge_sync_schedule.sql`, Store types, validation, owner
mutations, due claims, lease completion, and focused concurrent/reclaim tests.
Validate with `cargo test -p store --test bridge_schedules --offline -- --nocapture`;
expect the named suite to pass. Recovery: keep the schedule disabled and fix
the smallest SQL/lease issue; never weaken the existing sync-run unique guard.

### M3 - Internal governed proposal provenance

Add the internal proposal method and tests for Governor decisions, actor and
invocation fields, deterministic slot idempotency, and fail-closed capability
binding. Validate with `cargo test -p fabric --test scheduled_proposals --offline -- --nocapture`;
expect the named suite to pass. Recovery: revert to a proposal-only seam;
never call an execution handler or approval method from the scheduler.

### M4 - Supervised Kernel scheduler

Add explicit opt-in config, the bounded worker, supervisor/readiness wiring,
and real runtime construction. Validate with
`cargo test -p hydra-kernel --test bridge_scheduler --offline -- --nocapture`;
expect the named suite to pass. Recovery: leave scheduling disabled and report
the runtime unavailable if the sync handler is not registered.

### M5 - Owner operations and documentation

Add confirmation-gated schedule create/list/status/enable/disable operations,
update commands/environment/operations/architecture/Nexus/readiness docs, and
refresh SQLx metadata. Validate with `cargo test -p hydra-admin --offline` and
`bash scripts/check-execplan-state.sh`; expect both to pass.

### M6 - Full acceptance

Run format, diff, preflight, security, dependency, focused suites, and the
full verifier against disposable loopback services. Validate with
`bash scripts/verify.sh`; expect `verify: ok`. Do not claim production
readiness; record any external EP-010 gaps unchanged.

## 9. Concrete Steps

1. Reconfirm the existing composite adapter foreign-key and active sync-run
   constraints before writing migration 0022.
2. Implement and test Store lease operations without raw SQL outside Store.
3. Add internal proposal provenance and idempotency without changing external
   authentication behavior.
4. Implement scheduler poll/claim/propose/complete with bounded failure and
   no direct Executor or BridgeHost access.
5. Wire opt-in startup, health, shutdown, and owner operations.
6. Run each milestone validation immediately and update this plan's Progress,
   Decision Log, and Outcomes.

## 10. Validation and Acceptance

- All M1-M6 commands pass with their expected markers.
- `bash scripts/verify.sh` prints `verify: ok`.
- Schedule rows are tenant-scoped and lease claims are replica-safe.
- Every scheduled action has internal actor, correlation, causation where
  present, and deterministic idempotency provenance.
- Governor remains the only approval/execution decision point.
- Disabled or unavailable scheduling fails closed and is observable.
- No secrets, prompts, customer bodies, or access tokens enter schedule rows,
  envelopes, logs, or events.
- `git diff --name-only` is within §6.
- No production deployment or production database action occurred.

## 11. Idempotence and Recovery

Migration 0022 is additive. Schedule creation rejects duplicate composite
keys. Claims use expiring tokens and `SKIP LOCKED`; an interrupted worker
leaves a reclaimable lease. Proposal retries reuse the deterministic slot key
and Store idempotency record. Completion with a stale token fails closed. A
rerun starts with `preflight`, inspects schedule state, and resumes the first
unticked milestone.

## 12. Progress

- [x] M1 - Contract and active-plan state (`preflight: ok`; `execplan state: ok`)
- [x] M2 - Durable schedule and lease Store boundary (`cargo test -p store --test bridge_schedules --offline -- --nocapture` -> `cargo test: 2 passed`)
- [x] M3 - Internal governed proposal provenance (`cargo test -p fabric --test scheduled_proposals --offline -- --nocapture` -> `cargo test: 1 passed`)
- [x] M4 - Supervised Kernel scheduler (`cargo test -p hydra-kernel --lib --offline -- --nocapture` -> `cargo test: 8 passed`; scheduler contract -> `cargo test: 1 passed`)
- [x] M5 - Owner operations and documentation (`cargo test -p hydra-admin --offline -- --nocapture` -> `cargo test: 4 passed`; docs and environment contract updated)
- [x] M6 - Full acceptance (`preflight: ok`; full `bash scripts/verify.sh` exited 0 after the corrected supervisor lint path; disposable Postgres stopped and port 55438 verified closed)

## 13. Surprises & Discoveries

The first focused Store invocation omitted `DATABASE_URL` and failed before
tests started; rerunning it against the disposable loopback Postgres cluster
passed two tests. SQLx preparation then completed after explicit casts were
added to the schedule completion update. The first two full-verifier attempts
stopped at Clippy for a needless reborrow and then an unused `mut` in the new
three-worker supervisor; both were corrected before the final run. The final
full verifier exited 0 after 996.9 seconds. No provider, staging, or production
dependency was used as scheduler evidence.

## 14. Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-08-12 | Select scheduled synchronization as the next seam | Manual incremental/full-relist execution is verified, while the documented operational gap is the absence of a durable scheduler; staging-only EP-010 evidence remains separate. |
| 2026-08-12 | Make scheduling owner-created and explicitly opt-in | Autonomous background mutation without an operator-controlled schedule and runtime flag would violate fail-closed authority and make standalone Hydra behavior surprising. |
| 2026-08-12 | Keep all schedule SQL in Store and disable rows instead of deleting them | The Store remains the only SQL boundary, while soft-disable preserves operational history and avoids destructive schedule removal. |
| 2026-08-12 | Route scheduler work through `propose_scheduled_bridge_sync` | A fixed internal method preserves the existing Governor/idempotency/dispatch path without manufacturing an external principal or adding a generic execute API. |
| 2026-08-12 | Enable scheduling only with `HYDRA_BRIDGE_SYNC_SCHEDULER_ENABLED=true` and a registered sync handler | The default remains standalone/manual Hydra; enabled configuration fails closed when the typed runtime capability is unavailable. |

## 15. Outcomes & Retrospective

M1-M6 are complete with focused and full disposable-service evidence. The full
verifier process exited 0 after the supervisor lint corrections. EP-010
production-readiness remains partial regardless of this plan's local result;
staging, multi-replica, recovery, soak, provider, accessibility, and human
sign-off evidence was not performed.
