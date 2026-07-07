# SPEC-000 Product Scope
Status: Accepted | Owner: djw | Phase: 0 | ExecPlans: all

## User-visible goal
One self-hosted app where a business runs its CRM natively, wraps legacy CRMs into live agentic tabs, and dials agent autonomy from L0 to L5 per action domain.

## Non-goals
SaaS control plane; native mobile; telephony; fine-tuning; agent-authored sync code; Node toolchain.

## Terms
CDM (canonical data model); Envelope (gated agent action); Cell (domain×action×kind autonomy setting); Bridge/Adapter (WASM legacy connector); Wiring (field map file); TK (TOKENKILLER); Nuke (over-budget model output).

## Required behavior (outcome level)
O1 native CRM CRUD across party/deal/pipeline/activity/ticket/campaign; O2 bridge lifecycle discover→synthesize→conform→wire→canary→promote→tab; O3 autonomy matrix editing + approval queue + audit trail; O4 agent capabilities (draft/send email, pipeline hygiene, dedupe, social publish) all via envelopes; O5 integration surface REST/GraphQL/MCP²/OAuth²/n8n/webhooks/email; O6 token economics: deepseek routes ≥97% cache-hit, zero un-guarded streams.

## Inputs/Outputs
Inputs: user actions in shell, API/MCP calls, adapter change feeds, inbound email/webhooks. Outputs: CDM state, events, envelopes+receipts, outbound sends, metrics/ledger.

## Error states
Every failure is one of SPEC-006 taxonomy; user-visible errors are problem+json or shell flash with error code.

## Data rules
Tenant-scoped rows; soft-delete only; origin provenance on bridged entities.

## Security rules
SECURITY.md is normative; INV-1..5, TK-1..6 binding.

## Success metrics / acceptance
The four PROJECT_BRIEF success metrics, demonstrated in EP-010.
