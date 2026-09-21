# Hydra and Nexus Integration

Bridge adapter scratch state is tenant-scoped. Nexus never supplies tenant
authority to bridge state; the authenticated Hydra binding selects the Hydra
tenant, and governed lifecycle requests carry that tenant into Store-backed
`tenant_adapter_kv`. Historical unscoped `adapter_kv` rows are not a runtime
fallback.

## Boundary

Hydra is Nexus's CRM/revenue bounded context, not a Nexus-owned database or vendor proxy. Hydra remains the canonical CRM source of truth and can run without Nexus. Nexus may hold Hydra entity references and deterministic projections, but it must not maintain a writable duplicate CRM.

Nexus connects through three versioned seams:

- MCP Streamable HTTP for authenticated agent reads and governed proposals.
- `/v1/nexus/` REST for deterministic service integration.
- NATS JetStream for durable canonical Hydra events.

Nexus never connects to Hydra Postgres, bridge secrets, raw vendor APIs, arbitrary SQL, unrestricted providers, or direct mutation handlers.

## Identity and tenancy

Fabric validates an asymmetric short-lived token and translates verified claims into a generic `PrincipalContext`. Hydra then resolves an active `ExternalTenantBinding` from external provider, tenant, and business identifiers to exactly one Hydra tenant in v1. Request headers, MCP `_meta`, and tool arguments cannot select or override that tenant.

Bindings can be disabled or revoked. A missing, ambiguous, disabled, or mismatched binding fails closed without revealing whether another tenant or business exists. Standalone Hydra principals continue to use Hydra's local identity path and need no external binding.

## Dual gate

Nexus authorization answers: may this principal request this capability?

Hydra Governor answers: may this CRM action execute under the tenant's current policy and the action's risk/reversal/blast properties?

Both must allow execution. External commands become ActionEnvelopes. Models and agents may propose work or request human approval; they cannot approve, execute, or mutate the store directly.

## Capability contract

One typed capability registry drives MCP tool discovery, `GET /v1/nexus/capabilities`, execution-handler availability, schemas, and tests. A capability advertises stable name/version, category, schemas, scopes, risk/reversal/idempotency semantics, Governor binding, sync/async behavior, and current availability.

Reads return canonical typed projections. Mutations return envelopes or durable receipts. Unavailable bridge or agent behavior is reported unavailable rather than simulated.

The kernel constructs the Wasmtime/WIT BridgeHost. When `HYDRA_ADAPTERS_PATH` resolves to a trusted component root and the configured SecretSource is available, the runtime registers typed governed handlers for prebuilt adapter deployment, pause, resume, and manual synchronization; Fabric status is projected from the tenant-scoped registry. A new deployment probes the digest-pinned component, runs bounded read-only conformance, and enters `active` only after both pass; conformance failure is persisted as `failed`. Synchronization selects incremental or bounded full-relist behavior from the persisted descriptor. Missing or invalid lifecycle configuration fails closed. Bounded mapping-proposal synthesis is Experimental only when the Kernel has a configured TOKENKILLER `bridge_mapping` route; generated code, autonomous canary, and promotion remain unavailable.

Internal agent capability truth is separate from the public Nexus tool registry. DataSteward can emit an experimental governed merge proposal but cannot execute it; BridgeEngineer can emit only a bounded experimental mapping proposal when configured, and cannot activate it; Comms draft generation is available but transport is unavailable. The kernel emits this inventory from typed runtime descriptors at startup instead of treating placeholder behavior as complete.

The initial governed command is `hydra.crm.propose_action`, exposed through MCP and `POST /v1/nexus/proposals/stage-change`. Its fixed provider-neutral input accepts one canonical deal ID, target stage, rationale, idempotency key, and optional objective/task references. Both transports use the same capability dispatcher; the result is an envelope receipt, never a direct entity mutation. Availability is derived from the typed `pipeline/move_stage/deal` execution-handler registration.

