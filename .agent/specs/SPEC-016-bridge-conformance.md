# SPEC-016: Governed Bridge Conformance

Status: Accepted for EP-036
Version: 1.0

## 1. Purpose

Define a read-only, authenticated conformance workflow for a configured
Hydra bridge adapter. Conformance proves that a digest-pinned Wasmtime/WIT
component satisfies the bounded describe, probe, schema, list, and optional
incremental-read contracts without activating it or mutating CRM state.

## 2. Authority and Call Path

The only external path is:

`authenticated A2A bridge-conformance task -> Fabric authorization -> Kernel
BridgeConformanceRuntime -> Store adapter lookup -> BridgeHost conformance`

The authenticated principal supplies only the adapter identifier, optional
kind, and bounded page limit. Hydra resolves the tenant, component reference,
digest, grant, and adapter configuration from the tenant-scoped Store record.

## 3. Input Contract

The task input is a bounded JSON object:

```json
{"adapterId":"memcrm","kind":"Contact","limit":25}
```

- `adapterId` is 1-128 bytes and is tenant-bound by Store lookup.
- `kind` is optional and is at most 128 bytes.
- `limit` is optional and defaults to 25; the allowed range is 1-100.
- Unknown fields, control characters, traversal markers, and oversized input
  fail closed.

## 4. Conformance Contract

BridgeHost must:

1. load the stored component reference and expected SHA-256 digest;
2. validate the stored grant and instantiate through the existing host state;
3. require `describe` and `probe` descriptors to match in name and version;
4. validate descriptor names, versions, kinds, and capability consistency;
5. for the selected kind, call `introspect-schema` and one bounded `%list`
   page, validating record shape, IDs, JSON objects, and cursor bounds;
6. when `incremental-sync` is declared, call `changes-since` from the empty
   cursor and validate operation, kind, ID, JSON, duplicate identity, and
   next-cursor bounds;
7. return only descriptor metadata, digest, fuel remaining, counts, and a
   deterministic pass/fail report. Raw records, field values, URLs, secrets,
   and provider response bodies never leave the host.

The workflow must not call `upsert`, `delete`, or any other mutating adapter
operation. It must not write Store rows, outbox events, audit records, or CRM
entities.

## 5. A2A Contract

The existing `bridge-conformance` workflow becomes available only when the
configured BridgeConformanceRuntime and tenant-scoped adapter record exist.
It requires the authenticated `hydra.bridges.read` scope and returns a
durable completed or failed task artifact. Task idempotency, tenant isolation,
correlation, and bounded history remain governed by the existing A2A task
service.

Unavailable adapters and invalid configuration return a generic unavailable
or validation result without leaking adapter existence across tenants.

## 6. Non-goals

- No adapter activation, pause, resume, synchronization, canary, or promotion.
- No generated Wasm or source-code generation or execution.
- No ActionEnvelope, Governor mutation, CRM write, or event/outbox record.
- No direct SQL outside Store and no new migration or dependency.
- No vendor passthrough, raw provider data, secrets, or PII in artifacts.
- No claim of staging, production, or EP-010 readiness evidence.

## 7. Acceptance

- Invalid and cross-tenant requests fail closed.
- Digest changes and invalid grants fail closed.
- Descriptor/probe mismatch fails closed.
- Schema, list, cursor, duplicate-identity, and JSON violations fail closed.
- A valid fixture adapter returns deterministic metadata-only conformance.
- A2A discovery reports availability from the real configured runtime.
- Equivalent A2A retries return the same task; conflicting reuse fails.
- No mutation method is called and no raw record value is returned.
