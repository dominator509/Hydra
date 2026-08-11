# SPEC-010 Nexus Interoperability

Status: Accepted
Version: 1.0.0
Date: 2026-08-10
Owner: Hydra architecture

## 1. Purpose

Define the authenticated, tenant-safe, governed, versioned, and durable interoperability boundary through which Nexus uses Hydra as its CRM/revenue bounded context. This specification is normative for EP-011 through EP-015.

The key words MUST, MUST NOT, REQUIRED, SHOULD, SHOULD NOT, and MAY are normative requirements.

## 2. Ownership and invariants

Hydra MUST remain the canonical source of truth for CRM entities, graph, identity resolution, CRM bridges, synchronization, conflict policy, CRM action governance, execution, and audit. Nexus MAY store Hydra identifiers, references, or read projections, but MUST NOT maintain an independently writable duplicate CRM.

Hydra MUST remain independently deployable and functional with Nexus integration disabled.

The integration MUST preserve:

- Rust-first implementation and the six-layer import law.
- No Node/npm toolchain.
- Deterministic Governor with no LLM dependency.
- No direct execution of LLM output.
- TOKENKILLER for every Hydra-internal LLM call.
- Wasmtime/WIT as the only legacy CRM adapter ABI.
- SQL only in `crates/store`.
- Postgres audit/outbox authority and soft-delete-only behavior.
- Tenant isolation and additive migrations.

## 3. Allowed and forbidden seams

The v1 Nexus seam consists only of:

1. MCP Streamable HTTP for authenticated reads and governed proposals.
2. Versioned REST under `/v1/nexus/` for deterministic integration.
3. Durable canonical events published to NATS JetStream.

The following paths MUST NOT exist:

- Nexus to Hydra Postgres, arbitrary SQL, or store repositories.
- Nexus to raw vendor CRM APIs, bridge credentials, or unrestricted provider passthrough.
- HTTP headers, MCP metadata, tool arguments, or NATS messages as tenant authority.
- Models or agents to approval, executor, or direct state mutation.
- A generic execute-anything capability.
- Production deployment as part of EP-011 through EP-015.

GraphQL is not required for v1 and MUST NOT be claimed as implemented without a separate accepted specification and ExecPlan.

## 4. Generic internal types

Nexus claim and protocol translation MUST remain in Fabric. L1/L2 MUST use provider-neutral types.

### 4.1 Principal type

`PrincipalType` MUST distinguish at least:

- `Human`
- `NexusService`
- `NexusAgent`
- `HydraInternalAgent`
- `LocalHydraUser`

### 4.2 Principal context

An authenticated `PrincipalContext` MUST contain:

- stable `principal_id`
- `principal_type`
- resolved Hydra tenant ID
- external tenant and business IDs when applicable
- granted scopes/capabilities
- `delegated_by` when applicable
- authentication strength when applicable
- token identifier when present
- request and correlation context
- binding ID when externally bound

It MUST NOT contain the raw access token. It MUST be constructed only after token validation and binding resolution.

### 4.3 External tenant binding

Hydra MUST persist an additive `ExternalTenantBinding` equivalent to:

```text
id
provider
external_tenant_id
external_business_id
hydra_tenant_id
status
created_at
updated_at
```

The tuple `(provider, external_tenant_id, external_business_id)` MUST be unique. In v1, an active tuple MUST resolve to exactly one Hydra tenant. Bindings MUST support active, disabled, and revoked semantics. Hydra tenant IDs MUST remain the internal authority and MUST NOT be replaced by external IDs.

### 4.4 Invocation context

Externally proposed ActionEnvelopes MUST support a backward-compatible `InvocationContext` with serde defaults. It MAY contain:

- `request_id`
- `correlation_id`
- `causation_id`
- `origin_system`
- `external_actor_id`
- `external_actor_type`
- `objective_id`
- `task_id`
- `approval_id`
- `idempotency_key`

It MUST NOT contain access tokens, secrets, prompts, raw email bodies, full customer documents, or other unnecessary PII. Distributed trace context MUST remain separate unless a specific durable business reference is intentionally persisted.

## 5. Authentication

When Nexus integration is enabled, Hydra MUST act as an OAuth/OIDC resource server for short-lived asymmetric JWT access tokens issued by the configured Nexus issuer or identity provider.

Validation MUST include:

- allowed signing algorithm and signature
- issuer
- audience or resource
- expiration
- not-before when present
- token identifier when present
- required scopes
- principal type
- external tenant/business claims needed for binding
- optional authentication-strength claim when a capability requires it
- configured revocation strategy where supported

Configuration MUST support:

