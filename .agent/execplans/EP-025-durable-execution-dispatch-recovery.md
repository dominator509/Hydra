# EP-025 Durable Execution Dispatch Recovery

Plan status: COMPLETE

## 1. Purpose / Big Picture

Close the code-owned reliability gap where an approved ActionEnvelope is durable in Postgres but its ExecuteToken exists only in the Kernel process-local channel. A process crash or restart after the Governor commits `Approved` can otherwise strand work indefinitely. Hydra must recover only unambiguous durable `Approved` envelopes, preserve tenant and Governor boundaries, and fail closed when an action is already `Executing` and its outcome is uncertain.

## 2. Scope

- Add a Store-owned, bounded query for durable approved envelopes across tenants for internal Kernel recovery only.
- Add a private Executor recovery entry point that accepts a tenant/envelope identity only after Store reports the envelope as approved; do not expose a forgeable public ExecuteToken constructor.
- Extend the supervised ExecutorWorker with an immediate and periodic recovery scan while retaining the existing in-memory dispatch fast path.
- Make duplicate recovery attempts harmless through the existing tenant-scoped, concurrency-safe state transition.
- Surface stale `Executing` envelopes through readiness as a fail-closed operational condition rather than replaying potentially completed external work.
- Add executable Store and Kernel tests for restart-style recovery, tenant preservation, stale in-flight detection, and ordinary dispatch compatibility.
- Reconcile operations, security, readiness, audit, commands, decisions, and plan-state documentation.

## 3. Non-goals

- No new CRM abstraction, vendor API, arbitrary SQL path, or direct Nexus database access.
- No automatic replay of `Executing` envelopes whose external side effect may have completed before a crash.
- No new execution capability, approval bypass, model path, bridge ABI, or Governor rule.
- No destructive cleanup, hard delete, migration reversal, production database, staging deployment, push, merge, tag, or release.
- No distributed queue dependency; Postgres remains authoritative and the in-memory channel remains an optimization.

## 4. Context and Orientation

`crates/kernel/src/runtime_services.rs` currently builds `ExecutorWorker` around an in-memory `mpsc::Receiver<ExecuteToken>`. `crates/fabric/src/services.rs` persists an envelope as `Approved` and then dispatches the token, so a crash between those operations or while the process is restarting can leave an approved row with no process-local token. `crates/store/src/envelopes.rs` already owns tenant-scoped locking and legal transitions. `crates/governor/src/envelope.rs` permits `Approved -> Executing` but does not permit an ambiguous `Executing` envelope to be silently reset. The recovery seam must use these existing authorities rather than inventing a second queue.

The stale in-flight readiness threshold is 15 minutes from the envelope `updated_at` timestamp. This is an operational fail-closed signal, not proof that the external action failed; an operator must inspect the receipt/provider state before retrying.

## 5. Files to Read First

`AGENTS.md`; `COMMANDS.md`; `.agent/PLANS.md`; `.agent/EXECUTION_RULES.md`; `.agent/state/execplan-index.md`; `PRODUCTION_READINESS.md`; `OPERATIONS.md`; `SECURITY.md`; `NEXUS_INTEGRATION_AUDIT.md`; `DECISIONS.md`; `crates/store/src/envelopes.rs`; `crates/store/src/lib.rs`; `crates/governor/src/envelope.rs`; `crates/governor/src/decision.rs`; `crates/kernel/src/executor.rs`; `crates/kernel/src/runtime_services.rs`; `crates/kernel/src/main.rs`; `crates/kernel/tests/runtime_wiring.rs`; `crates/store/tests/integration_execution_provenance.rs`; `migrations/0003_envelopes.sql`; `migrations/0011_execution_receipts.sql`.

## 6. Files to Change

