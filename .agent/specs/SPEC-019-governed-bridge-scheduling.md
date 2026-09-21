# SPEC-019 - Governed Bridge Synchronization Scheduling

Status: Accepted for EP-039.

## 1. Purpose

Provide an optional, owner-controlled scheduler for the existing governed
`hydra.bridges.sync` capability. Scheduling creates ActionEnvelope proposals;
it never executes a bridge, approves an envelope, or writes CRM state directly.

## 2. Scope and non-goals

This specification covers durable schedule configuration, PostgreSQL lease
claims, internal provenance, idempotency, supervised Kernel execution, and
owner CLI operations. It does not add a CRM model, provider passthrough,
autonomous approval, automatic retries outside the schedule interval, browser
routes, or production deployment.

## 3. Schedule contract

Each schedule is keyed by `(hydra_tenant_id, adapter_id, kind)` and contains:

- stable schedule ID
- interval seconds, bounded to 60 through 86,400
- page limit, bounded to 1 through 100
- enabled status
- next due time
- optional lease token and lease expiry
- last envelope, start, finish, and redacted error references
- monotonic revision and timestamps

The row references the tenant-scoped `bridge_adapter` registry with a
composite foreign key. Schedules are disabled by default unless explicitly
created by the confirmation-gated owner CLI. Only active adapters are eligible
for a claim.

## 4. Lease and concurrency rules

The Store claims due rows with `FOR UPDATE SKIP LOCKED`, a bounded lease, and a
random lease token. A worker must present that token to complete the claim.
Expired leases may be reclaimed. A lost lease cannot update the schedule.
Multiple Kernel replicas therefore create at most one proposal per claimed
slot, while the existing unique active sync-run constraint remains the final
bridge execution guard.

## 5. Governed proposal rules

For every claimed row, the scheduler constructs the existing `sync_adapter`
envelope shape:

- domain `bridges`
- action `sync_adapter`
- no envelope kind; the adapter kind remains in the validated payload
- non-empty schedule target
- bounded `adapter_id`, `kind`, and `limit`
- reversal `Compensating`
- no external sends, money, or PII egress

The proposal uses an internal Hydra principal with:

- `principal_type = hydra_internal_agent`
- `principal_id = hydra-scheduler`
- `origin_system = hydra.scheduler`
- request/correlation IDs derived from the schedule slot
- idempotency key `bridge-sync-schedule/<schedule-id>/<slot>

The Governor evaluates every proposal. `Block`, `Suggest`, `Queue`, and
`Execute` retain their normal semantics. The scheduler may not manufacture an
approval or call the Executor directly. A queued proposal remains available
for the existing human approval path.

## 6. Configuration and operations

Scheduling is disabled unless `HYDRA_BRIDGE_SYNC_SCHEDULER_ENABLED=true`.
When enabled, startup fails closed if the sync capability is unavailable. The
owner CLI is the only schedule mutation path in this plan and requires
`HYDRA_ADMIN_CONFIRM=I_UNDERSTAND`. There is no unauthenticated HTTP schedule
authority.

## 7. Failure and recovery

Proposal failures are stored as bounded redacted schedule errors and the next
interval remains the next retry boundary. Process death leaves a lease to
expire; it does not create an ungoverned replay. Shutdown stops the worker and
allows leases to expire. Unknown adapter state, invalid descriptor, invalid
configuration, missing handler, or lost lease fails closed.

## 8. Acceptance

- schedule creation validates bounds and tenant-scoped adapter references
- duplicate schedule keys are rejected deterministically
- due-row claims are isolated across concurrent workers
- expired leases can be reclaimed and lost leases cannot complete
- schedule proposals carry internal actor, slot correlation, and idempotency
- repeated slot proposal returns the same envelope
- Governor Block/Queue/Execute behavior is preserved
- scheduler never calls bridge execution or approval directly
- disabled/unavailable scheduler behavior is truthful in readiness and logs
- owner CLI mutations require explicit confirmation
- supervised startup and shutdown cover the scheduler worker
- focused Store/Fabric/Kernel tests and the full verifier pass

