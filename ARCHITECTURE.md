# ARCHITECTURE.md — Boundaries and Invariants (HYDRA)

## Purpose
Define concrete component boundaries, dependency rules, flows, and invariants so agents modify HYDRA without violating its safety or token-economics model.

## System overview
HYDRA = CDM entity graph (Postgres) + event spine (NATS JetStream) + deterministic Autonomy Governor + Agent Mesh (multi-LLM via Router+TOKENKILLER) + Universal Bridge runtime (Wasmtime, WIT contract) + Integration Fabric (REST/GraphQL/MCP/OAuth/n8n/email/social) + server-rendered Shell.

## The 6-Layer Paradigm (normative)
| Layer | Name | Crates / dirs | May import |
|---|---|---|---|
| L1 | Domain | `crates/cdm`, `crates/governor` | std only (+serde, uuid, thiserror) |
| L2 | Persistence | `crates/store` (sqlx repos, migrations/) | L1 |
| L3 | Services | `crates/bridge-host`, `crates/llm-router`, `crates/tokenkiller`, `crates/agents`, `crates/fabric` | L1, L2 |
| L4 | Interface | `crates/shell` (Askama+htmx), REST/GraphQL handlers in `crates/fabric` | L1–L3 |
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
crates/fabric/       REST v1, GraphQL, MCP server+client, OAuth2, webhooks, email, egress
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
Shell/REST/MCP request → `fabric` handler → service trait → (`store` for reads) | (agent proposal path for mutations): agent builds ActionEnvelope → `governor.evaluate()` → Execute→executor (in `kernel`) applies via `store` + emits events | Queue→approval queue | SuggestOnly | Block. Every state change appends to NATS stream `hydra.events.<tenant>` and the Postgres `event_log` (outbox pattern; store writes outbox row in the same tx, kernel relay publishes).

## Data flow (bridge)
Adapter `changes-since` (host-polled) → raw-record JSON → wiring transform pipeline (fixed library) → CDM upsert as `origin=bridge:<id>` → identity resolution merge → events. CDM edits to bridged entities → reverse wiring → envelope `bridge.write_back` → adapter `upsert` with etag; conflicts land in review queue per wiring `conflict:` policy.

## State management rules
Shell is stateless (session cookie → server state). All durable state in Postgres; NATS JetStream is transport + replay buffer, never source of truth. Adapter KV is adapter-scoped scratch (Postgres table `adapter_kv`), never CDM data.

## Persistence boundaries
Only `crates/store` executes SQL. sqlx macros with checked queries; migrations forward-only + paired `-- revert:` note; JSONB bodies validated against the kind's JSON Schema before write.

## External integration boundaries
All egress → `fabric::egress::Proxy` (allow-list, auth injection from vault, rate limits, audit). Adapters get egress only via `host.http` which delegates to the same proxy with the adapter's grant.

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
