# SPEC-014: Tenant-Scoped Bridge State

**Status:** Accepted for EP-034
**Owner:** Hydra Store and BridgeHost
**Scope:** Adapter scratch state used by the bridge runtime

## Purpose

Hydra adapter identities are scoped by tenant, but the historical adapter KV
table is keyed only by `adapter_id`. This specification defines the additive,
tenant-safe replacement boundary used by production bridge lifecycle code.

## Normative Requirements

1. Every production adapter KV read and write MUST be authorized by both a
   Hydra tenant ID and an adapter ID.
2. Equal adapter IDs belonging to different tenants MUST have isolated keys,
   values, and visibility.
3. The Store layer remains the only SQL boundary.
4. The historical `adapter_kv` table MUST NOT be reinterpreted by guessing a
   tenant for existing rows. It remains for compatibility and inspection only.
5. Unscoped legacy `get` and `set` APIs MUST fail closed without issuing SQL.
6. Tenant IDs are derived from authenticated or governed Hydra context; they
   MUST NOT come from adapter configuration or bridge payload data.
7. Keys and values are scratch state only. They MUST NOT contain access
   tokens, secrets, customer records, or tenant authority.
8. Nil tenant IDs, blank identifiers, control characters, and oversized
   inputs MUST be rejected deterministically.
9. The migration MUST be additive and safe to run repeatedly through the
   repository migration mechanism.

## Persistence Contract

EP-034 adds `tenant_adapter_kv` with primary key `(tenant_id, adapter_id, k)`.
The old `adapter_kv` table is retained without a compatibility fallback,
because its rows do not carry enough information to establish tenant
ownership safely.

## Runtime Contract

`TenantStoreKvStore` carries a non-nil tenant ID and adapter ID and delegates
to tenant-scoped Store methods. `BridgeLifecycle::probe` receives the tenant
from the governed request. The kernel passes the envelope tenant when it
constructs the lifecycle request.

## Failure Semantics

Unknown, malformed, or out-of-bound state requests fail closed. A tenant may
observe only its own adapter scratch state. No migration or runtime path
backfills historical unscoped state automatically.

## Non-Goals

This specification does not implement CRM synchronization, mapping
synthesis, canary promotion, bridge deployment, or provider credential
management. Those capabilities remain unavailable unless their own runtime
contracts are implemented and advertised truthfully.

## Acceptance

- The additive migration applies on a disposable PostgreSQL database.
- Store tests prove equal adapter IDs remain isolated across tenants.
- Legacy unscoped Store methods fail without SQL.
- BridgeHost lifecycle tests use the tenant-scoped KV implementation.
- Kernel governed bridge execution passes the envelope tenant.
- Preflight, security, dependency, and full verification gates remain green.

