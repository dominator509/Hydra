# SPEC-017 - Governed Bridge Full-Relist Synchronization

Status: Accepted for EP-037

## 1. Purpose

This specification defines the bounded full-relist fallback required by the
normative Hydra bridge ABI when an active adapter does not advertise
`incremental_sync`. Full-relist is a Hydra-owned, governed synchronization
mode of `hydra.bridges.sync`; it is not a second CRM model, a provider
passthrough, or an autonomous background worker.

## 2. Authority and boundaries

- An authenticated principal proposes the existing `hydra.bridges.sync`
  capability.
- Hydra Governor decides whether the proposal may execute.
- Kernel selects incremental or full-relist mode from the persisted adapter
  descriptor; caller input cannot select or override the mode.
- BridgeHost invokes only the WIT `list` read export for full-relist.
- Store validates and commits canonical CDM entities, soft deletes, sync
  state, audit, and outbox records in one tenant-scoped transaction.
- The adapter never receives SQL, a Hydra database handle, or a caller
  supplied Hydra tenant.

## 3. Request and bounded resource contract

The existing sync input remains unchanged:

```json
{
  "adapter_id": "memcrm",
  "kind": "party",
  "limit": 100
}
```

`limit` bounds each adapter page to 1-100 records. The host applies a fixed
maximum number of pages, records, and aggregate record bytes per invocation.
The limits are implementation constants and are not caller-controlled. A
repeated cursor, an invalid cursor, an over-limit page, a duplicate identity,
or a bound violation fails closed without committing a partial relist.

## 4. Relist and diff semantics

The host starts at a null WIT list cursor and follows `next_cursor` until the
adapter returns no next cursor. Each record must have the requested declared
kind, a bounded non-control-character ID, and an object-shaped JSON body.
Kernel converts the validated records into Store-owned sync changes; raw
records never appear in receipts, errors, event subjects, or task artifacts.

Store resolves identity by `(Hydra tenant, origin = bridge:<adapter_id>,
origin_ref = <kind>:<external_id>)`. In one transaction it:

- creates new records and emits canonical created events;
- updates changed records and revives matching soft-deleted records;
- leaves unchanged active records at their current version;
- soft-deletes active bridge-origin records of the requested kind that were
  absent from the complete relist; and
- advances the durable sync state only after all entity, audit, and outbox
  writes succeed.

Already soft-deleted historical rows are not purged. A failed or bounded
relist leaves the cursor/state and canonical entities unchanged, and its run
is marked failed with sanitized metadata.

## 5. Concurrency and idempotence

Only one `(tenant, adapter, kind)` sync run may be active. Repeating an
equivalent full relist is safe: unchanged active entities are not version
incremented, and the same external identity maps to the same canonical row.
The external idempotency behavior remains the existing envelope/idempotency
contract; no new caller-controlled cursor or run identity is introduced.

## 6. Capability truth and failure modes

The existing capability remains available when the typed runtime is present;
its handler supports both incremental and full-relist descriptors. It fails
closed for inactive or missing adapters, unsupported kinds, invalid persisted
descriptors/grants, host errors, invalid records, duplicate cursors, resource
bounds, concurrent runs, tenant mismatch, and unknown persistence state.
Scheduling, autonomous retries, canary, promotion, and activation remain
unavailable.

## 7. Non-goals

- Background scheduling or autonomous synchronization.
- A new public route, MCP tool, CRM abstraction, or provider API.
- Hard deletion, purge, automatic conflict resolution, or destructive
  reconciliation outside soft-delete semantics.
- Adapter write-back, mapping synthesis, canary, promotion, or deployment.
- Production deployment, live provider validation, or EP-010 sign-off.

## 8. Acceptance

Acceptance requires BridgeHost pagination and bound tests, Store atomic diff
and soft-delete tests, Kernel mode-selection/tenant-isolation tests, the
existing Fabric capability contract, SQLx metadata validation, formatting,
preflight, security, dependency, state, and full verification. The final
verifier must produce `verify: ok` with no masked required test.
