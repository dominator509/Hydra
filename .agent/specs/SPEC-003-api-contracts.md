# SPEC-003 API & Service Contracts
Status: Accepted | Owner: djw | Phase: 3 | ExecPlans: EP-004

## Goal
Stable external surface: REST v1 + MCP server; internal service traits; bridge ABI = WIT.

## Non-goals
GraphQL (post-v1 flag), n8n node packaging (EP-009 artifact), UI.

## REST v1 (problem+json errors; utoipa OpenAPI served at /v1/openapi.json)
- `GET/POST /v1/entities/{kind}`; `GET/PATCH/DELETE /v1/entities/{kind}/{id}` (DELETE = soft; PATCH = JSON Merge Patch, version via If-Match ⇒ 409 on mismatch)
- `GET /v1/envelopes?state=`, `POST /v1/envelopes` (propose), `POST /v1/envelopes/{id}/approve|reject`
- `GET/PUT /v1/autonomy/cells` (PUT requires admin scope; emits event)
- `POST /v1/bridges` (register adapter+grant+wiring; itself envelope-gated `bridges.deploy_adapter`), `GET /v1/bridges/{id}/status`, `POST /v1/bridges/{id}/pause|resume`
- `GET /v1/tk/ledger?window=1h` → {route stats, hit_ratio}
- Auth: session cookie (shell) or Bearer token (OAuth2); 401/403 per SPEC-005.

## MCP server tools (same envelope discipline; external principal)
hydra.search_entities{kind,query} ; hydra.get_entity{kind,id} ; hydra.propose_envelope{domain,action,targets,payload,rationale} ; hydra.list_pending ; hydra.approve{id} (requires scope) ; hydra.pipeline_stats{pipeline_id} ; hydra.tk_stats{window}.

## Internal service traits (fabric)
EntityService, EnvelopeService, AutonomyService, BridgeService, TkStats — shell and REST both consume these; no handler touches store directly.

## Bridge ABI
`wit/hydra-bridge.wit` (copy from reference/bridge/hydra-bridge.wit) is normative; version `hydra:bridge@1.0.0`; adapters MUST export adapter interface exactly; capability degradation: kernel disables write-back if caps.write=false, uses full-relist sync if !incremental-sync.

## Inputs/Outputs/Errors
DTOs mirror CDM bodies; error taxonomy per SPEC-006 with `code` field; every mutating route returns the resulting envelope or entity with `version`.

## Performance
p95 <150ms reads at 25 tenants; list endpoints paginate (cursor, max 200).

## Observability
per-route metrics + trace span; envelope routes log envelope_id.

## Required tests
contract tests assert OpenAPI ↔ handler parity; integration: If-Match conflict, envelope approve flow, bridges register envelope-gated (attempt without approval at L2 stays queued), MCP tool schema snapshot.

## Acceptance
`bash scripts/test-integration.sh` ok; `curl -s localhost:8080/v1/openapi.json | jq .info.version` = "1.0.0".
