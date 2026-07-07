# 6Layer-MasterPrompt — INPUTS (filled for HYDRA)

## Project Name
HYDRA — The CRM of CRMs (Agentic Meta-CRM)

## Project Description
HYDRA is a self-hosted, agentic meta-CRM. It maintains a Canonical Data Model (CDM) entity graph in PostgreSQL, an event spine on NATS JetStream, and a deterministic Autonomy Governor that gates every agent action through six autonomy levels (L0 manual → L5 autonomous) scoped per (domain × action × entity-kind). Legacy CRMs are "upgraded" into HYDRA via the Universal Bridge: sandboxed WASM Component Model adapters (WIT contract `hydra:bridge@1.0.0`) that any language can target, generated/tested/deployed by the Bridge Engineer agent behind a brutal conformance gate, then projected into the GUI as native tabs (or authenticated passthrough iframes). Multi-LLM routing supports Anthropic, DeepSeek, and OpenAI-compatible self-hosted backends (llama-server/vLLM/Ollama/Venice) with a structural PII gate. The TOKENKILLER subsystem enforces DeepSeek prefix-cache discipline (>97% cache-hit target) and prevents "nuclear" oversized model outputs via streaming NukeGuard, output contracts, and a token ledger.

## Product Goal
Ship a production-ready, self-hosted meta-CRM where (1) any business runs full CRM operations natively, (2) any legacy CRM can be wrapped, wired, and upgraded to agentic within hours via autonomous bridge synthesis, and (3) autonomy is safe, auditable, and tunable from fully manual to fully autonomous — at a token cost an order of magnitude below naive agent stacks.

## Target Users
- Small/medium businesses consolidating multiple CRMs
- Agencies operating CRMs for many clients (multi-tenant)
- Ops/RevOps engineers who want agentic automation with hard guardrails
- Privacy-focused orgs requiring self-hosted LLMs for PII workloads

## Core User Outcomes
1. Manage parties/deals/pipelines/activities/tickets/campaigns in one GUI.
2. Connect a legacy CRM (API, DB, or UI-only) and see it as a live HYDRA tab with bidirectional sync and agent buttons on every record.
3. Set autonomy per domain/action; approve queued agent envelopes; audit everything.
4. Agents draft/send email, manage pipeline hygiene, dedupe/enrich data, publish social — all envelope-gated.
5. Integrate outward: REST, GraphQL, MCP (server+client), OAuth2 (both roles), n8n nodes, webhooks, IMAP/SMTP.
6. LLM spend stays within constitution caps; DeepSeek routes sustain ≥97% prefix-cache hit rate; no output ever exceeds route byte budgets.

## Existing Repository Status
Selected status: Greenfield repository

## Preferred Tech Stack
Frontend: Server-rendered Rust — Axum + Askama templates + vendored htmx (no Node toolchain). ASSUMPTION marked in ASSUMPTIONS.md.
Backend: Rust 1.79+ (workspace of crates: kernel, cdm, governor, bridge-host, bridge-wit, agents, llm-router, tokenkiller, fabric, shell)
Database: PostgreSQL 16 (sqlx, JSONB entity graph, logical replication for CDC)
Authentication: OAuth2/OIDC provider+client (self-issued tokens; argon2id local accounts), session cookies for shell
Hosting / Deployment: Self-hosted docker-compose (single box) → k8s later; images built via multi-stage Dockerfile
Testing: cargo test (unit/integration), proptest (conformance property suite), wiremock (HTTP fakes), sqlx test pools, curl-based smoke
Package Manager: cargo (Rust); no npm anywhere
CI/CD: GitHub Actions (or Gitea Actions equivalent) running scripts/verify.sh
Observability: tracing + tracing-subscriber (JSON logs), Prometheus /metrics via axum-prometheus, /healthz + /readyz