- issuer URL
- expected audience/resource
- JWKS URL or pinned public-key file for private/offline deployments
- JWKS cache duration
- allowed MCP Origins
- allowed clock skew
- Nexus integration enabled/disabled

No hard-coded signing key is permitted outside deterministic tests. Key lookup SHOULD be cached and MUST NOT require a Nexus network call for every request. Unknown key IDs, algorithms, issuers, audiences, expired/not-yet-valid tokens, malformed claims, and unavailable required trust configuration MUST fail closed.

Hydra SHOULD expose OAuth protected-resource metadata and MUST return standards-appropriate `WWW-Authenticate` information for protected-resource failures without leaking tenant existence.

## 6. Tenant and business authorization

Authentication MUST occur before tenant resolution. Fabric MUST resolve the Hydra tenant from verified external claims plus an active Hydra-owned binding. Caller-supplied `x-hydra-tenant`, MCP `_meta`, tool arguments, query parameters, or body fields MUST NOT override that resolution.

Missing, disabled, revoked, ambiguous, or cross-business bindings MUST return an authorization failure without revealing whether another tenant or business exists.

Standalone local Hydra authorization MAY use Hydra sessions and roles, but local and external paths MUST converge on one authorization service before capability execution.

Stable scopes MUST include, at minimum:

- `hydra.capabilities.read`
- `hydra.crm.read`
- `hydra.crm.context.read`
- `hydra.crm.propose`
- `hydra.envelopes.read`
- `hydra.envelopes.approve`
- `hydra.bridges.read`
- `hydra.bridges.admin`
- `hydra.autonomy.read`
- `hydra.autonomy.admin`

Authorization MUST evaluate principal type, scopes, Hydra tenant, binding status, delegation, and authentication strength where applicable.

## 7. Dual-gate mutation rule

Nexus authorization establishes that a principal MAY request a capability. Hydra Governor independently decides whether the CRM action is Blocked, SuggestOnly, Queued, or Executed under the tenant's current policy. Both gates MUST allow execution.

Every Nexus/MCP mutation MUST follow:

```text
authenticated PrincipalContext
  -> canonical capability
  -> schema-validated input
  -> ActionEnvelope proposal
  -> current tenant Governor evaluation
  -> Block | Suggest | Queue | Execute
  -> typed execution handler when allowed
  -> verification
  -> durable receipt, audit, outbox, and event
```

External mutations MUST NOT call `EntityService::create`, `patch`, or `delete` directly. Existing intentionally trusted local-human CRUD MAY remain only where current product specifications authorize it.

## 8. Capability registry

Hydra MUST have one typed capability registry used by MCP `tools/list`, `GET /v1/nexus/capabilities`, execution-handler discovery, documentation/schema generation, and contract tests.

Each `CapabilityDescriptor` MUST include:

- stable capability name and version
- category: `Query`, `Command`, or `Workflow`
- description
- input JSON Schema
- output JSON Schema
- required scopes
- risk class
- reversal semantics
- idempotency semantics
- Governor domain/action/kind binding where applicable
- synchronous/asynchronous behavior
- availability and unavailable reason
- required bridge/provider capability where applicable

Definitions MUST NOT be duplicated in endpoint-local switch statements. Advertised availability MUST reflect real runtime handler/service wiring.

The initial canonical MCP capability names are:

- `hydra.crm.search`
- `hydra.crm.get`
- `hydra.crm.context`
- `hydra.crm.timeline`
- `hydra.crm.pipeline_summary`
- `hydra.crm.propose_action`
- `hydra.envelopes.list`
- `hydra.envelopes.get`
- `hydra.capabilities.list`

Safe old names MAY remain as deprecated aliases only if they use the same authenticated implementation and schemas.

Hydra MUST NOT advertise arbitrary SQL, shell, provider passthrough, raw vendor endpoints, secrets, arbitrary ActionEnvelope construction, direct mutations, or unrestricted approval.

## 9. MCP transport and behavior

Hydra MUST target MCP protocol version `2025-11-25` with protocol-version negotiation. It MUST implement Streamable HTTP POST and required GET behavior, authentication before dispatch, Origin validation, request-size limits, deterministic tool listing, cancellation where applicable, typed JSON-RPC errors, and secret-safe responses.

Tool declarations MUST include input schemas and output schemas. Successful tool results MUST provide `structuredContent` conforming to the declared output schema. Reads MUST return canonical typed projections. Mutating tools MUST return ActionEnvelope or receipt data rather than fabricated success text.

No tenant authority may be read from MCP `_meta` or arguments. Search MUST apply the supplied query. Protocol and compatibility tests MUST use deterministic snapshots.

