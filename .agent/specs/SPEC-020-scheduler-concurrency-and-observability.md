# SPEC-020: Governed Scheduler Concurrency and Observability

## Status

Accepted for EP-040. This specification governs the existing opt-in bridge
synchronization scheduler. It does not create a new CRM mutation abstraction.

## Purpose

Hydra may run more than one Kernel replica. A due bridge-sync schedule must be
claimed by at most one worker for a slot, while an interrupted worker must be
safe to retry. Scheduler retries must remain proposals through the existing
Governor, idempotency, Executor, audit, and outbox path.

## Normative requirements

1. **Tenant and schedule authority**
   - All schedule rows and lease completion calls remain tenant-scoped.
   - A lease token is opaque, non-empty, and required for completion.
   - A stale or wrong lease token MUST fail closed and MUST NOT overwrite a
     newer worker's completion.

2. **Concurrent workers**
   - Due-row claims MUST use the existing Store transaction and row-locking
     boundary with `SKIP LOCKED` semantics.
   - Concurrent workers MUST produce no more than one active lease for a
     schedule slot.
   - The scheduler MUST NOT call BridgeHost or mutate canonical CRM state
     directly.

3. **Interrupted proposal recovery**
   - The slot identity MUST be deterministic from the schedule ID and due
     timestamp.
   - If a worker creates a proposal and exits before completing its lease, a
     later worker MAY reclaim the expired lease.
   - Replaying the slot MUST resolve to the same envelope through the existing
     tenant/origin/capability/idempotency boundary.
   - A retry with a different request hash MUST fail as an idempotency
     conflict.

4. **Governance**
   - Every scheduled action MUST retain `hydra.scheduler` provenance and the
     existing internal actor type.
   - No scheduler path may approve an envelope or mint an execution token.
   - Unavailable typed handlers remain unavailable and fail closed.

5. **Metrics**
   - The Kernel MAY expose process-local counters for scheduler stages.
   - Metric labels MUST be bounded constants only; tenant IDs, adapter IDs,
     schedule IDs, correlation IDs, errors, secrets, and customer data MUST
     not appear in labels or metric values.
   - Counter state resets on process restart and MUST NOT be described as
     durable operational evidence.

6. **Failure behavior**
   - Store claim failures remain visible through the existing readiness and
     dependency boundary; the worker MUST not silently fall back to an
     in-memory schedule.
   - Proposal failures complete the owned lease with a bounded error and leave
     the schedule eligible for a later due slot.
   - Lease-finalization conflicts are logged without attempting an unscoped
     repair.

## Acceptance

The implementation is accepted only when focused tests prove concurrent
workers, interrupted-proposal replay, stale-token rejection, deterministic
idempotency, and bounded metric labels. Full repository verification remains
required. These tests are local disposable-service evidence only and do not
satisfy EP-010 staging, recovery-drill, soak, or human sign-off requirements.
