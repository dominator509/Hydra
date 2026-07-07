# SPEC-005 Auth & Permissions
Status: Accepted | Owner: djw | Phase: 5 | ExecPlans: EP-006

## Goal
Local + OIDC login; role/tenant authz; OAuth2 provider for external apps; vault + grants operational.

## Non-goals
SCIM; per-field ACLs; SSO beyond one OIDC issuer.

## Model
Roles: Viewer < Operator < Approver < Admin (tenant-scoped). Principals: human user, service token, external MCP client, agent (internal principal `agent:<name>` — agents hold NO credentials; executor acts with system principal after Governor).
Sessions: cookie 12h idle/72h absolute; logout revokes. Tokens: OAuth2 client-credentials + auth-code; scopes read:cdm, write:envelopes, approve:envelopes, admin:bridges, admin:autonomy; JWT (ed25519, key in vault `oauth_provider_signing_key`), 1h expiry, jti revocation table.
Permission rules (enforced in service traits):
- read entity: Viewer; write entity (native): Operator; approve envelope: Approver; edit autonomy/bridges/grants: Admin.
- Envelope approval cannot be performed by the proposing principal (four-eyes at L2/L3).
Vault CLI: `hydra vault set|get-names|rotate` (get returns nothing to stdout for values — names only).
Audit: every authn event + role change → event_log.

## Security headers
CSP default-src 'self'; frame-ancestors 'none' (except passthrough proxy route which sets frame-src per-adapter); HSTS in prod; X-Content-Type-Options nosniff.

## Abuse prevention
Login rate limit 5/min/IP; token endpoint 10/min/client; lockout 15min after 10 fails.

## Error states
401 unauth, 403 {code:"authz_denied", need:"Approver"}, 409 four_eyes_violation.

## Required tests
authz matrix table-test (role × endpoint expected code); four-eyes test; token scope enforcement; session expiry; PII-gate proof (fabricated pii request to non-private provider returns router error — yes, this lives here as a security acceptance).

## Acceptance
`bash scripts/security-check.sh` ok; `bash scripts/test-integration.sh` ok including `authz_matrix` test names.