- `.agent/execplans/EP-025-durable-execution-dispatch-recovery.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `crates/store/src/envelopes.rs`
- `crates/store/tests/execution_recovery.rs` (new)
- `crates/kernel/src/executor.rs`
- `crates/kernel/src/runtime_services.rs`
- `crates/kernel/src/main.rs`
- `crates/kernel/tests/runtime_wiring.rs`
- `COMMANDS.md`
- `OPERATIONS.md`
- `SECURITY.md`
- `PRODUCTION_READINESS.md`
- `NEXUS_INTEGRATION_AUDIT.md` (append current verification only)
- `DECISIONS.md`
- `.sqlx/` (refresh checked query metadata)

## 7. Interfaces and Contracts

- `EnvelopesRepo::list_approved_all(limit)` is internal recovery metadata, returns tenant plus envelope identity, is bounded, deterministic, and never accepts caller-selected tenancy.
- `Executor::execute_recovered(tenant, envelope_id, clock)` is `pub(crate)` only; it reuses the same approval, handler, tenant, transition, verification, receipt, and audit path as ExecuteToken execution.
- `ExecutorWorker` scans immediately on startup and periodically with a bounded batch. It may dispatch the same envelope more than once across workers, but only one tenant-scoped `Approved -> Executing` transition can win.
- A stale `Executing` envelope is never automatically replayed. Readiness reports `execution_recovery_required` and operations must resolve the uncertain outcome through the existing audit/receipt/provider evidence.
- Existing `ExecutionDispatcher` behavior remains backward-compatible; a full in-memory channel continues to return capability-unavailable rather than silently dropping a token.
- `readiness_report` preserves `/readyz` body compatibility and adds a non-secret `execution_recovery` check to `/readyz/details`.

## 8. Milestones

### M1 - Activate and verify the dispatch-loss finding

Activate EP-025 after EP-024, run state/preflight, and prove the process-local channel versus durable `Approved` row from current symbols.

Validation: `bash scripts/check-execplan-state.sh` and `bash scripts/preflight.sh`

Expected: `execplan state: ok` and `preflight: ok`.

Recovery: repair the state index/checker before Rust changes; do not rewrite historical plans.

### M2 - Store recovery query and stale-state evidence

Add bounded `Approved` identity recovery and stale `Executing` count queries. Add migration-backed Store tests proving tenant identity, deterministic bounds, and the 15-minute fail-closed threshold. Refresh SQLx metadata.

Validation: `cargo test -p store --test execution_recovery --offline -- --nocapture` and `cargo sqlx prepare --workspace -- --all-targets`.

Expected: Store recovery tests pass and SQLx metadata is refreshed without schema changes.

Recovery: use the isolated loopback database from `ENVIRONMENT.md`; never query or mutate a non-test database.

### M3 - Runtime recovery worker and readiness boundary

Add the private Executor recovery path, startup/periodic worker scan, duplicate-safe behavior, and readiness projection for stale in-flight execution. Preserve the existing dispatch path and approval checks.

Validation: `cargo test -p hydra-kernel --test runtime_wiring --offline -- --nocapture` and `cargo check -p hydra-kernel --tests --offline`.

Expected: an approved envelope executes after worker construction without a dispatched token; stale executing work is reported and not replayed.

Recovery: if concurrency exposes an illegal transition, narrow to the Store row lock and preserve fail-closed behavior; never reset `Executing` to `Approved` automatically.

### M4 - Documentation and acceptance wiring

Document restart recovery, stale execution handling, operator response, and the distinction between local evidence and staging drills. Add ADR-0035 and the command/state references.

Validation: `bash scripts/check-execplan-state.sh` and `bash scripts/preflight.sh`.

Expected: `execplan state: ok` and `preflight: ok`.

Recovery: update the owning operational contract; do not claim automatic recovery of uncertain external side effects.

### M5 - Full isolated verification and reconciliation

Run focused tests, full lint/typecheck/security/dependency/integration/E2E gates, and `bash scripts/verify.sh` against loopback Postgres and JetStream. Complete only when the durable approved recovery path and stale-state safety are evidenced.

Validation: explicit isolated `bash scripts/verify.sh`.

Expected: exit 0 through the terminal `verify: ok` path; no production or staging action.

Recovery: follow AGENTS.md §7 and preserve external staging gaps in EP-010.

## 9. Concrete Steps

1. Activate EP-025 and validate state/preflight.
2. Add Store approved-envelope recovery and stale-execution queries with tests.
3. Add the private Executor recovery call and worker scan.
4. Add readiness reporting and runtime restart-style tests.
5. Update operations/security/readiness/audit/decision/command documentation.
6. Refresh SQLx metadata and run focused/full acceptance.
7. Reconcile EP-025 and retain EP-010's operator-owned partial status.

## 10. Validation and Acceptance

- The Store returns only bounded, tenant-qualified approved envelope identities for internal recovery.
- A process restart-style worker construction executes a previously approved envelope without a retained in-memory token.
- Existing direct dispatch still works.
- Concurrent recovery attempts cannot both transition or execute the same envelope.
- Stale `Executing` envelopes are visible through readiness and are not replayed automatically.
- Recovery cannot construct a public or caller-forgeable ExecuteToken.
- No new SQL boundary, capability, credential, or tenant authority is exposed.
- SQLx metadata, focused tests, preflight, state validation, and isolated full verification pass.
- EP-010 remains partial for staging, recovery drills, human review, and production sign-off.

## 11. Idempotence and Recovery

The read-only recovery scan is bounded and safe to repeat. The existing row lock and legal state transition make duplicate scans converge on one executor winner. If a worker crashes after `Executing`, the next scan does not replay the action; readiness fails closed after 15 minutes and the operator must resolve the uncertain outcome. If interrupted, resume at the first unchecked milestone. Never reset or delete envelope, receipt, event, or audit rows.

## 12. Progress

- [x] M1 - Dispatch-loss finding verified and EP-025 activated.
- [x] M2 - Store recovery query and stale-state tests complete (`cargo test -p store --test execution_recovery --offline -- --nocapture` -> `cargo test: 2 passed`; checked SQLx metadata refreshed).
- [x] M3 - Runtime recovery worker and readiness boundary complete (`cargo test -p hydra-kernel --test runtime_wiring --offline -- --nocapture` -> `cargo test: 6 passed`; readiness projection and concurrent exactly-once receipt tests pass).
- [x] M4 - Documentation, ADR, and acceptance wiring complete (`bash scripts/check-execplan-state.sh` -> `execplan state: ok`; `bash scripts/preflight.sh` -> `preflight: ok`).
- [x] M5 - Full isolated verification and truthful reconciliation complete (isolated `bash scripts/verify.sh` -> exit 0 in 383.0s with required success markers, including `verify: ok`, on 2026-08-11).

## 13. Surprises & Discoveries

The existing Governor sealed ExecuteToken cannot be reconstructed outside the governor crate. The recovery path therefore had to be a private Executor identity method rather than a public token factory, preserving the authority boundary while still allowing the Store-backed worker to recover approved work.

## 14. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-11 | Recover only durable `Approved` envelopes | Replaying `Executing` work could duplicate an external side effect whose result is unknown; fail-closed visibility is safer than an unprovable retry. |
| 2026-08-11 | Keep ExecuteToken construction sealed | Recovery uses a private Executor identity path so models, agents, Fabric callers, and generic code cannot mint execution authority. |
| 2026-08-11 | Use bounded Postgres scans instead of a new queue dependency | Postgres is already authoritative for envelopes and receipts; adding NATS/Redis as a second command source would violate Hydra's source-of-truth boundary. |
| 2026-08-11 | Surface stale `Executing` rows through readiness instead of retrying them | A crash can occur after an external side effect but before the receipt commits; automatic replay would risk duplicate CRM/provider actions. |

## 15. Outcomes & Retrospective

EP-025 is complete for code-owned durable execution recovery. EP-010 remains partial until real staging recovery/rollback drills, identity/TLS, security/privacy/accessibility/performance reviews, observability, and human sign-off exist.
