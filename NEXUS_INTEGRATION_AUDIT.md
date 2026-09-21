# Nexus Integration Audit

Date: 2026-08-10
Baseline commit: `f38689a`
Scope: current checked-out Hydra worktree, read-only runtime/schema/deployment inspection before EP-011 changes

## Executive finding

Hydra has strong domain primitives and several real implementation slices, but its current external seam is not safe for Nexus. CDM, Governor, Postgres repositories, outbox, WIT, Wasmtime hosting, TOKENKILLER, an LLM router, REST/MCP handlers, and a kernel relay all exist. The running kernel, however, wires a development Governor and store-backed service facades while omitting the executor, bridge host, real Hydra agents, and real TOKENKILLER/router. Tenant authority can come from caller metadata, JWT behavior is development-only, rate limiting allows every request, approvals lack strong provenance, and event publication is neither versioned nor JetStream-acknowledged.

This audit describes current behavior only. Target requirements are normative in `.agent/specs/SPEC-010-nexus-interoperability.md`.

## EP-034 Update

The bridge runtime now has an additive tenant-scoped scratch-state boundary.
Adapter IDs are not globally authoritative: Store reads and writes use
`(tenant_id, adapter_id, key)`, lifecycle probes receive the governed envelope
tenant, and the historical unscoped `adapter_kv` interface fails closed.
Synchronization remains unavailable.

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

## Post-EP-019 Current Verification (2026-08-11)

The baseline findings above remain historical evidence of the pre-Nexus
implementation. EP-012 through EP-018 corrected the authenticated control
plane, governed execution, durable events, local E2E gates, session boundary,
and runtime vault wiring. EP-019 M1-M5 are complete locally: it adds a
tenant-scoped `bridge_adapter` registry plus a Wasmtime component-root,
digest, grant, fuel, and probe boundary; registers typed deploy, pause, and
resume handlers only when `HYDRA_ADAPTERS_PATH` and the configured
SecretSource are valid; and projects Fabric status and compact context from
the durable registry. The full verifier and three fake Nexus E2E scenarios
pass against isolated Postgres and JetStream services. Sync, synthesis,
canary, promotion, staging deployment, and production readiness remain
unavailable and must not be inferred from this code-owned lifecycle slice.

## Post-EP-020 Current Verification (2026-08-11)

EP-020 adds a shared kernel readiness evaluator behind the backward-compatible
`/readyz` response and the non-secret `/readyz/details` JSON projection. The
details surface reports Postgres, NATS, required event infrastructure, and
configured bridge lifecycle checks without exposing paths, credentials,
tokens, prompts, or customer data. The backup helper now validates an archive
before atomic publication, while restore verification requires an explicit
ephemeral confirmation, refuses production environment markers, generates its
own target name, restores in one transaction, and cleans up that target.

The fake-client operational suite and isolated readiness smoke test pass
locally. This does not prove a real Postgres restore, JetStream/vault recovery,
staging deployment, D1-D5 drill, soak, or production readiness; those remain
EP-010 gates.

## Post-EP-021 Current Verification (2026-08-11)

EP-021 closes the repository-policy gap around release provenance without
claiming a release occurred. The release workflow now requests explicit
BuildKit maximum provenance and SBOM output, captures the immutable image
digest, and invokes signed `actions/attest@v4` publication with least-privilege
permissions. Because this is a private personal repository, organization-only
storage-record creation is explicitly disabled; private-repository hosted
attestation remains a tag-run capability prerequisite. A static policy checker
validates the workflow and rejects masked required paths. Nightly ignored
conformance now runs through a wrapper that preserves Cargo failure status,
requires a positive discovery count, and requires `c9_soak_10k` to pass. The
local wrapper and policy checks pass; no tag, image push, attestation, registry
verification, or deployment occurred.

## Post-EP-022 Current Verification (2026-08-11)

EP-022 closes the code-owned outbound-client contract gap identified in the
baseline audit. `HYDRA_EGRESS_PROXY_URL` is typed and required in staging and
production, and the Kernel passes it explicitly to configured model providers
and OIDC JWKS retrieval. Fabric and BridgeHost also expose proxy-aware client
constructors, while development/test constructors remain available for
deterministic local use. `scripts/check-egress-policy.sh` provides a mandatory
static contract check. This is local construction and configuration evidence
only; staging proxy ACLs, DNS/TLS, IdP reachability, and external provider
connectivity remain unexecuted.

## Post-EP-023 Current Verification (2026-08-11)

