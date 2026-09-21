# SECURITY.md

## Goals

Tenant isolation, governed autonomy, sandboxed integrations, secret hygiene, auditable state changes, and structural controls on PII egress are non-negotiable.

## Threat Model Summary

Relevant adversaries include a compromised adapter, prompt-injected agent, stolen local session, forged Nexus token, hostile MCP/REST client, malicious cross-business caller, and untrusted upstream CRM content. Protected assets include canonical CRM data, tenant bindings, credentials, approvals, audit history, LLM budget, and external execution authority.

Trust boundaries are Fabric authentication/authorization, Hydra-owned external bindings, the Governor, typed execution handlers, Store transactions, BridgeHost grants, the egress proxy hop, and the canonical event relay. Store is the sole runtime SQL boundary, including authentication/session persistence and Kernel health/migration access; Fabric and Kernel receive typed Store APIs instead of issuing database queries. Nexus authorization does not replace Hydra governance.

NATS is delivery infrastructure, not an authority path. The Kernel uses a
mounted credentials file and required TLS in staging and production, rejects
credential-bearing NATS URLs, and supports pinned CA or mTLS files without
logging their contents. Provider, OIDC, compatibility, and public base
endpoint URIs are also required to be absolute and free of embedded
credentials. Runtime diagnostics are emitted through centrally redacted
fields rather than dynamic `message` interpolation; filesystem paths are
redacted as well. Development and loopback tests are the only plain NATS
profile.

## Authentication And Authorization

- Nexus mode is an OAuth/OIDC resource server using configured asymmetric JWT algorithms, issuer, audience, signature, time-claim, scope, principal-type, and binding validation. JWKS keys are cached; a pinned public key is the offline alternative.
- One active Nexus provider/tenant/business binding maps to one Hydra tenant in v1. Caller headers, tool arguments, and MCP `_meta` are never tenant authority.
- Local Hydra roles remain a separate compatibility path. The fixed development bearer is enabled only in `HYDRA_ENV=dev`.
- The Shell login page derives its development banner and form copy from the same runtime flag; staging and production never advertise arbitrary-credential behavior. The outer Store-backed limiter admits login requests before password verification and fails closed if its authority is unavailable.
- The legacy `fabric::auth::jwt::TokenService` is a development/test helper only; it is not used by the HTTP resource-server path and rejects non-`HS256`/`JWT` headers, invalid issuer or subject, expired/future tokens, and oversized input. Production authentication remains asymmetric OIDC.
- Migration `0016_auth_seed_hardening.sql` marks the historical `0007_auth.sql` `admin` row as `development_seed` and disabled; `SessionStore` rejects that source in every environment. The dev bearer is a separate test fixture, not a database credential.
- External approval requires a human-delegated principal, `hydra.envelopes.approve`, accepted authentication strength, matching tenant/envelope, and a proposer/approver separation check.
- Missing identity, scope, binding, capability, handler, schema, or approval fails closed.
- The local `hydra-admin` owner tool is not an HTTP authority path. It requires
  `HYDRA_ADMIN_CONFIRM=I_UNDERSTAND` for mutations, reads passwords only from
  stdin, hashes them with the existing Argon2id helper, prints identifiers and
  status only, and cannot enable the migration-owned `development_seed`.
  Binding changes reuse the Store-owned soft status transitions.

## Governed Mutations

Every Nexus/MCP mutation becomes an idempotent ActionEnvelope and passes the tenant-aware deterministic Governor before a typed handler can execute. Models and agents may propose but cannot approve, construct execute tokens, or mutate Store directly. Approved execution identities are recoverable from Store after a Kernel restart, while stale `Executing` work fails readiness instead of being replayed without outcome evidence. Transition history, immutable approval assertions, execution receipts, audit rows, and outbox records preserve actor and non-secret invocation provenance.

## Input, Output, And Trace Safety

