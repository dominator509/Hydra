# ARCHITECTURE.md — Boundaries and Invariants (HYDRA)

## Purpose
Define concrete component boundaries, dependency rules, flows, and invariants so agents modify HYDRA without violating its safety or token-economics model.

## System overview
HYDRA is the CRM and revenue bounded context: a canonical CDM entity graph in Postgres, deterministic Autonomy Governor, Wasmtime/WIT bridge runtime, TOKENKILLER-governed internal LLM seam, integration Fabric, event/outbox spine, and independently deployable server-rendered Shell. Some components are target architecture rather than currently wired runtime; `NEXUS_INTEGRATION_AUDIT.md` records the verified implementation state.

## Hydra and Nexus bounded contexts
Nexus is the larger whole-life and executive AI control plane. Nexus owns global identity and context, objectives and cross-domain orchestration, global model and agent routing, global memory, user communication, and Nexus-level approvals.

Hydra owns canonical CRM data, CRM identity resolution, CRM bridges and legacy wrappers, CRM synchronization, CRM conflict resolution, CRM action governance, CRM audit, and CRM execution. Nexus stores references or read projections to Hydra entities; it does not maintain an independently writable duplicate CRM source of truth.

The v1 interoperability seam is narrow and versioned:

- MCP Streamable HTTP serves authenticated AI/agent reads and governed proposals.
- `/v1/nexus/` REST endpoints serve deterministic system integration.
- NATS JetStream carries durable canonical Hydra events for Nexus consumers.
- Nexus never accesses Hydra Postgres, vendor CRM APIs, bridge secrets, arbitrary SQL, or unrestricted provider passthrough.
- Hydra remains independently deployable and usable when Nexus integration is disabled.

Nexus concepts are translated in Fabric into generic internal types such as `PrincipalContext`, `ExternalTenantBinding`, and `InvocationContext`; Nexus-specific protocol concerns do not spread into L1/L2.

Optional Agent Skills remain an L3 declarative discovery concern. `agents::skills::SkillRegistry` verifies upstream `SKILL.md` metadata plus a Hydra-local Ed25519 manifest against owner-controlled trust anchors; it exposes metadata only and never executes package scripts, grants credentials, or changes capability/Governor policy.

### Dual-gate mutation rule
Nexus authorization establishes that an authenticated actor may request an action. Hydra Governor independently establishes whether that CRM action may execute. Both gates must allow execution. A model, agent, HTTP header, MCP metadata field, or NATS message can never approve or directly perform a store mutation.

### Tenant and business binding
In v1, one Nexus business maps explicitly to one Hydra tenant through a binding stored by Hydra. External tenant and business IDs never replace Hydra tenant IDs, callers cannot self-select or override the mapped Hydra tenant, disabled or revoked bindings fail closed, and standalone Hydra tenants need no Nexus binding.

## The 6-Layer Paradigm (normative)
| Layer | Name | Crates / dirs | May import |
|---|---|---|---|
| L1 | Domain | `crates/cdm`, `crates/governor` | std only (+serde, uuid, thiserror) |
| L2 | Persistence | `crates/store` (sqlx repos, migrations/) | L1 |
| L3 | Services | `crates/bridge-host`, `crates/llm-router`, `crates/tokenkiller`, `crates/agents`, `crates/fabric` | L1, L2 |
| L4 | Interface | `crates/shell` (Askama+htmx), REST/MCP handlers in `crates/fabric`; GraphQL deferred | L1–L3 |
| L5 | Agentic policy | agent prompts/, autonomy matrix config, constitution | consumed by L3; contains no Rust importable by L1/L2 |
| L6 | Operations | scripts/, docker/, dashboards/, runbooks | none (drives the others) |

Dependency rule (hard): Ln may import only L≤n as listed. L1 imports nothing from L2+. `crates/governor` MUST NOT depend on `llm-router` or any network crate — the Governor is deterministic.

Concrete import rules:
- `cdm` may not import `store`. `store` may import `cdm`. `shell` may not import `store` directly — it calls `fabric` service traits.
- `tokenkiller` may not import `agents` (agents call TK, never the reverse).
- `bridge-host` is the ONLY crate linking wasmtime. No other crate may.
- Only `llm-router` performs LLM HTTP; only `bridge-host`+`fabric` perform other egress; both via the egress proxy client in `crates/fabric::egress`.

