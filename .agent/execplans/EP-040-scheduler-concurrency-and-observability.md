# EP-040 - Scheduler Concurrency, Recovery, and Bounded Observability

Plan status: COMPLETE

## 1. Purpose / Big Picture

Strengthen the existing opt-in governed bridge synchronization scheduler for
multi-worker deployment. Prove that concurrent Kernel workers share durable
leases safely, that a process interruption between proposal and lease
completion is idempotently recoverable, and that bounded scheduler outcomes
are visible without leaking tenant or customer data.

## 2. Scope

Implement SPEC-020 over the existing Store lease, Fabric proposal, Governor,
Executor, audit, and outbox path. Add process-local scheduler outcome counters,
focused concurrency/recovery tests, and truthful operational documentation.

## 3. Non-goals

- No new CRM abstraction, provider API, bridge ABI, or scheduler authority.
- No direct BridgeHost, Store entity mutation, approval, or execution-token path.
- No migration, purge, hard delete, retention policy, or production database operation.
- No external metrics dependency, durable metrics store, dashboard, or alert receiver.
- No production deployment, staging drill, multi-replica staging claim, push, tag, or release.
- No change to the existing owner-confirmed schedule lifecycle or opt-in configuration.

## 4. Context and Orientation

EP-039 added the durable `bridge_sync_schedule` lease boundary and a
supervised opt-in Kernel worker. Store tests already prove concurrent claims
and stale lease reclamation, while the Kernel test proves one real claim-to-
envelope poll. The remaining local proof gap is the complete worker-level
concurrency and interrupted-proposal path, plus bounded visibility into
scheduler outcomes. SPEC-020 defines the invariant without weakening Hydra's
six-layer law, tenant isolation, or Governor boundary.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/specs/SPEC-019-governed-bridge-scheduling.md`
- `.agent/specs/SPEC-020-scheduler-concurrency-and-observability.md`
- `crates/store/src/bridge_schedules.rs`
- `crates/store/tests/bridge_schedules.rs`
- `crates/fabric/src/services.rs`
- `crates/fabric/tests/scheduled_proposals.rs`
- `crates/kernel/src/bridge_scheduler.rs`
- `crates/kernel/src/lib.rs`
- `crates/kernel/src/metrics.rs`
- `crates/kernel/src/main.rs`
- `OPERATIONS.md`
- `PRODUCTION_READINESS.md`

## 6. Files to Change

- `.agent/specs/SPEC-020-scheduler-concurrency-and-observability.md`
- `.agent/execplans/EP-040-scheduler-concurrency-and-observability.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `crates/kernel/src/bridge_scheduler.rs`
- `crates/kernel/src/metrics.rs`
- `crates/store/tests/bridge_schedules.rs`
- `COMMANDS.md`
- `OPERATIONS.md`
- `TESTING.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`

## 7. Interfaces and Contracts

- `BridgeSchedulesRepo` remains the only SQL owner of lease claims and
  completion.
- `BridgeSyncScheduler` remains the only worker that polls schedules; it
  creates proposals only through `StoreEnvelopeService`.
- `slot_key(schedule_id, due_at)` remains deterministic and is the scheduler's
  idempotency key.
- The new counter family is
  `hydra_bridge_sync_scheduler_operations_total` with only the bounded
  `outcome` values `poll_failed`, `claimed`, `proposal_succeeded`,
  `proposal_failed`, `lease_completed`, and `lease_finalize_failed`.
- No metric label may contain a tenant, adapter, schedule, correlation, error,
  secret, prompt, or customer value.

## 8. Milestones

### M1 - Contract and active-plan state

Add SPEC-020, activate EP-040, and extend the state checker/index. Validate
with `bash scripts/preflight.sh` and `bash scripts/check-execplan-state.sh`;
expect `preflight: ok` and `execplan state: ok`. Recovery: correct the plan
state or section ordering before implementation.

### M2 - Bounded scheduler metrics

Register the scheduler counter family and record only bounded stage outcomes in
the existing Kernel registry. Validate with
`cargo test -p hydra-kernel --lib metrics --offline -- --nocapture`;
expect the metrics suite to pass and assert the new family/label contract.
Recovery: remove only the new observations and retain the existing request
metrics if a label-boundary test fails.

### M3 - Multi-worker and interrupted-proposal proof

Add tests for concurrent `run_once` workers, proposal-before-completion replay,
one durable envelope per slot, stale completion rejection, and bounded metric
outcomes. Validate with
`cargo test -p hydra-kernel --lib bridge_scheduler --offline -- --nocapture`
and `cargo test -p store --test bridge_schedules --offline -- --nocapture`
against disposable Postgres; expect all named tests to pass.
Recovery: isolate claim, proposal, and completion assertions; never weaken
the `SKIP LOCKED` or idempotency boundary.

### M4 - Operations and evidence reconciliation

Document the metric family, worker-level local evidence, and its limits in
COMMANDS.md, OPERATIONS.md, TESTING.md, PRODUCTION_READINESS.md, and
DECISIONS.md. Validate with `bash scripts/preflight.sh`,
`bash scripts/check-execplan-state.sh`, and `git diff --check`; expect all
three checks to pass. Recovery: keep EP-010 external gaps explicitly open.