Fabric uses typed DTOs and JSON Schemas for Nexus capabilities, MCP input/output, canonical events, and bridge records. MCP enforces a configurable request-size ceiling and Origin policy. WIT is the only adapter ABI. Errors use typed/problem responses and must not echo tokens, secrets, full prompts, customer records, upstream response bodies, credential-bearing endpoint details, or raw provider diagnostics. Structured telemetry masks secret-bearing and dynamic diagnostic fields (`error`, `reason`, `query`, `url`, `uri`, `body`, and `payload`) centrally; constant operational `message` markers and explicitly bounded `failure_code` values remain available for runbooks. Adapter guest logs emit only severity, adapter identity, and bounded payload length; guest message content is never logged.

Durable invocation context excludes tokens, prompts, secrets, raw email bodies, and customer documents. W3C trace headers are propagated separately from business provenance; inbound baggage is not accepted. NATS subjects use a fixed non-PII taxonomy.

## Secrets

Tracked files and image context exclude `.env` and local agent state; security checks scan for secret-shaped material and inspect every production Fabric, admin, and Kernel Rust source file for SQL or pool access outside Store. Code and bridge grants refer to credentials by name, never by returning raw values to adapters.

The Kernel now loads the configured age-encrypted vault through `bridge_host::VaultSecretSource`. The owner-controlled `hydra-vault` tool reads values from stdin, prints names/status only, supports explicit key rotation, and provides validated ciphertext-preserving `backup` and confirmation-gated `restore` commands. Grants still constrain which named secrets an adapter can request. Owner-key custody, off-box artifact protection, and a staged vault restore drill are not yet proven. Prebuilt bridge deployment, pause, and resume are advertised only when `HYDRA_ADAPTERS_PATH` and the SecretSource are valid; bounded mapping proposals are separately available only through the TOKENKILLER `bridge_mapping` route and never activate a bridge.

BridgeEngineer mapping synthesis accepts only bounded adapter metadata, rejects secret-shaped or executable content, uses fixed TOKENKILLER S0-S2 segments plus a bounded dynamic tail, and validates the MappingYaml contract before normalization. The resulting artifact contains no prompt, token, customer body, Wasm, or executable diff. The authenticated A2A task may return a proposal or a redacted failure state; it cannot approve, deploy, synchronize, canary, promote, or mutate CRM state.

Signed skill packages are a separate owner-controlled trust boundary. Kernel accepts only a valid upstream `SKILL.md` frontmatter document plus a Hydra-local Ed25519 JWS whose content hash, signer, key ID, version, validity window, declared scopes, declared capabilities, and `declarative-only` sandbox policy all pass the configured trust policy. Revoked/unknown keys, duplicate versions, hash changes, `allowed-tools`, credentials, and policy-widening declarations are omitted or rejected without execution. The registry never reads or runs package scripts and never returns instruction bodies as authority.

## Release Integrity

The tag release workflow explicitly enables BuildKit maximum provenance and SBOM generation, then creates a signed GitHub artifact attestation for the exact pushed image digest. The image job uses narrowly scoped package, OIDC, attestation, and contents permissions and explicitly disables organization-only storage-record creation. `scripts/check-release-policy.sh` rejects missing provenance, SBOM, digest binding, attestation, storage-record policy, or masked required workflow paths. Local policy validation is not a signed release result; unsupported hosted private-repository attestation or missing permissions must fail the release rather than fall back to an unsigned image.

## Data Protection

Store queries and transition APIs are tenant-scoped, cross-tenant reads fail closed, and externally governed actions preserve soft-delete-only policy. The admin-only local tenant export and retention-preview routes derive tenant authority from a verified session; they return canonical records or safe aggregate metadata only and never expose prompts, tokens, secrets, or raw outbox payloads. Postgres audit/event/outbox state remains authoritative; NATS is delivery infrastructure, not a mutation input or source of truth.

