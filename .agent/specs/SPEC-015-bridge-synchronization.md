# SPEC-015 - Governed Bridge Synchronization

Status: Accepted for EP-035

## 1. Purpose

This specification defines the first executable synchronization seam between a
Wasmtime bridge adapter and Hydra's canonical CRM data model. Synchronization
is a Hydra-owned, governed command. It is not a direct provider passthrough,
an alternate CRM model, or a source-of-truth transfer to NATS.

## 2. Authority and boundaries

- The authenticated external principal proposes a capability through Fabric.
- Hydra Governor decides whether the proposal may execute.
- Kernel dispatches only a typed `bridges/sync_adapter` handler.
- BridgeHost invokes only the WIT `changes-since` export through Wasmtime.
- Store validates and commits canonical CDM entities, soft deletes, sync state,
  conflict records, audit, and outbox records.
- The adapter never receives SQL or a Hydra database handle.
- Cursor state is scoped by Hydra tenant, adapter identity, and CDM kind.
- A caller-supplied tenant or cursor is never authoritative.

## 3. Sync request

The canonical capability is `hydra.bridges.sync` and the governed action is
`sync_adapter`. Its input is:

```json
{
  "adapter_id": "memcrm",
  "kind": "party",
  "limit": 100,
  "rationale": "reconcile the latest customer changes",
  "idempotency_key": "sync-2026-08-12-001"
}
```

`adapter_id` is 1-128 characters, `kind` is a registered CDM kind, `limit` is
1-100, `rationale` is 1-2000 characters, and `idempotency_key` is 1-200
characters. The cursor is read from durable Hydra state and is not accepted in
the external request.

## 4. Adapter and record contract

The adapter must be active and its persisted descriptor must advertise
`incremental_sync: true` and the requested kind. The host calls
`changes-since(cursor, limit)` using the existing WIT 1.0 ABI. Each raw record
must have a nonblank bounded kind and ID, JSON object data, and a kind matching
the requested sync kind. Unknown kinds and CDM schema violations are parked as
conflicts rather than ingested.

Bridge records use `origin = bridge:<adapter_id>` and an origin reference that
uniquely identifies the adapter, kind, and external record ID. Existing entity
identity is resolved by the tenant-scoped origin uniqueness constraint. The
Store remains the only SQL boundary.

## 5. Cursor and transaction semantics

Each `(Hydra tenant, adapter, kind)` has one durable cursor and one active run
at a time. A run starts with an atomic lease. A successful page applies all
changes and advances the cursor in one transaction. A page containing a
validation, provider, or persistence conflict does not advance the cursor.
Already-applied records are safe to replay because origin identity and
idempotent version handling are Store-owned. A failed or abandoned run is
visible and may be retried by a later governed request.

## 6. Change application

- `upserted` records are validated against the CDM kind schema and persisted as
  canonical entities with bridge provenance.
- `deleted` records soft-delete the matching bridge-origin entity. A missing
  entity is recorded as a conflict because deletion cannot be verified.
- Raw adapter payloads are not copied into conflict events or NATS subjects.
- Conflict rows store bounded opaque metadata and a sanitized reason only.
- Every canonical entity change and conflict event uses the invocation's
  correlation, causation, and envelope identifiers where available.

## 7. Output and failure modes

Successful execution returns a governed receipt containing the sync run ID,
adapter, kind, applied upsert/delete counts, conflict count, and resulting
cursor. It does not return raw CRM records.

The handler fails closed for inactive adapters, missing or false incremental
sync capability, tenant mismatch, invalid payloads, invalid grants, host
errors, schema failures, concurrent runs, and unknown persistence state.
Conflicts are durable and observable through the run record and canonical
`hydra.crm.sync.conflict.v1` event.

## 8. Non-goals

- Full-relist diffing for adapters without incremental sync.
- Background scheduling or autonomous synchronization.
- Bridge mapping synthesis or generated mapping execution.
- Automatic conflict resolution or destructive deletion.
- Provider-specific APIs outside the WIT adapter ABI.
- Production deployment, live provider validation, or staging sign-off.

## 9. Acceptance

Acceptance requires focused Store, BridgeHost, Kernel, Fabric, tenant-isolation,
cursor-replay, conflict, and capability tests; `cargo fmt --all -- --check`;
`bash scripts/preflight.sh`; `bash scripts/security-check.sh`;
`bash scripts/dependency-audit.sh`; SQLx metadata validation; and
`bash scripts/verify.sh` producing `verify: ok` with no masked required test.

## Current Verification Status (EP-037, 2026-08-12)

EP-035's incremental path remains implemented as specified. EP-037 adds the
separately specified WIT full-relist fallback for descriptors with
`incremental_sync: false` while preserving this capability's authenticated,
Governor-controlled, Store-only mutation boundary. Scheduling, activation,
canary, promotion, and EP-010 production evidence remain outside both plans.