## Repository map (intended)
```
/                    Cargo.toml (workspace), rust-toolchain.toml, deny.toml
crates/kernel/       bin `hydra`: wiring, config, startup, NATS consumers
crates/cdm/          entity kinds, JSON Schema registry, identity resolution (pure)
crates/governor/     ActionEnvelope, Level, PolicyMatrix, Constitution, Decision
crates/store/        sqlx repositories, migrations/, event append, outbox
crates/bridge-host/  Wasmtime engine, GrantTable, host impls, conformance/
crates/bridge-wit/   wit/ contract + generated bindings (wit-bindgen)
crates/llm-router/   Provider trait, Anthropic/DeepSeek/OpenAI-compat impls, routes
crates/tokenkiller/  canon.rs, prefix.rs, nukeguard.rs, ledger.rs, contracts.rs
crates/agents/       Concierge, DataSteward, PipelineOp, Comms, BridgeEngineer, Auditor
crates/fabric/       REST v1, MCP server, auth, webhooks, email, egress; GraphQL deferred
crates/shell/        Askama templates, htmx views, static/ (vendored htmx)
wit/                 hydra-bridge.wit (source of truth for the bridge ABI)
adapters/            built+signed .wasm adapters (runtime-loaded)
wiring/              *.map.yaml field-wiring files per adapter
reference/           INFORMATIVE reference implementations (copy-adapt targets)
migrations/          sqlx migrations
docker/              compose.yaml, Dockerfile, Caddyfile
scripts/             the only allowed commands (see COMMANDS.md)
```

## Runtime / request flow
Required target flow: Shell/REST/MCP request -> `fabric` handler -> service trait -> (`store` for canonical reads) or ActionEnvelope proposal -> `governor.evaluate()` -> Execute -> typed kernel handler -> `store` mutation plus event/outbox, or Queue -> approval queue, SuggestOnly, or Block. Postgres audit/outbox is authoritative; JetStream publication is acknowledged before an outbox row is marked published. The pre-EP-011 runtime does not fully wire this target; see `NEXUS_INTEGRATION_AUDIT.md`.

GraphQL is not implemented and is not required for Nexus v1. It remains a possible later interface only after a separate accepted specification and ExecPlan.

### Current runtime capability truth
- `pipeline/move_stage/deal` is the only registered CRM execution handler. It performs and verifies governed stage changes.
- The Wasmtime/WIT `BridgeHost` is constructed at kernel boot, and the configured age-encrypted vault is loaded into its read-only `SecretSource` boundary. When `HYDRA_ADAPTERS_PATH` and the SecretSource are valid, typed governed handlers expose prebuilt adapter deployment, pause, resume, and manual synchronization. A new deployment must pass the bounded read-only BridgeHost conformance contract after probe and before the registry enters `active`; failures persist a tenant-scoped `failed` state. Synchronization selects the persisted descriptor's incremental feed or bounded full-relist fallback; read-only conformance is separately exposed through authenticated A2A. Bounded mapping-proposal synthesis remains an experimental TOKENKILLER-backed A2A capability when a provider chain is configured; generated code, autonomous canary, and promotion remain unavailable.
- DataSteward merge is experimental and produces an `ActionEnvelope` proposal only; no merge execution handler is registered.
- BridgeEngineer mapping proposals are experimental only when the Kernel has a configured `bridge_mapping` TOKENKILLER route and remain non-executable; Comms can draft text but has no delivery transport.
- The TOKENKILLER concierge path is available only when a real LLM provider is configured. Otherwise the kernel reports it disabled; fake routers are test-only.
- Signed skill discovery is disabled when its two optional paths are absent, available only when at least one package verifies, and unavailable when configured but no package passes trust validation. The runtime inventory never treats an invalid or untrusted package as executable.

## Data flow (bridge)
Adapter `changes-since` (host-polled) → raw-record JSON → wiring transform pipeline (fixed library) → CDM upsert as `origin=bridge:<id>` → identity resolution merge → events. CDM edits to bridged entities → reverse wiring → envelope `bridge.write_back` → adapter `upsert` with etag; conflicts land in review queue per wiring `conflict:` policy.

## State management rules
Shell is stateless (session cookie → server state). All durable state in Postgres; NATS JetStream is transport + replay buffer, never source of truth. Adapter KV is tenant-and-adapter-scoped scratch (Postgres table `tenant_adapter_kv`), never CDM data. Historical `adapter_kv` rows are not used by runtime code because they lack tenant authority.

### Canonical event delivery
Store writes one versioned canonical event document to append-only `event_log` and `outbox` in the same transaction as the governed CRM state change. The outbox owns the stable `event_id`; Kernel never creates a replacement ID during relay. Store leases pending rows without holding a database transaction across network I/O. Kernel publishes to the exact non-PII semantic event subject with `event_id` as `Nats-Msg-Id`, waits for a JetStream publish acknowledgement, and only then records `published_at` and the positive stream sequence through Store. A crash after broker acknowledgement but before that receipt causes an at-least-once retry of the same logical event. Invalid canonical rows are parked, not deleted, and transient broker failures remain pending. A durable parked row makes canonical event readiness fail closed, including after a relay restart, until it is investigated through the operator recovery process.

