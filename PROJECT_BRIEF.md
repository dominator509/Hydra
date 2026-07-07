# PROJECT_BRIEF — HYDRA (The CRM of CRMs)

## Project name
HYDRA — Agentic Meta-CRM.

## Problem statement
Businesses run fragmented CRMs with no agentic capability, no unified data graph, and no safe autonomy controls. Migrations are brutal, so legacy CRMs persist. Agent stacks that do exist burn tokens and act without guardrails.

## Target users
SMBs consolidating CRMs; agencies operating many client CRMs; RevOps engineers wanting guarded automation; privacy-focused orgs requiring self-hosted LLMs.

## Primary user outcomes
1. Full native CRM (parties, deals, pipelines, activities, tickets, campaigns) in one self-hosted GUI.
2. Legacy CRM wrapped into a live HYDRA tab via autonomously synthesized, conformance-gated WASM bridge adapters, with bidirectional field-wired sync.
3. Autonomy tunable L0→L5 per (domain × action × entity-kind); every agent action is an auditable ActionEnvelope.
4. Outward integration: REST/GraphQL/MCP/OAuth2/n8n/webhooks/email/social.
5. LLM cost discipline: DeepSeek prefix-cache hit rate ≥97%, hard output budgets, constitution spend caps.

## Business goals
- Replace 3+ tools per tenant; hours-not-months legacy onboarding; run 25 tenants on one box.

## Technical goals
- Rust workspace, single deterministic Governor, WASM-sandboxed adapters, event-sourced audit, TOKENKILLER token economics, zero Node toolchain.

## Out of scope (v1)
SaaS control plane; native mobile; telephony; fine-tuning; agent-authored code in sync path; k8s (stub only).

## Success metrics
- Bridge a SuiteCRM sandbox → live tab with bidirectional sync in < 4h wall clock, zero human-written adapter code.
- ≥97% DeepSeek cache-hit rate on agent routes over any 1h window (ledger metric `tk_cache_hit_ratio`).
- 0 envelope executions bypassing the Governor (audit invariant).
- scripts/verify.sh green; EP-010 checklist green.

## Production readiness definition
All PRODUCTION_READINESS.md sections green, scripts/production-readiness-check.sh prints `production readiness: ok`, restore drill + rollback drill executed and logged in OPERATIONS.md.