The baseline findings above remain historical evidence from the pre-Nexus
implementation. EP-023 adds a Store-owned `hydra.tenant-data.v1` export and a
read-only age-based retention preview. The local admin routes
`GET /v1/tenant/export` and `GET /v1/tenant/retention-preview` derive tenancy
from a verified local session, require the `Admin` role, use bounded
parameterized Store queries, and preserve soft-deleted records. The export
contains canonical entities, same-tenant edges, and safe event metadata; it
does not expose prompts, tokens, secrets, or raw outbox payloads.

The Store suite passed 3 tests, the Fabric boundary suite passed 1 persisted
session/loopback HTTP test, the OpenAPI contract passed, and the isolated full
verifier exited through `verify: ok` in 805.9 seconds. This does not establish
a legal retention period, purge scheduler, export delivery workflow, staging
privacy/restore evidence, or EP-010 production readiness.

## Post-EP-024 Current Verification (2026-08-11)

EP-024 closes the code-owned fixed-credential path without rewriting the
historical `0007_auth.sql` migration. Additive migration `0016_auth_seed_hardening.sql`
preserves the known `admin` row, records `auth_source=development_seed`, and
sets `disabled_at`. `SessionStore::authenticate` and `SessionStore::lookup`
reject that source in every environment, including existing sessions. The
separate `hydra-dev-admin` bearer fixture remains gated by `HYDRA_ENV=dev` and
does not enable the database seed. The historical stored hash also does not
validate the documented `hydra-dev` password, so re-enabling it would not be a
valid compatibility strategy.

The migration-backed auth suite and Kernel compile check are the local evidence
for this seam. No owner bootstrap endpoint or CLI, staging identity/TLS
evidence, recovery drill, production deployment, or human sign-off occurred;
EP-010 remains partial.

## Post-EP-025 Current Verification (2026-08-11)

The current Kernel previously delivered ExecuteTokens only through a
process-local channel. EP-025 adds a bounded Store query for tenant-qualified
`Approved` envelope identities and a supervised startup/periodic recovery scan.
The private Executor identity path reuses the normal approval and typed-handler
execution path; it does not expose a token constructor or direct mutation route.
Concurrent recovery remains safe because the existing tenant-scoped row lock
allows only one `Approved -> Executing` transition to win.

An old `Executing` row is deliberately not retried because its external outcome
could be unknown. Store-backed readiness reports `execution_recovery_required`
after 15 minutes and leaves the row unchanged for operator investigation. The
Store recovery suite passed 2 tests, the Kernel runtime-wiring suite passed 6,
and checked SQLx metadata refreshed locally. No staging crash drill, production
deployment, or human sign-off occurred; EP-010 remains partial.

## Post-EP-026 Current Verification (2026-08-11)

The production Kernel now constructs a Store-backed fixed-window limiter rather
than relying on process-local quota state. The additive `rate_limit_window`
repository uses database time and row-level upsert locking; Fabric hashes the
derived principal/network key before it reaches Store, and middleware uses the
async authority path. Exceeded windows retain 429 with bounded `Retry-After`;
authority or pruning errors return a generic 503 and never disclose SQL detail.

The isolated Store suite passed atomic concurrency, digest isolation, reset,
pruning, and validation tests. Fabric passed local 429, digest, and backend
failure tests; the Kernel binary suite passed 24 tests and the full Kernel test
target compiled. This proves the local implementation seam only. Multi-replica
staging quota behavior, outage drills, and EP-010 human sign-off remain open.

## Post-EP-027 Current Verification (2026-08-11)

The Kernel now installs request metrics middleware in the real router path and
records the existing `hydra_requests_total` and
`hydra_request_duration_seconds` families. Route labels are reduced to a
fixed taxonomy (`/`, health/readiness, metrics, MCP, A2A, `/v1/*`,
`/v1/nexus/*`, static, or `/other`); method and status labels are bounded, and
query strings are excluded. The focused binary suite passed 8 tests. The
registry remains process-local and `/metrics` remains an operational surface;
no live Prometheus scrape, dashboard, alert, staging drill, or production
deployment was performed.

## Post-EP-033 Current Verification (2026-08-12)

EP-033 connects the existing BridgeEngineer, TOKENKILLER, Kernel router, Store
ledger, and authenticated A2A task seams without changing the Wasmtime ABI or
adding a second bridge abstraction. `agents::BridgeEngineer::synthesize` now
accepts only bounded adapter metadata, uses the `bridge_mapping` MappingYaml
contract through `tokenkiller::Session`, validates adapter/entity/field safety,
and returns a normalized non-executable proposal with redacted provenance.

