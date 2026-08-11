# Hydra and Nexus Integration

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

The kernel constructs the Wasmtime/WIT BridgeHost, but adapter deployment, pause, resume, and sync lifecycle execution remain unavailable because no persisted lifecycle handler is registered. Host construction alone is not advertised as an executable bridge command.

Internal agent capability truth is separate from the public Nexus tool registry. DataSteward can emit an experimental governed merge proposal but cannot execute it; BridgeEngineer synthesis is unavailable; Comms draft generation is available but transport is unavailable. The kernel emits this inventory from typed agent descriptors at startup instead of treating placeholder behavior as complete.

The initial governed command is `hydra.crm.propose_action`, exposed through MCP and `POST /v1/nexus/proposals/stage-change`. Its fixed provider-neutral input accepts one canonical deal ID, target stage, rationale, idempotency key, and optional objective/task references. Both transports use the same capability dispatcher; the result is an envelope receipt, never a direct entity mutation. Availability is derived from the typed `pipeline/move_stage/deal` execution-handler registration.

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

## Modes

Standalone mode disables Nexus resource-server routes and requires no Nexus trust anchor or binding.

Nexus-connected mode enables configured issuer/audience/JWKS validation, Origin policy, external bindings, `/v1/nexus/`, MCP, and the external event contract. Failure of required identity or event infrastructure makes affected readiness fail closed.

## Deployment seam

The reference Compose topology publishes only Caddy. Kernel, Postgres, and NATS remain private and separated; a trusted Nexus event consumer may join only the named attachable event network. Kernel has no external network attachment and reaches configured HTTP providers through the dedicated egress proxy, while BridgeHost grants remain the destination-authorization boundary.

`docker/nexus.env.example` is the Nexus-connected configuration template and `NEXUS_PACKAGE_CONTRACT.md` defines the future installer inputs. The installer must provision an owner and active external business binding through a Hydra-owned authenticated setup operation. Nexus must never bootstrap itself through direct SQL, caller-supplied Hydra tenant IDs, or a startup backdoor. No such external bootstrap endpoint is currently advertised.

## Version and status

The normative v1 requirements are in `.agent/specs/SPEC-010-nexus-interoperability.md`. Current implementation truth and known gaps are in `NEXUS_INTEGRATION_AUDIT.md`. GraphQL is not part of Nexus v1.
