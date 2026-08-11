# Nexus Integration Audit

Date: 2026-08-10
Baseline commit: `f38689a`
Scope: current checked-out Hydra worktree, read-only runtime/schema/deployment inspection before EP-011 changes

## Executive finding

Hydra has strong domain primitives and several real implementation slices, but its current external seam is not safe for Nexus. CDM, Governor, Postgres repositories, outbox, WIT, Wasmtime hosting, TOKENKILLER, an LLM router, REST/MCP handlers, and a kernel relay all exist. The running kernel, however, wires a development Governor and store-backed service facades while omitting the executor, bridge host, real Hydra agents, and real TOKENKILLER/router. Tenant authority can come from caller metadata, JWT behavior is development-only, rate limiting allows every request, approvals lack strong provenance, and event publication is neither versioned nor JetStream-acknowledged.

This audit describes current behavior only. Target requirements are normative in `.agent/specs/SPEC-010-nexus-interoperability.md`.

## Actual runtime topology

`crates/kernel/src/main.rs::run` currently:

1. Loads `Config`, connects a Postgres pool, runs migrations, and connects a plain async-NATS client.
2. Constructs store-backed envelope, entity, autonomy, bridge-control, and TK-ledger service objects.
3. Constructs `ConciergeServiceImpl` with `PingRouter` and an in-memory ledger.
4. Uses `services::demo_governor()` for envelope and bridge services.
5. Spawns `relay::run`, whose `publish_once` reads outbox rows, publishes with the core NATS client, flushes, then marks rows published.
6. Serves Fabric routes plus `/healthz`, `/readyz`, `/metrics`, and static assets.

The runtime does not import or construct `kernel::executor::Executor`, `bridge_host::BridgeHost`, `agents::BridgeEngineer`, `agents::DataSteward`, `tokenkiller::Session` with `StoreLedgerSink`, or `llm_router::Router`. `crates/kernel/src/executor.rs` is compiled only by its path-based integration test, not by the binary module tree.

## Actual initialized services

| Service | Actual implementation | Runtime truth |
|---|---|---|
| Entity API | `StoreEntityService` | Real Postgres CRUD, including direct external writes |
| Envelope API | `StoreEnvelopeService` | Real storage with a boot-time demo Governor; approval provenance is weak |
| Autonomy API | `StoreAutonomyService` | Persists cells, but the already-built service Governor is not refreshed |
| Bridge control API | `StoreBridgeService` | Envelope/status facade only; no runtime BridgeHost lifecycle execution |
| TK ledger API | `StoreTkService` | Reads real ledger rows |
| Concierge | `ConciergeServiceImpl<PingRouter, InMemoryLedger>` | Exercises TOKENKILLER shape with an echo router, not a configured LLM provider |
| Outbox relay | `kernel::relay::run` | Plain NATS publish; not a JetStream producer contract |
| Executor | Source/test seam only | Not constructed or supervised by the binary |
| Agents | Library-only | Not constructed or supervised by the binary |

## Current public routes

Fabric declares these routes in `crates/fabric/src/rest/mod.rs::router`:

- `GET /v1/openapi.json`
- `POST /mcp`
- `GET|PUT /v1/autonomy/cells`
- `POST /v1/bridges`
- `GET /v1/bridges/:id/status`
- `POST /v1/bridges/:id/pause`
- `POST /v1/bridges/:id/resume`
- `POST /v1/concierge/ping`
- `GET|POST /v1/entities/:kind`
- `GET|PATCH|DELETE /v1/entities/:kind/:id`
- `GET|POST /v1/envelopes`
- `POST /v1/envelopes/:id/approve`
- `POST /v1/envelopes/:id/reject`
- `GET /v1/tk/ledger`
- `POST /oauth/token`

The kernel also exposes `GET /healthz`, `GET /readyz`, `GET /metrics`, and static routes. No `/v1/nexus/` facade exists. No GraphQL handler or route exists.

## Current MCP surface

`crates/fabric/src/mcp.rs::McpServer` implements only `initialize`, `tools/list`, and `tools/call` over one POST route. It reports protocol version `1.0`. Current tools are:

- `hydra.search_entities`
- `hydra.get_entity`
- `hydra.propose_envelope`
- `hydra.list_pending`
- `hydra.approve`
- `hydra.pipeline_stats`
- `hydra.tk_stats`