## Business Constraints
- Solo/small-team build; every phase must leave a runnable system
- LLM budget: constitution-capped (default $300/mo); DeepSeek preferred for high-volume agent loops due to cache pricing
- Must run on one 8-vCPU/32GB box for ≤25 tenants

## Technical Constraints
- Zero Node/JS build chain; vendored htmx only
- Adapters run ONLY inside Wasmtime with capability grants (origin allow-list, named secrets, fuel metering); no native plugins
- All external egress (adapters + LLM calls) through the single egress proxy
- Deterministic Governor: no LLM in the approval path
- TOKENKILLER discipline mandatory on every LLM call: canonical serialization, stability-ordered segments, 64-token block alignment, NukeGuard streaming caps

## Security / Compliance Constraints
- PII-bearing prompts may only route to providers tagged `private` (structural gate, not prompt-based)
- Secrets only via vault names; adapters never see raw credentials
- Append-only audit stream; hard-delete forbidden (soft-delete + 30d retention)
- Irreversible actions auto-demote one autonomy level

## Performance Requirements
- Shell TTFB < 150ms p95 (25 tenants, warm)
- Bridge sync: 10k records initial import < 10 min per adapter; steady-state pull ≤ 60s cadence
- Governor decision < 5ms p99
- DeepSeek routes: prompt_cache_hit_tokens / total prompt tokens ≥ 0.97 over any 1h window (CI replay gate ≥ 0.97)
- No single LLM response > route byte budget (default 16 KiB); NukeGuard aborts at budget +10%

## Accessibility Requirements
Shell: semantic HTML, keyboard navigable, no color-only state, WCAG AA contrast; htmx interactions must degrade to full-page POSTs.

## Data / Privacy Requirements
- Tenant isolation at row level (tenant_id everywhere, enforced in repository layer)
- Export: per-tenant JSONL dump; Deletion: soft-delete cascade + 30d purge job
- Backups: nightly pg_dump + WAL archiving; restore drill in EP-010

## Integrations
MCP server (expose CDM+envelopes as tools), MCP client (consume external servers), REST v1, GraphQL, OAuth2 provider+client, n8n (Hydra Trigger + Hydra Action nodes via webhook relay), IMAP/SMTP/JMAP email, social adapters (Meta/X/LinkedIn) as WASM bridges, DeepSeek + Anthropic + OpenAI-compatible LLM providers.

## Non-Goals
- No SaaS/multi-region control plane in v1
- No native mobile apps (responsive shell only)
- No arbitrary agent-written code in the sync path (fixed transform library only)
- No Node/npm toolchain; no Kubernetes in v1 docs beyond a stub
- No telephony/dialer in v1
- No fine-tuning pipelines

## Timeline / Milestones
Phases 0–9 per ROADMAP.md; production readiness gate = EP-010 complete + PRODUCTION_READINESS.md checklist green.

## Deployment Target
Self-hosted Linux (Ubuntu 24 LTS) via docker-compose; single box; Caddy TLS terminator.

## Special Instructions
1. Bake TOKENKILLER into the architecture as a first-class crate: DeepSeek context caching is prefix-based with 64-token block granularity and usage fields `prompt_cache_hit_tokens`/`prompt_cache_miss_tokens`; design for ≥97% hit rate via stability-ordered canonical segments (S0 constitution/system → S1 tool schemas → S2 tenant policy snapshot → S3 dynamic tail), append-only transcripts, zero timestamps/UUID churn in prefixes, and a CI replay gate.
2. "Nuclear failure" prevention is mandatory: streaming NukeGuard (byte/line budgets, dump-pattern detection, abort+repair), output contracts (diffs/references, never full payloads), max_tokens per route, ledger alerts.
3. Include reference implementations (reference/) for the hardest code: WIT contract, Wasmtime capability host, Governor state machine, TOKENKILLER canonical serializer + prefix assembler + NukeGuard + ledger, conformance harness, mapping-DSL executor. Lower-tier agents copy from reference/, they do not invent.