W3C `traceparent` and optional bounded `tracestate` are operational carriers, not tenant authority or business provenance. Fabric derives a server child only after parsing the request, Store persists the carrier separately from `InvocationContext`, Executor derives downstream children after asynchronous dispatch, and Kernel emits only those two headers to JetStream. Baggage is neither accepted nor persisted. Durable request, correlation, causation, objective, task, and approval references remain in the business envelope/event contract even after a trace ends.

Nexus-connected readiness requires Postgres, core NATS connectivity, the exact `HYDRA_CRM_EVENTS_V1` stream contract, and a running relay that has completed a successful iteration. `/v1/nexus/events/status` reads the same typed runtime status. Standalone mode does not require the Nexus event seam.

## Persistence boundaries
Only `crates/store` executes SQL. This includes Kernel health and migration operations plus local authentication/session persistence; Fabric and Kernel consume typed Store repositories and do not hold a database pool for runtime queries. sqlx macros with checked queries; migrations forward-only + paired `-- revert:` note; JSONB bodies validated against the kind's JSON Schema before write. Store also owns the versioned, tenant-scoped export projection and non-destructive retention preview; these return canonical entities, relationships, and safe operational metadata without exposing TOKENKILLER content or creating a second CRM source of truth.

## External integration boundaries
Required target: all external egress -> the explicit proxy boundary (allow-list, auth injection from vault, rate limits, audit). Adapters get egress only via `host.http`, delegated through the same proxy with the adapter's grant. The Kernel validates `HYDRA_EGRESS_PROXY_URL` and, in staging/production, passes it explicitly to every configured LLM provider and OIDC JWKS client; Fabric and BridgeHost expose the same proxy-aware construction seam. Existing unconfigured constructors remain only for standalone development and deterministic tests. Ambient `HTTP_PROXY`/`HTTPS_PROXY` behavior is not a production authorization contract.

The configured Kernel bridge lifecycle uses the same explicit proxy-aware
BridgeHost client at runtime; it does not inject the deny-only test client into
an active adapter path. If proxy-client construction fails, the lifecycle is
unavailable and its typed handlers are not registered. This proves local
construction and wiring only, not staging ACL or upstream connectivity.

## Security boundaries
Vault (file-based age-encrypted in v1) ↔ named secrets, loaded by Kernel through `bridge_host::VaultSecretSource`. `hydra-vault` is the owner provisioning/rotation surface and never prints values. Grants: per-adapter {origins[], secret_names[], dsn?, fuel}. Governor constitution is loaded read-only at boot; hot-reload requires signed config. AuthN in `fabric::auth`; AuthZ = role×tenant checks in service traits (never in templates).

## Validation boundaries
Trust boundaries validate: fabric handlers (serde + garde), store (schema registry), bridge-host (WIT types + record JSON schema), tokenkiller (output contracts).

## Error handling boundaries
`thiserror` per crate; `fabric` maps to problem+json (RFC 7807). Adapter `bridge-error` variants map 1:1 to retry/park/alert policies in kernel sync loop (SPEC-006 taxonomy).

## Observability boundaries
`tracing` spans at every boundary crossing with fields {tenant, envelope_id?, adapter_id?, route?}. The Kernel owns the process-local Prometheus registry and records bounded request totals and latency after responses through its L6 middleware; route labels use a fixed taxonomy and never contain tenant IDs, query strings, identities, or customer data. Crates expose `metrics()` hooks. No `println!` outside scripts. Live scraping, dashboards, alerts, and staging observability drills remain operational readiness work.

## TOKENKILLER boundary (mandatory on every LLM call)
`agents` NEVER call `llm-router` directly. Call path: agent → `tokenkiller::Session::complete(route, segments, tail)` → assembles canonical prefix → router → NukeGuard-wrapped stream → contract validation → ledger. Invariant TK-1..TK-6 below.