`extract_tenant` accepts `arguments._meta.x-hydra-tenant` and otherwise returns a fixed development tenant. `mcp_route` has no authenticated principal, Origin policy, request protocol-version negotiation, or GET transport support. Tool results contain text content rather than declared `structuredContent` plus output schemas. `call_search_entities` parses but does not apply `query`. `call_approve` fabricates `AuthCtx { principal: "anonymous", session: None }`.

## Current authentication and authorization paths

### REST tenant authority

`crates/fabric/src/services.rs::tenant_from_headers` parses `x-hydra-tenant` directly. Entity CRUD, envelopes, autonomy, and bridge handlers use that value. The entity paths permit unauthenticated direct CRUD. `auth_ctx_from_headers` falls back to a fixed development tenant and constructs a synthetic session.

### Bearer and local auth

`crates/fabric/src/auth/jwt.rs` implements a custom HS256 token format. It does not validate OIDC issuer, audience, expiration, not-before, token identifier, asymmetric signature, or JWKS. In `auth_ctx_from_headers`, an unrecognized bearer value is accepted as a Viewer principal. The local role check does not bind the session tenant to the requested tenant; `crates/fabric/tests/authz_endpoints.rs` currently asserts that a mismatched tenant can still pass.

### Token endpoint

`crates/fabric/src/rest/oauth.rs::token_endpoint` ignores a complete OAuth grant flow, issues for a fixed tenant UUID, and signs with a hard-coded HMAC key. This is development behavior, not an OAuth/OIDC authorization server or resource-server trust path.

### Approval

REST approval derives an `AuthCtx` through the development parser. MCP approval uses an anonymous context. Proposal history does not durably identify a strongly authenticated proposer, so the four-eyes comparison cannot prove distinct human actors. No immutable approval assertion table exists.

### Rate limiting

`crates/fabric/src/rate.rs::rate_limit_middleware` always calls `next.run(request)` and returns success. Configuration objects exist, but no request is limited.

## Current executor and governance behavior

`crates/kernel/src/executor.rs::Executor::apply` supports only `domain="pipeline"`, `action="move_stage"`, and target kind `deal`. Unsupported envelopes fail, but the executor is not in the running kernel path. It loads envelopes through `EnvelopesRepo::get_by_id`, which is not tenant-scoped.

`StoreEnvelopeService` captures one `Governor` at construction. Later autonomy-cell writes therefore do not alter its policy. Envelope approval/rejection lists pending rows, mutates an in-memory envelope, and saves the full document rather than using a concurrency-safe, tenant-scoped transition operation. Envelope transitions do not write event/outbox rows.

## Current bridge and agent behavior

`crates/bridge-host` contains a real Wasmtime component host, WIT bindings, grants, secret-name checks, KV, fuel, egress, optional SQL, and conformance tests. The kernel does not construct it. Bridge REST operations expose envelope/status semantics but do not invoke a live bridge lifecycle worker.

`agents::BridgeEngineer::run` always reaches `LoopStep::Synthesize` and returns `AgentError::SynthesisNotImplemented`. `agents::DataSteward::merge` directly returns a consolidated `Entity` rather than a governed ActionEnvelope proposal. `agents::Comms` only drafts strings and explicitly has no transport. None is runtime-wired.

## Current event and outbox contract

`migrations/0002_event_log_and_outbox.sql` provides append-only `event_log` rows and an `outbox(id,event,published_at)`. Entity/autonomy repositories build ad hoc JSON payloads. Events do not have a canonical interoperability envelope with event/schema versions, source, subject, observed time, binding, actor reference, correlation, causation, envelope/entity references, or data classification.

`crates/kernel/src/relay.rs::publish_once` publishes each row to `hydra.events.<tenant_id>` using core NATS, calls `flush`, and then marks the row published. It does not create/verify a JetStream stream or wait for a JetStream publish acknowledgement. The database transaction remains open across network publication. Subject names are coarse and include a tenant identifier.

## Current persistence gaps

- No external tenant/business binding table or repository exists in migrations `0001` through `0007`.
- No idempotency table/repository exists.
- No immutable approval assertion exists.
- `ActionEnvelope` in `crates/governor/src/envelope.rs` has no first-class invocation context.
- `EnvelopesRepo::get_by_id` is not tenant-scoped.
- Envelope `save` can update `tenant_id` on primary-key conflict.
- Envelope state changes do not atomically append canonical audit/outbox events.

## Current test-gate truthfulness

