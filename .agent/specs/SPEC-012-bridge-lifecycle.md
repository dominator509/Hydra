# SPEC-012: Governed Bridge Activation and Lifecycle

## Status

Accepted for EP-019. This specification defines the first bounded lifecycle
slice; it does not complete bridge discovery, synthesis, synchronization,
canary, promotion, or production deployment.

## Authority and Invariants

1. `wit/hydra-bridge.wit` remains the only legacy-CRM adapter ABI.
2. Hydra Postgres remains authoritative for adapter metadata and lifecycle
   history; NATS is not a source of truth.
3. All lifecycle mutations requested through REST, MCP, Nexus, or an agent
   become ActionEnvelopes and pass the deterministic Governor.
4. Only typed execution handlers may instantiate or call an adapter.
5. Component loading is restricted to the configured `HYDRA_ADAPTERS_PATH`,
   rejects traversal/absolute paths/symlinks, and verifies a stored lowercase
   SHA-256 digest.
6. Grants contain only named origins, named secret references, optional
   read-replica DSN name, and fuel. Secret values never enter durable CRM,
   envelope, event, log, or response payloads.
7. Adapter execution uses Wasmtime fuel, BridgeHost-mediated egress, named
   secrets, adapter-scoped KV, and optional read-only SQL only when granted.
8. Tenant IDs are derived from authenticated authority and are never accepted
   from caller headers or bridge payloads.

## Durable Model

`bridge_adapter` is keyed by `(tenant_id, adapter_id)` and stores the logical
component reference, exact SHA-256, grant projection, optional descriptor,
state, last error, revision, and timestamps. `bridge_adapter_transition` is
append-only and records each state change, including the initial registration.
Supported states are `inactive`, `activating`, `active`, `paused`, and
`failed`. Optimistic revision and expected-state checks prevent lost updates.

## Governed Actions

- `bridges/deploy_adapter`: validate and probe a prebuilt component, then
  persist its descriptor and digest as active only after successful probing.
- `bridges/pause_adapter`: persist a paused state and stop future lifecycle
  work for the adapter.
- `bridges/resume_adapter`: persist an active state only after the stored
  component identity and grant remain valid.

The exact execution registry descriptor, payload schema, required approval,
receipt, and failure mapping are generated from the implementation and tested
as one registry. Unsupported workflow stages must be reported unavailable.

## Failure Behavior

Missing configuration, invalid path, changed digest, invalid grant, missing
named secret, fuel exhaustion, probe error, tenant mismatch, stale revision,
and unsupported workflow fail closed. A failed activation remains inactive or
failed and cannot be reported active. Retry of an equivalent request is
idempotent; reuse with a different component digest conflicts.

## Acceptance

EP-019 must prove Store tenant isolation and append-only history; Wasmtime
component-root/digest/grant/fuel enforcement; governed deploy/pause/resume;
runtime capability truth; safe REST status and receipts; focused and full
security/dependency/verify gates; and no production operation. Full adapter
sync, synthesis, canary, promotion, and staging readiness require later
accepted plans.