Current limitation: export and tenant retention preview remain read-only code
paths. The optional `backup` profile can run the bounded artifact-retention
helper, but deletion is preview-only unless two explicit controls are set and
the helper can only remove matching files under its configured directory. The
optional `vault-backup` profile writes validated encrypted copies with no
network access and never prints the key. Off-box replication, legal retention
policy, JetStream snapshot/restore, key custody, and staging privacy/restore
evidence remain required before production readiness.

The autonomy freeze overlay is Store-owned and tenant-scoped. The local
`hydra-admin autonomy freeze|thaw` mutation requires explicit
`HYDRA_ADMIN_CONFIRM=I_UNDERSTAND`, preserves the prior matrix, bumps the
tenant policy revision, and emits a bounded audit/outbox event. No model,
agent, MCP metadata, header, or external caller can invoke it; already-
dispatched ExecuteTokens are not silently revoked. Staging D5 execution and
operator evidence remain open.

Event recovery is fail-closed and owner-confirmed. `--replay-events` accepts
only a non-negative cursor and a maximum batch of 1000, reads canonical rows
through Store-owned parameterized SQL, and requires the exact
`HYDRA_EVENT_REPLAY_CONFIRM=I_UNDERSTAND` value. It emits no event payload,
token, secret, URL, or customer record in its success output; only bounded
counts and the last outbox ID are printed. JetStream acknowledgement is
required before each row is considered replayed, while the outbox row remains
unchanged. Consumers must deduplicate the stable event ID, and no replay path
accepts a NATS message as a state mutation or source of truth.

## Rate Limiting And Session Boundary

The production Kernel installs a Store-backed fixed-window limiter at 60 requests per 60 seconds. Fabric derives a principal or peer-network key, hashes it with the versioned SHA-256 namespace, and sends only the digest to the Store. Postgres time and row locking provide the atomic multi-replica decision; expired rows are pruned opportunistically. Exceeded windows return 429 with bounded `Retry-After`. If the Store authority or pruning operation is unavailable, request admission fails closed with a generic 503 and no SQL detail. `RateLimiter::new` remains an explicit local-only constructor for deterministic tests and compatibility fixtures.

Shell protected routes now require `SessionStore::lookup` to succeed and receive a session-derived `AuthCtx`; they do not infer tenant authority from cookie shape or request headers. New local bearer sessions are stored in Postgres as SHA-256 token hashes rather than plaintext. Legacy plaintext rows are accepted only for the presenting token and upgraded in place on lookup; the durable token column is then cleared. Session and CSRF cookies are HttpOnly and SameSite=Lax, and Kernel enables `Secure` for staging and production while retaining an explicit dev-only non-secure mode. Browser CSRF/accessibility validation and external session-store/recovery evidence remain open in EP-010.

## Adapter And Egress Rules

Wasmtime/WIT is the only adapter runtime. Grants constrain named origins, secret names, optional read-replica access, and fuel; fuel exhaustion traps the guest and adapter KV is scoped by both Hydra tenant and adapter ID. BridgeHost lifecycle probes receive tenant identity from the governed envelope, and the historical unscoped KV API fails closed. BridgeHost grants are the destination-authorization boundary.

Tinyproxy is a source-restricted network choke point, not a second destination allowlist. In Compose, only Kernel joins `proxy-internal`; only Tinyproxy joins both that network and `egress-external`. Kernel must never receive direct external-network attachment or ambient adapter credentials. Configured OIDC JWKS requests have a bounded five-second deadline; LLM, BridgeHost, and Fabric egress HTTP requests have a bounded 30-second deadline. A slow or unreachable upstream cannot hold a worker indefinitely.

`HYDRA_EGRESS_PROXY_URL` is the explicit application contract for external HTTP egress. Kernel configuration rejects a missing or malformed value in staging and production, and configured LLM, OIDC, Fabric, and BridgeHost clients are constructed with that proxy rather than relying on ambient proxy environment variables. Proxy credentials are rejected in the URI and never echoed in configuration errors. Development/test constructors may remain unconfigured for deterministic local fixtures only.