- `scripts/test-e2e.sh` prints `e2e tests: ok` when no `fn e2e_` exists.
- `scripts/test-integration.sh` runs selected bridge failure tests with `|| true` and then prints `failure suites: ok`.
- `.github/workflows/nightly.yml` marks workspace verification and cache audit `continue-on-error: true`; ignored conformance runs with `|| true`.
- EP-007's required three consecutive green verifies are not recorded in its empty Outcomes.
- The 2026-08-10 `bash scripts/verify.sh` baseline failed in lint before all later gates. No later command from that run is counted as passed.

## Current deployment topology

`docker/compose.yaml` places kernel, Postgres, NATS, egress proxy, and migration service on `backnet-internal`, which is `internal: true`. The egress proxy has no second external-capable network, so its claimed internet route is not established. Kernel and NATS client/monitoring ports are published to the host by default. Caddy and kernel share `frontnet`; the kernel is also directly published, bypassing the intended Caddy-only ingress. NATS runs with JetStream enabled, but the application uses only core publish APIs.

`/readyz` checks Postgres and NATS reachability only. It does not prove required Nexus auth configuration, external binding storage, event stream state, relay health, BridgeHost state, or configured LLM/TOKENKILLER runtime.

## Plan-vs-code reconciliation

- EP-005 is incomplete at M5 and has no real E2E gate.
- EP-006 completion exceeds implementation: JWT/OAuth, tenant binding, rate limiting, and authorization are development-grade.
- EP-007 completion exceeds evidence: masked suites, non-gating nightly, placeholder agents, and empty Outcomes.
- EP-008 completion exceeds implementation: advertised metric hooks and optional monitoring topology are not fully wired.
- EP-009 completion exceeds safe topology: egress and port exposure are incorrect for the documented boundary.
- EP-010 explicitly remains partial; its security review text incorrectly treats several planned controls as implemented.

The authoritative row-by-row state is `.agent/state/execplan-index.md`. Historical checkboxes remain unchanged.

## Nexus integration risks

| Priority | Risk | Consequence | Owning remediation |
|---|---|---|---|
| P0 | Caller-controlled tenant authority | Cross-tenant reads/writes and confused-deputy access | EP-012 |
| P0 | Development JWT/token path | Forged, expired, wrong-issuer, or wrong-audience tokens accepted | EP-012 |
| P0 | Agent/anonymous approval path | Model or unauthenticated actor can appear to approve | EP-012, EP-013 |
| P0 | External direct CRUD | Nexus mutation can bypass envelopes/Governor | EP-013 |
| P0 | Unwired executor/policy refresh | Approved work is not reliably executed; policy updates are stale | EP-013 |
| P0 | Non-tenant-scoped envelope lookup | Cross-tenant execution risk | EP-013 |
| P0 | Core-NATS relay marks after flush | No durable external delivery proof | EP-014 |
| P1 | No business binding | Nexus identity cannot map safely to Hydra tenancy | EP-012 |
| P1 | No idempotency/provenance | Retries duplicate work and actions lack attribution | EP-013 |
| P1 | Ad hoc event schemas | Consumers cannot safely version, replay, or correlate | EP-014 |
| P1 | False-green local/CI gates | Regressions can ship despite claimed validation | EP-015 |
| P1 | Broken egress topology/public ports | Provider traffic fails or services bypass intended boundaries | EP-015 |
| P2 | Placeholder agent capability claims | Nexus may plan work Hydra cannot perform | EP-013 |
| P2 | GraphQL architecture claim | Consumers may depend on a nonexistent interface | EP-011 (corrected) |

## Prioritized remediation map

1. EP-011: establish truthful history, bounded-context ownership, SPEC-010, ADR-0019, and the active-plan ledger.
2. EP-012: authenticate first, resolve tenant only from verified claims plus Hydra binding, centralize scopes, expose a schema-backed capability registry, and implement conformant MCP/REST reads/proposals.
3. EP-013: add invocation/idempotency/approval persistence, typed handlers, tenant-safe transitions, policy refresh, and actual kernel wiring.
4. EP-014: define canonical events, transact transition/outbox records, require JetStream acknowledgements, add replay/deduplication and trace propagation.
5. EP-015: prove a fake Nexus round trip, remove false-green gates, fix Compose boundaries, validate standalone and Nexus-connected profiles, and keep production readiness partial.

## Baseline validation

- `bash scripts/preflight.sh` -> `preflight: ok` (plus local `.env` note).
- `bash scripts/verify.sh` -> failed in `lint.sh` on pre-existing denied warnings. See `.agent/state/execplan-index.md` for exact file set. This audit does not relabel unexecuted later gates as passed.