The official Rust MCP SDK MUST be evaluated before extending the custom implementation. Adoption requires acceptable license/audit results, stable protocol support, Axum fit, and compliance with the layer law. If rejected, the decision and equivalent conformance coverage MUST be recorded.

## 10. REST Nexus facade

The versioned facade MUST be under `/v1/nexus/`. Initial deterministic endpoints are:

- `GET /v1/nexus/capabilities`
- `GET /v1/nexus/context`
- `GET /v1/nexus/bindings`
- `GET /v1/nexus/events/status`

Mutation endpoints MUST be added only for implemented governed command capabilities. The compact context response SHOULD include business/tenant identity, pipeline summary, hot/stalled deals, overdue activities, unanswered leads if represented, recent important CRM events, pending approval count, bridge health, and capability availability. It MUST NOT dump all CRM records into an LLM context.

Existing safe `/v1/` compatibility SHOULD be preserved. A security correction MAY reject behavior that previously trusted caller metadata; such corrections MUST be documented as migration/deprecation behavior.

## 11. Approval security

General-purpose approval MUST NOT be exposed to ordinary service or agent principals. An agent MAY request approval but MUST NOT grant it.

Approval requires:

- a human-delegated principal
- explicit `hydra.envelopes.approve` scope
- configured sufficient authentication strength
- matching active Hydra tenant/binding
- a matching pending envelope
- a proposer distinct from the approver
- an immutable approval reference and audit record

Hydra MUST persist an immutable approval assertion containing approval ID, human actor, authentication strength, time, envelope ID, request/objective correlation, decision, and optional comment. The executor MUST verify required approval is present and valid before execution.

## 12. Idempotency

Hydra MUST persist idempotency records scoped at minimum by Hydra tenant, origin system, idempotency key, and capability/action. The record MUST include a canonical request hash and resulting envelope/receipt reference.

An equivalent retry MUST return the existing envelope or receipt. Reuse of the same key with a different request hash MUST return a deterministic conflict. Storage and envelope creation MUST be atomic enough to prevent concurrent duplicate actions.

## 13. Execution registry and runtime

Execution MUST use a typed registry. A handler declares domain, action, optional kind, payload schema, target requirements, risk/reversal expectations, execute behavior, verification behavior, optional compensation, and capability link.

The registry MUST reject duplicate registrations at boot, reject unsupported approved envelopes, expose availability through capabilities, allow isolated tests, preserve the layer law, prevent model invocation, and prevent arbitrary credential access.

When configured, the kernel MUST construct and supervise the persisted tenant-aware Governor provider, envelope service, execution registry, executor, BridgeHost and lifecycle workers, outbox relay, real TOKENKILLER/LLM router, and real internal agents. Development/demo providers MUST NOT be used in non-test staging/production paths.

Tenant autonomy changes MUST affect later evaluations through a safe cache/invalidation strategy. Envelope transitions MUST be tenant-scoped, concurrency-safe, state-machine-valid, append history, preserve actor/invocation context, and atomically emit audit/outbox records.

Bridge and agent capabilities MUST report unavailable or experimental when implementation/runtime wiring is absent. DataSteward merge MUST propose governed work; BridgeEngineer and Comms MUST describe actual capabilities.

## 14. Canonical event contract

Hydra MUST publish a CloudEvents-compatible or equivalently rigorous envelope containing:

- `event_id`
- `spec_version`
- `event_type`
- `schema_version`
- `source`
- `subject`
- `occurred_at`
- optional `observed_at`
- Hydra tenant ID
- optional external binding ID
- actor reference
- optional correlation ID
- optional causation ID
- optional ActionEnvelope ID
- optional entity reference
- `data_class`
- typed payload

Stable semantic v1 names include:

- `hydra.crm.entity.created.v1`
- `hydra.crm.entity.updated.v1`
- `hydra.crm.entity.deleted.v1`
- `hydra.crm.envelope.proposed.v1`
- `hydra.crm.envelope.queued.v1`
- `hydra.crm.envelope.approved.v1`
- `hydra.crm.envelope.executed.v1`
- `hydra.crm.envelope.failed.v1`
- `hydra.crm.bridge.health_changed.v1`
- `hydra.crm.sync.conflict.v1`

NATS subjects MUST NOT contain PII. Events MUST NOT contain access tokens or secrets. Event fixtures MUST be schema validated and scanned for token/secret-shaped values.

## 15. JetStream durability

The application MUST create or verify the required JetStream stream at startup. Retention and ordering expectations MUST be documented. The relay MUST receive a JetStream publish acknowledgement before marking an outbox row published. Publish failure MUST leave the row unpublished and safely retryable.

The logical `event_id` MUST be stable for an outbox record so consumers can deduplicate retries. Unrecoverable serialization failures MUST be parked or dead-lettered with safe diagnostics. Postgres remains authoritative; JetStream is transport/replay, not source of truth.