The real configured bridge lifecycle injects `ReqwestEgressClient` with this
validated proxy. `DenyEgressClient` is retained only for explicit disabled or
test helpers; a configured adapter cannot be advertised as active while using
the deny-only runtime path. Construction failure remains fail closed.

## Governed Synchronization Boundary

Bridge synchronization is an external proposal, not a direct entity write.
Fabric authenticates the principal and resolves the Hydra tenant from the
active binding before creating the envelope. MCP metadata, HTTP headers,
request bodies, and REST path parameters cannot supply tenant authority; the
REST adapter ID is only a target identifier and is bound into the validated
proposal path.

The Kernel handler reads the active tenant-scoped adapter registry, persisted
grant, descriptor, and validated configuration before invoking WIT
`changes-since` or bounded `list` pages through BridgeHost. Store owns cursor
state and applies either an incremental page or the complete full-relist diff
transactionally with canonical entity, audit, event, and outbox writes.
Unchanged active rows are not rewritten, missing active bridge-origin rows are
soft-deleted only, and failed or over-bound relists cannot commit a partial
snapshot. Invalid records produce bounded conflict metadata and a redacted
canonical sync-conflict event; raw provider payloads, secrets, tokens, and
customer data are not stored in conflict diagnostics.

## Dependency Security

`scripts/security-check.sh` and `scripts/dependency-audit.sh` run Cargo audit/deny policy. The direct Wasmtime dependency disables defaults and names only the async Component Model, Cranelift, runtime, and standard-library features; the mandatory feature gate rejects the optional profiling/fxhash path. The age vault is pinned at `0.12.1`, and a mandatory graph gate rejects the retired `proc-macro-error2` path. Warning-only duplicate/license findings remain explicit residual risks in `PRODUCTION_READINESS.md`; a green command does not erase them.

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

## Governed Conformance Boundary

Bridge conformance is read-only and fail-closed. Fabric authenticates the A2A
principal and checks `hydra.bridges.read`; Kernel then performs a tenant-scoped
Store lookup for an active adapter and reuses only its persisted digest, grant,
and configuration. Callers cannot supply Hydra tenant IDs, component paths,
digests, grants, secrets, cursors, or provider URLs.

BridgeHost calls only `describe`, `probe`, schema, list, and declared
incremental-read exports. It bounds kinds, fields, records, JSON object shape,
identities, cursors, and fuel. It does not call adapter mutation exports or
write Store, CDM, audit, event, or outbox state. A2A artifacts contain only
sanitized metadata and counts; adapter data, secrets, upstream response bodies,
and control characters are not returned. Missing runtime, inactive or
cross-tenant adapters, digest changes, malformed pages, and invalid grants
fail closed without revealing adapter existence.

## Post-EP-037 Current Verification (2026-08-12)

EP-037 keeps the same authenticated dual-gate mutation path and adds only the
WIT-required bounded full-relist fallback. BridgeHost performs read-only list
pagination under fixed resource and identity bounds; Store performs the
tenant-scoped snapshot diff and soft-delete-only reconciliation atomically.
The caller cannot choose the mode, supply a cursor, or bypass Governor. No
raw record data is returned or logged. This is locally verified code evidence,
not staging or production-readiness evidence.

## Post-EP-051 Activation Gate

Prebuilt adapter activation now performs the same tenant-scoped, digest-pinned
read-only conformance check after probe and before the adapter registry enters
`active`. The gate uses the persisted grant and configuration, has a fixed
25-record limit, and never calls adapter mutation exports. A conformance
failure is bounded to 512 control-free characters, persisted on a revision-
checked `activating` to `failed` transition, and returned through the failed
execution path. No secret, raw record, prompt, or caller-selected tenant is
included in the transition event or receipt.
