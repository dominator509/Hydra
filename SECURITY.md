# SECURITY.md

## Goals

Tenant isolation, governed autonomy, sandboxed integrations, secret hygiene, auditable state changes, and structural controls on PII egress are non-negotiable.

## Threat Model Summary

Relevant adversaries include a compromised adapter, prompt-injected agent, stolen local session, forged Nexus token, hostile MCP/REST client, malicious cross-business caller, and untrusted upstream CRM content. Protected assets include canonical CRM data, tenant bindings, credentials, approvals, audit history, LLM budget, and external execution authority.

Trust boundaries are Fabric authentication/authorization, Hydra-owned external bindings, the Governor, typed execution handlers, Store transactions, BridgeHost grants, the egress proxy hop, and the canonical event relay. Nexus authorization does not replace Hydra governance.

## Authentication And Authorization

- Nexus mode is an OAuth/OIDC resource server using configured asymmetric JWT algorithms, issuer, audience, signature, time-claim, scope, principal-type, and binding validation. JWKS keys are cached; a pinned public key is the offline alternative.
- One active Nexus provider/tenant/business binding maps to one Hydra tenant in v1. Caller headers, tool arguments, and MCP `_meta` are never tenant authority.
- Local Hydra roles remain a separate compatibility path. The fixed development bearer is enabled only in `HYDRA_ENV=dev`.
- External approval requires a human-delegated principal, `hydra.envelopes.approve`, accepted authentication strength, matching tenant/envelope, and a proposer/approver separation check.
- Missing identity, scope, binding, capability, handler, schema, or approval fails closed.

## Governed Mutations

Every Nexus/MCP mutation becomes an idempotent ActionEnvelope and passes the tenant-aware deterministic Governor before a typed handler can execute. Models and agents may propose but cannot approve, construct execute tokens, or mutate Store directly. Transition history, immutable approval assertions, execution receipts, audit rows, and outbox records preserve actor and non-secret invocation provenance.

## Input, Output, And Trace Safety

Fabric uses typed DTOs and JSON Schemas for Nexus capabilities, MCP input/output, canonical events, and bridge records. MCP enforces a configurable request-size ceiling and Origin policy. WIT is the only adapter ABI. Errors use typed/problem responses and must not echo tokens, secrets, full prompts, or customer records.

Durable invocation context excludes tokens, prompts, secrets, raw email bodies, and customer documents. W3C trace headers are propagated separately from business provenance; inbound baggage is not accepted. NATS subjects use a fixed non-PII taxonomy.

## Secrets

Tracked files and image context exclude `.env` and local agent state; security checks scan for secret-shaped material. Code and bridge grants refer to credentials by name, never by returning raw values to adapters.

Current limitation: `HYDRA_VAULT_KEY` is startup-required, but a persisted encrypted vault and production BridgeHost secret source are not wired in the current Kernel. Bridge lifecycle execution is therefore advertised unavailable. Do not claim `vault/secrets.age` backup/decryption or provision real bridge credentials until that implementation and restore path are verified.

## Data Protection

Store queries and transition APIs are tenant-scoped, cross-tenant reads fail closed, and externally governed actions preserve soft-delete-only policy. Postgres audit/event/outbox state remains authoritative; NATS is delivery infrastructure, not a mutation input or source of truth.

Current limitation: no deployed retention/export/backup scheduler is present in the checked-in Compose topology. Retention, tenant export, backup cadence, and restore behavior require implementation and staging evidence before production readiness.

## Rate Limiting And Session Boundary

The Kernel installs a process-local fixed-window limiter at 60 requests per 60 seconds. It keys by `PrincipalContext` when that extension is already available and otherwise by peer IP. This is tested to return 429, but it is not a distributed multi-replica quota system.

Shell cookies are HttpOnly and SameSite=Lax and state-changing shell routes use CSRF tokens. Current limitation: the cookie builder does not set `Secure`; production session hardening and browser validation remain EP-010 work.

## Adapter And Egress Rules

Wasmtime/WIT is the only adapter runtime. Grants constrain named origins, secret names, optional read-replica access, and fuel; fuel exhaustion traps the guest and adapter KV is scoped. BridgeHost grants are the destination-authorization boundary.

Tinyproxy is a source-restricted network choke point, not a second destination allowlist. In Compose, only Kernel joins `proxy-internal`; only Tinyproxy joins both that network and `egress-external`. Kernel must never receive direct external-network attachment or ambient adapter credentials.

## Dependency Security

`scripts/security-check.sh` and `scripts/dependency-audit.sh` run Cargo audit/deny policy. Allowed advisories and warning-only duplicate/license findings remain explicit residual risks in `PRODUCTION_READINESS.md`; a green command does not erase them.

## Security Checklist

[ ] No secret material in the diff or image context
[ ] Every new trust boundary validates identity, scope, schema, binding, and tenant
[ ] External mutations use envelopes and Governor; no direct Store path
[ ] New logs/events/traces exclude secrets, prompts, tokens, and customer bodies
[ ] Adapter grants and egress topology are unchanged or ADR-reviewed
[ ] Cargo audit/deny results and allowed advisories are reviewed
[ ] Session, CORS/Origin, rate-limit, and cross-business tests pass
[ ] Production-only gaps in `PRODUCTION_READINESS.md` remain open until evidenced

## STOP Conditions

Disabling the sandbox, widening adapter authority beyond named grants, exposing credentials, bypassing Governor, allowing caller-selected tenant authority, connecting Nexus to Hydra Postgres, weakening approval separation, or performing destructive production data operations requires an immediate STOP under `AGENTS.md`.