Required event infrastructure health MUST contribute to readiness in Nexus-connected mode. Nexus MUST be able to create a durable consumer without database access.

## 16. Trace propagation

Hydra MUST propagate W3C trace context across Nexus request -> Fabric -> Governor -> Store -> Executor -> Bridge adapter -> event relay. OpenTelemetry-compatible APIs SHOULD be used if dependency/license review passes. Trace baggage MUST NOT contain PII, secrets, prompts, or tokens.

Durable correlation/causation IDs are business provenance and MUST survive where specified even when an in-memory trace ends. Redacted structured JSON logging remains the local fallback.

## 17. Versioning and backward compatibility

- MCP protocol negotiation MUST reject unsupported required versions with a typed error.
- REST remains under `/v1`; breaking REST changes require a new major namespace.
- Capability names and versions are stable; breaking schema changes require a new capability version.
- Additive event changes MAY remain in a schema version when consumers remain compatible. Breaking event changes require a new event/schema version.
- Old event consumers MUST receive a documented compatibility window.
- Stored pre-invocation-context envelopes MUST continue to deserialize through serde defaults.
- Deprecated MCP aliases MUST be documented and tested until removal in a later major contract.

Security fixes that remove caller-selected tenant behavior are intentionally fail-closed and MAY be backward incompatible. Migration guidance MUST identify the replacement authenticated binding path.

## 18. Failure modes

Hydra MUST fail closed for unknown/missing identity configuration, invalid tokens, unknown scopes, absent/disabled bindings, tenant mismatch, unknown capabilities, invalid schemas, duplicate handler registration, unsupported handlers, missing approval, conflicting idempotency keys, illegal transitions, and unavailable required event infrastructure.

Failures MUST use stable typed codes and MUST NOT reveal secrets, token contents, customer records, existence of another tenant, upstream response bodies containing sensitive data, or internal SQL.

NATS messages MUST NOT mutate state. Event consumer failure MUST not roll back authoritative Postgres state. Provider or bridge unavailability MUST be represented in capability/health truth rather than fabricated success.

## 19. Deployment

Hydra MUST support:

- standalone mode with Nexus integration disabled
- Nexus-connected mode with configured trust anchors, binding bootstrap, MCP/REST facade, and external event stream

The Compose topology MUST separate Caddy ingress, kernel/backend, internal Postgres, internal NATS, and narrowly external-capable egress-proxy networks. Kernel MUST NOT receive unrestricted direct internet access. NATS client and monitoring ports MUST NOT be public in production by default. A shared/external NATS option MAY be configured explicitly.

Deployment documentation MUST define image/version, required and optional environment variables, volumes, migrations, health/readiness, owner/binding bootstrap, trust-anchor setup, backup/restore, rollback, and supported upgrades. EP-011 through EP-015 MUST NOT deploy production.

## 20. Test acceptance

Deterministic local fixtures MUST provide issuer/JWKS keys and fake Nexus service, agent, and human-delegated principals. Tests MUST NOT require a live Nexus repository or cloud identity service.

At minimum tests MUST prove:

- anonymous, expired, wrong-issuer, wrong-audience, and invalid-signature tokens are rejected
- caller tenant metadata/header cannot change the bound Hydra tenant
- disabled and cross-business bindings fail closed
- read-only principal cannot propose
- agent cannot approve; human cannot approve own proposal
- valid service principal reads only its bound tenant
- MCP Origin and protocol-version policy
- stable tool input/output schemas and valid `structuredContent`
- search applies its query
- idempotent retry returns one action and conflicting reuse fails
- unsupported/duplicate handlers fail closed
- low-autonomy queues and allowed high-autonomy executes
- required approval is enforced and immutable
- policy changes affect later decisions
- envelope lookup and transitions are tenant-safe
- canonical event schema, stable event ID, correlation/causation, ack-before-mark, retry, replay deduplication, and durable consumer resume
- event readiness fails when required infrastructure is down
- fake Nexus capability/context/search/propose/approve/execute/event round trip
- cross-business E2E access is blocked

Required scripts MUST fail when required tests are absent or fail. Required CI steps MUST NOT use `|| true` or `continue-on-error`. Informational jobs MUST be separate and clearly non-gating.

EP-015 acceptance requires exact success signals from preflight, unit, integration, E2E, security, dependency audit, and verify scripts; Docker image build and Compose config; and the fake Nexus round trip. These results establish the interoperability test boundary only. They MUST NOT mark EP-010 production readiness complete without real staging drills, soak, security/accessibility/performance/restore/rollback evidence, and human sign-off.
