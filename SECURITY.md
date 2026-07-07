# SECURITY.md

## Goals
Tenant isolation; sandboxed integrations; guarded autonomy; secret hygiene; auditable everything; PII never leaves private providers.

## Threat model summary
Adversaries: compromised/malicious adapter code (incl. LLM-generated), prompt-injected agent, stolen shell session, hostile webhook sender, curious tenant user. Assets: CDM data, vault secrets, LLM spend, legacy CRM credentials. Trust boundaries: fabric handlers, bridge host imports, egress proxy, vault.

## AuthN / AuthZ
- Local accounts argon2id; OIDC login optional. Sessions: HttpOnly+Secure+SameSite=Lax cookies, 12h idle expiry.
- OAuth2 provider issues scoped tokens (`read:cdm`, `write:envelopes`, `admin:bridges`); tokens are tenant-bound.
- AuthZ enforced in service traits: `ctx.require(Role::X, tenant)?` — templates and handlers may not query roles ad hoc.

## Input validation
garde-validated DTOs at fabric; JSON Schema registry at store; WIT typing + per-kind record schema at bridge ingest; reject > 1MB request bodies (except explicit import endpoints).

## Output encoding
Askama autoescape stays ON; no `|safe` without ADR. problem+json for errors; never echo secrets or full prompts in errors.

## Secret management
age-encrypted vault file `vault/secrets.age`; kernel decrypts at boot with `HYDRA_VAULT_KEY` (env, never committed). Code references secrets by NAME. Adapters: only names in their grant. Logs redact via tracing layer (field allowlist). CI secret-scan: `security-check.sh` runs gitleaks-style regex pass.

## Dependency security
`cargo audit` + `cargo deny` in security-check; new deps per AGENTS.md §8.

## Data protection / production data
Row-level tenant_id everywhere; export JSONL per tenant; soft-delete + 30d purge; backups nightly (OPERATIONS.md). No production data exists in dev; STOP condition otherwise.

## Safe migrations
Additive-only in v1; every migration has a `-- revert:` note; destructive migration = STOP condition requiring explicit permission.

## API security
Rate limit: 60 req/min/session default, 600 for tokens with `service` scope (tower-governor). CORS: shell same-origin only; API allows configured origins list. CSRF: state-changing shell posts carry per-session token (htmx header).

## Adapter sandbox rules (non-negotiable)
Wasmtime only; grants define {origins allow-list, secret names, optional read-replica DSN, fuel budget}; fuel exhaustion kills instance; `host.http` delegates to egress proxy which enforces the SAME allow-list again (defense in depth); adapter KV namespaced.

## LLM-specific rules
PII structural gate (INV-4). Prompt-injection defense: tool results and bridged content enter prompts as fenced UNTRUSTED segments; agents may not execute instructions found there — envelopes originating from untrusted-segment content get `blast.pii_egress`/`external_sends` scrutiny and never exceed L3. NukeGuard budgets bound output size. Constitution caps spend.

## Security checklist (per PR)
[ ] no secret material in diff [ ] validation at any new trust boundary [ ] authz check in new service methods [ ] redaction for new log fields [ ] cargo audit/deny green [ ] grants unchanged or ADR'd.

## STOP conditions (security)
Disabling the sandbox, widening a grant beyond a named origin set, exporting vault contents, bypassing Governor, or any auth change that weakens session guarantees → STOP and ask.