The bounded bridge synchronization command is `hydra.bridges.sync`, exposed
through MCP and `POST /v1/nexus/bridges/{id}/sync`. Its input contains only a
canonical entity kind, page limit, rationale, and idempotency key; the REST
adapter ID is bound by the path. Neither transport accepts a Hydra tenant or
cursor. The proposal is evaluated by the Governor and executed, when allowed,
by the typed `bridges/sync_adapter` handler. Store-owned tenant/adapter/kind
state advances only after a complete WIT incremental page or a complete,
bounded full-relist snapshot. Full-relist diffs active bridge-origin rows in
one transaction and soft-deletes only records missing from the validated
snapshot. Invalid records are parked as bounded conflicts and emit a
canonical sync-conflict event. Owner-created scheduling is an optional local
operation controlled by `HYDRA_BRIDGE_SYNC_SCHEDULER_ENABLED`; it creates the
same governed envelope through a durable leased Store row and never grants
Nexus a direct scheduler or mutation authority. Synthesis, canary, and
promotion remain unavailable.

`POST /v1/nexus/envelopes/{id}/approval` records one immutable human-delegated approve or reject decision for a pending envelope. Approval requires `hydra.envelopes.approve`, a configured accepted authentication-strength claim, a distinct proposer/approver, and the bound Hydra tenant; agents cannot successfully call this path. MCP intentionally exposes no approval tool.

## Provenance and retries

Every external proposal carries a durable non-secret invocation context with request/correlation/causation and optional objective/task/approval/idempotency references. Access tokens, prompts, secrets, raw email bodies, and customer documents never enter this context.

Idempotency is scoped by Hydra tenant, origin system, key, and capability/action. An equivalent retry returns the existing envelope or receipt. Reusing a key with a different request hash returns a deterministic conflict.

Envelope creation, the idempotency record, and the initial Governor transition/audit/outbox records commit atomically. Authenticated provider, principal, request, correlation, causation, objective, task, and idempotency references are persisted in the non-secret invocation context. Caller-supplied tenant fields are rejected and never become authority.

## Events

Postgres audit and outbox records remain authoritative. The relay publishes a versioned canonical event to JetStream and waits for a publish acknowledgement before marking the outbox row published. Stable event IDs support consumer deduplication. Nexus uses a durable consumer and does not infer writes by sending NATS messages back to Hydra.

The reference Nexus consumer uses a named durable pull consumer, explicit acknowledgement, and a fixed non-PII subject filter. It validates the JSON Schema and typed v1 contract before acknowledging, records each `event_id` once in its projection, and resumes the same durable after a process restart. Redelivery and duplicate publishes are therefore safe at the consumer boundary; exactly-once business effects still depend on the consumer's durable `event_id` ledger rather than broker delivery count.

The v1 stream is `HYDRA_CRM_EVENTS_V1` and accepts only the fixed non-PII taxonomy under `hydra.crm.>`. Limits retention is bounded to 30 days, one million messages, 10 GiB total, and 1 MiB per message, with a 24-hour producer duplicate window. The relay publishes the persisted event UUID as `Nats-Msg-Id`; a duplicate acknowledgement is accepted only from the configured stream and preserves the broker sequence. Ordering is stream order, while consumers must use tenant, entity/envelope, correlation, and causation fields rather than assuming a global business transaction order.

Store-owned leases prevent multiple relays from claiming the same pending row without holding SQL locks across broker I/O. Transient publish failures release the row for retry. Invalid canonical rows are parked with a redacted reason and remain inspectable in authoritative Postgres state; they are never silently dropped.

Fabric derives a W3C server child from valid inbound `traceparent`/`tracestate`; malformed or missing context starts a fresh trace and never changes business correlation. The trace carrier is persisted separately beside asynchronous envelope transitions and outbox rows, restored by Executor, and emitted as JetStream headers. Hydra does not accept W3C baggage. Tokens, prompts, secrets, customer documents, and tenant authority never enter trace metadata.

`GET /v1/nexus/events/status` and Nexus-connected `/readyz` share one runtime status source. Availability requires the configured stream contract plus an operating relay; a required stream outage fails closed. Standalone mode remains healthy without enabling this external event contract.