The Kernel constructs `BridgeSynthesisRuntime` only when a provider chain is
configured. Its runtime inventory reports Experimental in that case and
Disabled otherwise. Fabric's default service is unavailable, and the A2A
`bridge-synthesis` workflow remains authenticated, tenant-scoped, durable,
idempotent, correlation-preserving, and proposal-only. Provider or validation
failure produces a bounded failed task; it never activates an adapter, creates
an ActionEnvelope, or mutates CRM state.

Deterministic agent tests and offline compile checks pass. Service-backed A2A
and runtime assertions require the documented isolated loopback Postgres
fixture; no live provider, staging deployment, or production database was
used. EP-019 remains the activation boundary, and EP-010 remains partial.

## Post-EP-035 Current Verification (2026-08-12)

EP-035 adds one bounded governed synchronization page over the existing WIT
`changes-since` ABI. Store persists tenant/adapter/kind cursors, run leases,
and conflict metadata; complete pages atomically apply bridge-origin canonical
upserts or soft deletes with audit/event/outbox records. BridgeHost and Kernel
invoke the adapter only through the existing tenant-scoped Wasmtime grants and
typed `bridges/sync_adapter` handler. Fabric exposes the same capability through
authenticated MCP and `POST /v1/nexus/bridges/{id}/sync`; the REST adapter ID is
path-bound and no caller can provide tenant or cursor authority.

Focused Store, BridgeHost, Kernel, Fabric, MCP contract, SQLx, and workspace
checks pass against disposable loopback PostgreSQL. The plan does not add a
scheduler, full-relist fallback, mapping synthesis, canary, promotion, or
staging/production evidence. EP-010 remains partial.

## Post-EP-036 Current Verification (2026-08-12)

EP-036 makes the previously advertised `bridge-conformance` A2A workflow
truthful without adding a second bridge abstraction. BridgeHost now performs a
bounded metadata-only read contract over the configured digest-pinned
component. The Kernel resolves the active adapter, persisted grant, and
configuration through the tenant-scoped Store boundary, and Fabric exposes the
result only through the authenticated durable A2A task path.

The focused validator suite passed three malformed JSON, duplicate-identity,
and control-cursor tests; the fixture-backed lifecycle conformance test passed;
the Kernel bridge lifecycle test passed runtime-unavailable, tenant-isolation,
and no-mutation assertions; and the Fabric A2A suite passed successful and
failure/redaction/idempotency conformance scenarios. No raw adapter records,
secrets, CRM mutations, audit/outbox writes, or production deployment are
claimed by this seam. EP-010 remains partial.

## Post-EP-037 Current Verification (2026-08-12)

EP-037 implements the WIT-declared full-relist fallback without adding a
second synchronization abstraction. BridgeHost now follows bounded `list`
pages, rejects repeated cursors, duplicate identities, invalid records, and
page/record/byte overages before Store application. Store performs a complete
tenant/adapter/kind snapshot diff in one transaction: unchanged active rows
are not versioned, changed or revived rows emit canonical events, and missing
active bridge-origin rows are soft-deleted. The Kernel selects this mode only
from the persisted descriptor and returns strategy/count metadata without raw
records.

Focused BridgeHost guard tests passed 3/3, the fixture full-relist test passed,
Store diff/rollback tests passed 2/2 against disposable loopback PostgreSQL,
and the Kernel governed fallback test passed. No scheduler, provider,
production deployment, or EP-010 readiness claim is made; canary, promotion,
staging, recovery, soak, and human sign-off evidence remain open.
## Post-EP-044 Runtime Boundary Update (2026-08-12)

The EP-022 audit identified explicit proxy constructors in BridgeHost, but a
follow-up source audit found that `crates/kernel/src/runtime_services.rs`
still passed `DenyEgressClient` into `BridgeLifecycle::new` for configured
adapters. EP-044 corrected that call site: the real Kernel now constructs
`ReqwestEgressClient::new_with_proxy` from `LlmRuntimeConfig.egress_proxy_url`
and returns an unavailable runtime without registering lifecycle handlers when
construction fails. Disposable loopback tests passed bridge lifecycle `2/2`
and runtime wiring `7/7`; no external network or staging deployment was used.

