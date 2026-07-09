# EP-006 Auth, Security & Permissions

## 1. Purpose / Big Picture
Implement SPEC-005: real sessions+argon2id, roles, four-eyes approvals, OAuth2 provider+client, vault (age) + grants storage, security headers, rate limits — and retire all dev stubs.

## 2. Scope
crates/fabric/src/auth/*, vault CLI subcommands in kernel bin, migrations 0007_auth, shell login real, grants admin endpoints, security-check.sh hardened.

## 3. Non-goals
SCIM, per-field ACLs, multi-issuer OIDC, WebAuthn (backlog).

## 4. Context and Orientation
SPEC-005 normative; four-eyes enforced in EnvelopeService.approve; ExecuteToken path unchanged. OIDC client optional behind config (skip tests if unconfigured — feature-gated test).

## 5. Files to Read First
SPEC-005, SECURITY.md, crates/fabric/src/services.rs (approve), ENVIRONMENT secrets table.

## 6. Files to Change
migrations/0007_auth.sql (users, roles, sessions, oauth_clients, token_revocation), crates/fabric/src/auth/{mod.rs,session.rs,password.rs,oauth_provider.rs,oidc_client.rs,rate.rs,headers.rs}, crates/kernel/src/vault.rs + `hydra vault` CLI, crates/shell login real + role-aware nav, crates/fabric/tests/authz_matrix.rs + four_eyes.rs + token_scopes.rs, scripts/security-check.sh (add jwt/secret grep patterns), docker/Caddyfile HSTS note.

## 7. Interfaces and Contracts
`Ctx{principal, tenant, roles}` extracted per request; `ctx.require(Role, tenant)?`; JWT ed25519 kid rotation; scopes exactly SPEC-005; login/lockout limits per SPEC-005.

## 8. Milestones
M1 Users/sessions/argon2id + real login (replaces stub; HYDRA_ENV=dev seed user via db-setup flag). Validation: `cargo test -p fabric --test auth_sessions` → `m1: ok`. Recovery: argon2 params from crate defaults; do not invent.
M2 Roles + service-trait enforcement + authz matrix tests. Validation: `cargo test -p fabric --test authz_matrix` → `m2: ok` (matrix covers each SPEC-003 route × 4 roles). Recovery: missing route in matrix = test fails by design; extend matrix not code first.
M3 Four-eyes + audit events. Validation: `cargo test -p fabric --test four_eyes` → `m3: ok`.
M4 OAuth2 provider (+revocation) & tokens on REST; OIDC client behind cfg. Validation: `cargo test -p fabric --test token_scopes` → `m4: ok`. Recovery: clock skew in JWT tests → injected Clock.
M5 Vault CLI + grants storage + headers + rate limits; delete every `dev_stub` symbol (`rg dev_stub` empty). Validation: `bash scripts/security-check.sh && bash scripts/test-integration.sh` → both ok. Recovery: header middleware ordering (CSP before compression) — check tower layer order.

## 9. Concrete Steps
Order above; migration first.

## 10. Validation and Acceptance
security-check ok; integration ok; e2e still ok (login path changed — update e2e fixtures); PII-gate proof test present (SPEC-005); verify.sh ok; diff ⊆ §6.

## 11. Idempotence and Recovery
Migration additive; vault ops idempotent (set overwrites with backup file rotation).

## 12. Progress
- [x] M1 - [x] M2 - [x] M3 - [ ] M4 - [ ] M5

## 13. Surprises & Discoveries
## 14. Decision Log
## 15. Outcomes & Retrospective