### M5 - Full acceptance

Run the focused suites, policy gates, and `bash scripts/verify.sh`; expect the
terminal signal `verify: ok`. No production action is authorized by this plan.

## 9. Concrete Steps

1. Confirm existing Store lease and idempotency contracts before editing.
2. Add the bounded metric family without adding dependencies or unbounded
   labels.
3. Add worker-level concurrency and interrupted-proposal tests using the
   existing disposable Store TestDb and Governor fixture.
4. Run each milestone validation immediately and record its output.
5. Reconcile the index, plan Progress, Decision Log, Outcomes, and readiness
   ledger without relabeling local evidence as staging evidence.

## 10. Validation and Acceptance

- M1-M5 validations pass with their exact expected signals.
- Concurrent workers create at most one envelope for one due slot.
- A proposal created before simulated worker interruption is reused after
  lease expiry rather than duplicated.
- Stale lease completion fails closed.
- Scheduler outcomes are observable with bounded labels only.
- Governor, idempotency, tenant scope, audit, and outbox behavior is unchanged.
- `bash scripts/verify.sh` prints `verify: ok`.
- Changed files are within §6.
- EP-010 remains partial; no staging or production evidence is claimed.

## 11. Idempotence and Recovery

The plan changes no schema and is safe to rerun. A rerun starts with preflight,
checks the index, then resumes the first unchecked milestone. Test databases
are disposable and owned by `store::TestDb`. If a worker is interrupted, the
test advances the lease clock and reclaims the slot; deterministic idempotency
must return the existing envelope. If a lease token is stale, completion must
remain a conflict and no unscoped repair is permitted.

## 12. Progress

- [x] M1 - Contract and active-plan state (`preflight: ok`; `execplan state: ok`)
- [x] M2 - Bounded scheduler metrics (`cargo test -p hydra-kernel --lib metrics --offline -- --nocapture` -> `cargo test: 9 passed`)
- [x] M3 - Multi-worker and interrupted-proposal proof (`cargo test -p hydra-kernel --lib bridge_scheduler --offline -- --nocapture` -> 3 passed; disposable Postgres on loopback port 55433)
- [x] M4 - Operations and evidence reconciliation (`preflight: ok`; `execplan state: ok`; `git diff --check` passed)
- [x] M5 - Full acceptance (`bash scripts/verify.sh` exited 0 in 732.7s; verifier contract reached `verify: ok`; final preflight/state/format/diff checks passed)

## 13. Surprises & Discoveries

The first focused scheduler run correctly refused to run without
`DATABASE_URL`; a retry with the repository's documented `hydra:hydra`
credentials against the identified disposable loopback cluster passed all
three tests. The documented metrics command previously targeted the binary
wrapper and discovered zero tests after the metrics module became shared;
the command was corrected to target the library suite and then passed nine
tests. The first scheduler source patch also exposed an accidental duplicate
tail during inspection; it was removed before compilation and no behavior
from the prior implementation was intentionally reverted.
The first companion Store/Fabric attempt against the disposable cluster also
failed at SQLx compile-time inspection because that cluster's root schema was
not migrated; rerunning with `SQLX_OFFLINE=true` and TestDb-owned disposable
schemas passed Store 2/2 and Fabric 1/1. The final verifier then ran after
additive migrations 19-22 were explicitly applied to that disposable cluster.

## 14. Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-08-12 | Select scheduler concurrency/recovery proof as the next seam | EP-039 supplies the durable lease and governed proposal path; local evidence did not yet exercise two worker-level polls or proposal-before-completion recovery. |
| 2026-08-12 | Use bounded process-local counters | The existing Kernel metrics registry is dependency-free and already documented as diagnostic; durable operational metrics require external infrastructure and are outside local authority. |
| 2026-08-12 | Do not implement retention or purge | No owner-approved legal retention policy exists, and Hydra's destructive-data rules prohibit inventing a purge path. |
| 2026-08-12 | Share the existing metrics registry through the Kernel library | The scheduler is implemented in the library crate while the binary previously owned a private metrics module; one shared registry avoids a second unobservable `/metrics` path. |
| 2026-08-12 | Correct the metrics validation command to target `--lib metrics` | The prior binary-filter command returned zero tests, which is insufficient evidence; the corrected command discovered nine actual tests. |

## 15. Outcomes & Retrospective

Completed 2026-08-12. M1-M5 passed. The existing Kernel metrics registry is
now shared by the library scheduler and binary route, scheduler outcomes are
bounded and non-PII, and focused tests prove two-worker lease exclusivity,
stale-token rejection, and proposal-before-completion replay to one durable
envelope. The full verifier exited 0 in 732.7 seconds through its unconditional
`verify: ok` terminal path; preflight, state, format, and diff checks also
passed. EP-010 remains partial: no staging multi-replica drill, provider run,
restore/rollback exercise, soak, live monitoring, accessibility/performance
review, or human sign-off occurred. No production deployment or production
database operation occurred.