This closes the code-owned runtime wiring gap, but does not prove Tinyproxy
ACLs, staging DNS/TLS, upstream CRM reachability, or EP-010 production
readiness.

## Post-EP-051 Current Verification (2026-08-12)

The prebuilt bridge activation boundary is now stricter than the historical
probe-only path. `DeployAdapterHandler` performs a bounded read-only
`BridgeLifecycle::conformance` call after probe and before the tenant-scoped
registry transition to `active`, using the stored digest, grant, and config.
The real Kernel bridge suite passes valid activation, durable conformance
failure, and invalid-runtime fail-closed cases (3/3). Conformance failure
persists `activating -> failed` and returns the executor failure path; no CRM,
adapter mutation, secret, or raw record is exposed.

This is local code evidence only. Generated adapters, autonomous canary and
promotion, provider/staging validation, recovery drills, and EP-010 human
readiness evidence remain open.

## Current Verification Status (2026-08-13)

The findings in the opening sections are preserved as the pre-Nexus baseline;
they are not a statement of the current runtime. The later verification
sections are the authoritative implementation record. A bounded repository
audit confirms the following current state:

- Kernel startup constructs the persisted Governor provider, Store-backed
  session repository, Executor and supervised worker, BridgeHost/runtime
  availability inventory, TOKENKILLER/LLM router when configured, and the
  JetStream event publisher/relay. Unconfigured or unsupported capabilities
  remain unavailable or experimental rather than being advertised as ready.
- Nexus REST, MCP, and A2A routes authenticate before capability execution.
  External tenant authority comes from the validated principal and active
  binding; local `x-hydra-tenant` compatibility is accepted only when it
  matches the verified local session and is not used by Nexus routes.
- The capability registry is shared by MCP and REST discovery. External
  mutations enter the governed envelope path; local authenticated CRUD remains
  a separate compatibility surface and is not the Nexus mutation path.
- Runtime SQL is now confined to `crates/store`, including authentication and
  session persistence plus Kernel health/migration access. Fabric and Kernel
  use typed Store repositories rather than issuing database queries.
- Outbox relay publication uses the configured JetStream stream and requires a
  publish acknowledgement before Store marks an outbox record published. The
  stable outbox event ID is used for broker deduplication and retry safety.
- A durably parked canonical outbox row is loaded into relay health at startup
  and makes required event readiness fail closed across relay restarts; a
  transient JetStream publication failure remains retryable.
- Configured OIDC JWKS requests use a five-second deadline, and LLM,
  BridgeHost, and Fabric egress requests use a bounded 30-second deadline;
  upstream stalls cannot hold those workers indefinitely.
- Provider and bridge errors now expose only stable categories/statuses;
  upstream response bodies, endpoint credentials, queries, and URL paths are
  not forwarded in external error details. Fabric also replaces raw
  TOKENKILLER/provider diagnostics with the stable `LLM provider unavailable`
  detail while preserving the error category and status code. BridgeHost guest
  logs now emit only bounded metadata and never the guest message payload.
- BridgeEngineer's historical seven-step `run` method remains explicitly
  unavailable. The separate TOKENKILLER mapping proposal path is bounded,
  authenticated, proposal-only, and cannot activate an adapter or mutate CRM
  state. This is intentional truthful capability reporting, not an omitted
  completion claim.
- The historical `/oauth/token` finding is no longer current runtime behavior:
  `crates/fabric/src/rest/oauth.rs::token_endpoint` now fails closed with a
  typed `503` capability-unavailable response and never issues or signs an
  access token. `mcp_contract_token_endpoint_is_not_an_issuer` guards this
  boundary without requiring an external identity provider.

Local evidence for this reconciliation includes `bash scripts/verify.sh`
exiting 0 with `preflight: ok`, `readiness evidence: ok`, `nats policy: ok`,
`lint: ok`, `format check: ok`, and `typecheck: ok`; the focused Store/Fabric/
Kernel compile check; `bash scripts/security-check.sh`; and
`bash scripts/check-execplan-state.sh` returning `execplan state: ok`.
These checks do not satisfy EP-010's operator-owned gates: real staging
identity/DNS/TLS and proxy paths, dated D1-D5 restore/rollback/cache/freeze
drills, a 24-hour soak, live provider and observability evidence, privacy,
security, performance and accessibility review, recovery/rollback evidence,
and human launch sign-off. No production deployment or production database
operation occurred. No additional ExecPlan is required for this
reconciliation; the existing EP-000 through EP-056 corpus has no active plan.
