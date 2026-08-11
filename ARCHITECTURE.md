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
- The Wasmtime/WIT `BridgeHost` is constructed at kernel boot. Durable adapter deployment, pause, resume, and sync lifecycle handlers are not registered and remain unavailable.
- DataSteward merge is experimental and produces an `ActionEnvelope` proposal only; no merge execution handler is registered.
- BridgeEngineer synthesis is unavailable after discovery, and Comms can draft text but has no delivery transport.
- The TOKENKILLER concierge path is available only when a real LLM provider is configured. Otherwise the kernel reports it disabled; fake routers are test-only.

## Data flow (bridge)
Adapter `changes-since` (host-polled) → raw-record JSON → wiring transform pipeline (fixed library) → CDM upsert as `origin=bridge:<id>` → identity resolution merge → events. CDM edits to bridged entities → reverse wiring → envelope `bridge.write_back` → adapter `upsert` with etag; conflicts land in review queue per wiring `conflict:` policy.

## State management rules
Shell is stateless (session cookie → server state). All durable state in Postgres; NATS JetStream is transport + replay buffer, never source of truth. Adapter KV is adapter-scoped scratch (Postgres table `adapter_kv`), never CDM data.

### Canonical event delivery
Store writes one versioned canonical event document to append-only `event_log` and `outbox` in the same transaction as the governed CRM state change. The outbox owns the stable `event_id`; Kernel never creates a replacement ID during relay. Store leases pending rows without holding a database transaction across network I/O. Kernel publishes to the exact non-PII semantic event subject with `event_id` as `Nats-Msg-Id`, waits for a JetStream publish acknowledgement, and only then records `published_at` and the positive stream sequence through Store. A crash after broker acknowledgement but before that receipt causes an at-least-once retry of the same logical event. Invalid canonical rows are parked, not deleted, and transient broker failures remain pending.

W3C `traceparent` and optional bounded `tracestate` are operational carriers, not tenant authority or business provenance. Fabric derives a server child only after parsing the request, Store persists the carrier separately from `InvocationContext`, Executor derives downstream children after asynchronous dispatch, and Kernel emits only those two headers to JetStream. Baggage is neither accepted nor persisted. Durable request, correlation, causation, objective, task, and approval references remain in the business envelope/event contract even after a trace ends.

Nexus-connected readiness requires Postgres, core NATS connectivity, the exact `HYDRA_CRM_EVENTS_V1` stream contract, and a running relay that has completed a successful iteration. `/v1/nexus/events/status` reads the same typed runtime status. Standalone mode does not require the Nexus event seam.

## Persistence boundaries
Only `crates/store` executes SQL. sqlx macros with checked queries; migrations forward-only + paired `-- revert:` note; JSONB bodies validated against the kind's JSON Schema before write.

## External integration boundaries
Required target: all egress -> `fabric::egress::Proxy` (allow-list, auth injection from vault, rate limits, audit). Adapters get egress only via `host.http`, delegated through the same proxy with the adapter's grant. The pre-EP-011 bridge host and LLM router still construct direct `reqwest` clients; this is a verified implementation gap, not an accepted exception.

## Security boundaries
Vault (file-based age-encrypted in v1) ↔ named secrets. Grants: per-adapter {origins[], secret_names[], dsn?, fuel}. Governor constitution is loaded read-only at boot; hot-reload requires signed config. AuthN in `fabric::auth`; AuthZ = role×tenant checks in service traits (never in templates).

## Validation boundaries
Trust boundaries validate: fabric handlers (serde + garde), store (schema registry), bridge-host (WIT types + record JSON schema), tokenkiller (output contracts).

## Error handling boundaries
`thiserror` per crate; `fabric` maps to problem+json (RFC 7807). Adapter `bridge-error` variants map 1:1 to retry/park/alert policies in kernel sync loop (SPEC-006 taxonomy).

## Observability boundaries
`tracing` spans at every boundary crossing with fields {tenant, envelope_id?, adapter_id?, route?}. Metrics registry in kernel; crates expose `metrics()` hooks. No `println!` outside scripts.

## TOKENKILLER boundary (mandatory on every LLM call)
`agents` NEVER call `llm-router` directly. Call path: agent → `tokenkiller::Session::complete(route, segments, tail)` → assembles canonical prefix → router → NukeGuard-wrapped stream → contract validation → ledger. Invariant TK-1..TK-6 below.

## Architectural invariants (violations = failing review)
- INV-1 No LLM output executes without a Governor decision.
- INV-2 Only bridge-host links wasmtime; adapters have zero ambient capability.
- INV-3 Only store touches SQL; every mutation lands in event_log via outbox.
- INV-4 PII-tagged prompts route only to `private` providers (structural check in router).
- INV-5 hard delete is impossible through any code path (soft-delete flag + purge job only).
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