## Optional EP-016 extensions

The optional model gateway is inserted only as `Hydra agent -> TOKENKILLER -> llm-router -> NexusModelProvider`. Its bearer token is loaded from the named age-vault secret configured by `NEXUS_MODEL_GATEWAY_TOKEN_SECRET`; agents cannot call the gateway directly, and local providers remain the fallback chain. A gateway is not private/PII-safe unless `NEXUS_MODEL_GATEWAY_PRIVATE=true` is explicitly configured.

The A2A facade at `/a2a` is limited to the allowlisted workflow methods in SPEC-011. It persists tenant-scoped task metadata in Store, supports deterministic replay/cancel/resume, and exposes `bridge-synthesis` only as an authenticated, idempotent, proposal-only workflow when its runtime service is configured. Other bridge workflows remain unavailable. It is not an entity CRUD or arbitrary execution API; streaming and push notifications remain unavailable.

Signed Agent Skills are declarative discovery metadata only. Each package has an upstream-compatible `SKILL.md` plus a Hydra-local `hydra-skill.json` Ed25519 manifest. Kernel loads an owner-controlled trust file and exposes only verified name/version/scope/capability metadata in its runtime inventory. Invalid signatures, revoked keys, content mismatches, unknown scopes/capabilities, non-declarative sandbox policies, and tool/credential declarations fail closed. Hydra does not execute skill scripts or grant skills credentials.

## Modes

Standalone mode disables Nexus resource-server routes and requires no Nexus trust anchor or binding.

Nexus-connected mode enables configured issuer/audience/JWKS validation, Origin policy, external bindings, `/v1/nexus/`, MCP, and the external event contract. Failure of required identity or event infrastructure makes affected readiness fail closed.

## Deployment seam

The reference Compose topology publishes only Caddy. Kernel, Postgres, and NATS remain private and separated; a trusted Nexus event consumer may join only the named attachable event network. Kernel has no external network attachment and reaches configured HTTP providers through the dedicated egress proxy, while BridgeHost grants remain the destination-authorization boundary.

`docker/nexus.env.example` is the Nexus-connected configuration template and `NEXUS_PACKAGE_CONTRACT.md` defines the future installer inputs. The installer must provision an owner and active external business binding through a Hydra-owned authenticated setup operation. Nexus must never bootstrap itself through direct SQL, caller-supplied Hydra tenant IDs, or a startup backdoor. No such external bootstrap endpoint is currently advertised.

## Version and status

The normative v1 requirements are in `.agent/specs/SPEC-010-nexus-interoperability.md`. Current implementation truth and known gaps are in `NEXUS_INTEGRATION_AUDIT.md`. GraphQL is not part of Nexus v1.

## Bridge conformance workflow

`bridge-conformance` is an authenticated A2A read workflow requiring
`hydra.bridges.read`. It is advertised only when the Kernel has a configured
BridgeHost conformance runtime; the tenant-scoped adapter record, active state,
stored component digest, grant, and configuration are resolved by Hydra rather
than supplied by Nexus. The workflow returns a durable completed or failed
task with metadata-only conformance output and preserves A2A idempotency and
correlation.

The host exercises `describe`, `probe`, `introspect-schema`, one bounded
`list` page, and `changes-since` when the adapter declares incremental sync.
It validates descriptor consistency, schema and record bounds, JSON object
shape, duplicate identities, and cursor bounds. It never calls `upsert` or
`delete`, writes CRM state, emits mutation events, or returns raw record data.
Adapter activation, synchronization, canary, promotion, and generated code
remain separate capabilities.

## Full-relist synchronization

When an active persisted adapter descriptor has `incremental_sync: false`, the
same governed sync capability follows the WIT `list` cursor from the beginning
to completion. BridgeHost enforces fixed page, record, byte, cursor, and
duplicate-identity bounds; Store applies the validated snapshot atomically and
returns only run/count/strategy metadata. The same bounded command is also used
by the optional owner-controlled scheduler; scheduling is disabled by default
and does not bypass Governor, Executor, BridgeHost, or Store provenance rules.