## Architectural invariants (violations = failing review)
- INV-1 No LLM output executes without a Governor decision.
- INV-2 Only bridge-host links wasmtime; adapters have zero ambient capability.
- INV-3 Only store touches SQL; every mutation lands in event_log via outbox.
- INV-4 PII-tagged prompts route only to `private` providers (structural check in router).
- INV-5 hard delete is impossible through any code path (soft-delete flag; any future purge job requires a separately authorized policy and ExecPlan).
- TK-1 Every LLM request is assembled by tokenkiller::prefix (never string concat in agents).
- TK-2 Segments serialize via tokenkiller::canon (sorted keys, LF, NFC, fixed floats, no timestamps/randomness in S0–S2).
- TK-3 Segment order S0→S1→S2→S3; S0–S2 bytes may change only via versioned config bump (which resets cache intentionally).
- TK-4 Transcripts are append-only; prior turns are never rewritten or re-serialized.
- TK-5 Every stream passes NukeGuard; budget breach ⇒ abort + repair-prompt retry (max 1) + ledger `nuke_aborts` increment.
- TK-6 Ledger records hit/miss tokens per call; 1h rolling `tk_cache_hit_ratio` < 0.97 on any deepseek route ⇒ WARN alert; < 0.90 ⇒ page.

## Forbidden architecture moves
Adding Node/npm; second SQL entry point; agents holding credentials; LLM inside governor; adapter code outside Wasmtime; editing prior transcript turns; dynamic content (time, request-id, shuffled keys) in S0–S2 segments; unbounded `max_tokens`.

## How to add a new feature
1. Spec it (`.agent/templates/spec-template.md`) → 2. ExecPlan → 3. Domain types in L1 → 4. store queries → 5. service trait in fabric/agents → 6. shell view → 7. tests per TESTING.md → 8. docs.

## How to add a new dependency
AGENTS.md §8. Additionally run `cargo deny check licenses` and record in DECISIONS.md.

## How to modify data schema
New sqlx migration; update JSON Schema registry + `cdm` types; `cargo sqlx prepare --workspace`; integration tests prove old rows still read (additive-only in v1).

## How to add a new integration (bridge)
Never hand-wire into kernel. Write/generate an adapter against `wit/hydra-bridge.wit`, add grant entry, pass `cargo test -p bridge-host --test conformance -- <adapter>`, add wiring/*.map.yaml, register via `POST /v1/bridges` (envelope-gated).

## Architecture review checklist
[ ] imports respect layer table  [ ] no new SQL outside store  [ ] no wasmtime outside bridge-host  [ ] TK-1..6 hold (rg for `llm-router` imports in agents = only tokenkiller)  [ ] INV-1..5 hold  [ ] events emitted for every mutation  [ ] docs updated.

## Governed Bridge Synchronization

The bounded synchronization seam is a manually invoked incremental page or
full-relist snapshot. An authenticated Nexus principal proposes
`hydra.bridges.sync` through MCP or `POST /v1/nexus/bridges/{id}/sync`; Fabric
binds the adapter identity from the REST path, resolves tenant authority from
the verified binding, and creates an ActionEnvelope. The Kernel executes only
the typed `bridges/sync_adapter` handler after the Governor decision and selects
the mode from the persisted adapter descriptor.

Store owns the cursor, run lease, canonical bridge-origin upsert, soft delete,
and conflict metadata. Cursor state is scoped by Hydra tenant, adapter, and
canonical kind; callers cannot provide or advance a cursor. A complete page
advances the cursor in the same transaction as entity, audit, event, and
outbox writes. Invalid changes park bounded conflict metadata and leave the
cursor unchanged. No raw provider payload is persisted in conflict records.

The full-relist path follows WIT `list` cursors under fixed page, record, and
aggregate-byte bounds. Store compares the complete validated snapshot in one
transaction, leaves unchanged active rows untouched, revives matching
tombstones, and soft-deletes only missing active bridge-origin rows.

An owner-created schedule may optionally create the same governed sync
envelope on a durable cadence. The scheduler leases due rows in Store,
persists `hydra.scheduler` provenance and deterministic slot idempotency, and
never calls BridgeHost or canonical CRM mutation code directly. The feature is
disabled by default and fails readiness closed when enabled without the typed
sync handler. Mapping activation, canary, and promotion remain unavailable.

## Governed Bridge Conformance

The authenticated `bridge-conformance` A2A workflow is a read-only audit
boundary over a configured, digest-pinned adapter. Fabric supplies only the
verified tenant and the bounded adapter ID, kind, and limit; Kernel resolves
the active tenant-scoped adapter record and persisted grant/configuration;
BridgeHost performs bounded `describe`, `probe`, schema, list, and optional
incremental-read calls through Wasmtime/WIT. The result contains metadata,
counts, digest, fuel, and a deterministic report, never raw CRM records,
secrets, provider responses, or mutation results.

Conformance does not create an ActionEnvelope or write CDM, audit, event, or
outbox state. A2A task persistence is the only durable workflow boundary.
Missing runtime, inactive adapter, digest mismatch, invalid pages, and
cross-tenant lookup all fail closed. This does not make synchronization,
activation, canary, promotion, or EP-010 production readiness available.
