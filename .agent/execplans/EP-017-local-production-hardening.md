# EP-017 Local Production Hardening

Plan status: COMPLETE

## 1. Purpose / Big Picture
Close the highest-risk production code gaps that are executable inside the Hydra repository: local shell authentication must use a verified persisted session, local REST tenancy must come from that authenticated session rather than a caller-selected header, production cookies must be secure, and integration gates must refuse unsafe or ambient database targets. This plan does not manufacture staging, provider, recovery, or human-sign-off evidence.

## 2. Scope
Harden the server-rendered shell and local Hydra REST boundary, add regression tests for session and tenant binding, make cookie security environment-aware with staging/prod fail-closed behavior, and make the integration test harness explicit about its test database target. Preserve standalone operation, local development ergonomics, existing safe API compatibility, soft-delete-only semantics, and the Nexus control-plane boundary.

## 3. Non-goals
No production deployment, staging drill, real identity-provider setup, human approval, database destructive operation, new crypto dependency, retention purge, scheduler, live-provider call, or EP-016 model/A2A/skills implementation. No caller-provided tenant value may become authority. No direct store mutation is added to shell handlers.

## 4. Context and Orientation
The current shell parses the UUID-shaped session token as a tenant and falls back to a hard-coded development tenant; its AuthCtx has no loaded session. Local REST requires `x-hydra-tenant` before checking the session, which leaves the request shape responsible for tenant selection. The current cookie lacks `Secure`. `scripts/test-integration.sh` accepts an inherited `DATABASE_URL`, so a wrong local credential can make the required verifier fail against an unintended support database.

## 5. Files to Read First
`AGENTS.md`; `COMMANDS.md`; `.agent/PLANS.md`; `.agent/EXECUTION_RULES.md`; `.agent/state/execplan-index.md`; `PRODUCTION_READINESS.md`; `SECURITY.md`; `crates/fabric/src/auth/session.rs`; `crates/fabric/src/rest/mod.rs`; `crates/fabric/src/services.rs`; `crates/shell/src/routes/mod.rs`; all `crates/shell/src/routes/*.rs`; `crates/kernel/src/config.rs`; `crates/kernel/src/main.rs`; `scripts/test-integration.sh`; `TESTING.md`.

## 6. Files to Change
`.agent/state/execplan-index.md`; `scripts/check-execplan-state.sh`; this plan; `crates/fabric/src/rest/mod.rs`; `crates/fabric/src/services.rs` only if the local tenant helper contract changes; `crates/fabric/tests/authz_endpoints.rs`; `crates/shell/src/routes/mod.rs`; `crates/shell/src/routes/agents.rs`; `crates/shell/src/routes/approvals.rs`; `crates/shell/src/routes/autonomy.rs`; `crates/shell/src/routes/bridges.rs`; `crates/shell/src/routes/login.rs`; `crates/shell/src/routes/pipelines.rs`; `crates/shell/src/routes/workspace.rs`; `crates/kernel/src/config.rs`; `crates/kernel/src/main.rs`; `scripts/test-integration.sh`; `COMMANDS.md`; `TESTING.md`; `SECURITY.md`; `PRODUCTION_READINESS.md`.

## 7. Interfaces and Contracts
`SessionStore::lookup` is the only authority for a local browser session. A verified shell identity carries the loaded `Session`, tenant, and `AuthCtx` through request extensions; handlers do not infer tenant identity from cookie shape or request headers. Local REST derives tenant from the verified session and may reject a mismatching legacy header without using it as authority. `Secure` is required for staging/prod cookies and may be omitted only for explicit dev mode. Integration tests use `HYDRA_TEST_DATABASE_URL` or a documented loopback default and reject non-loopback database hosts.

## 8. Milestones
M1 Plan activation and state validation. Change the authoritative index and state checker to make EP-017 the only active plan while EP-016 remains deferred. Validation: `bash scripts/check-execplan-state.sh`. Expected: `execplan state: ok`. Recovery: restore the index/status pair together and rerun the checker; do not leave two active plans.

M2 Local REST session-bound tenancy. Remove request-header tenancy authority from `local_auth_middleware`; load the session first, derive the tenant from it, preserve the explicit dev identity only behind the existing dev flag, and add regression coverage for missing, mismatching, and caller-supplied tenant headers. Validation: `cargo test -p fabric --test authz_endpoints -- --nocapture` and `cargo test -p fabric --test nexus_auth -- --nocapture`. Expected: all tests pass and cross-tenant/header substitution is denied. Recovery: revert only the middleware/helper changes and keep the old tests as failing evidence; never restore header authority.

M3 Verified shell identity and cookie policy. Add authenticated shell request context backed by `SessionStore::lookup`, remove the hard-coded tenant fallback from protected routes, require a valid session for protected pages/actions, and add `Secure` to session/CSRF cookie headers in staging/prod. Validation: `cargo test -p shell --lib -- --nocapture` plus `cargo test -p fabric --lib auth -- --nocapture`. Expected: shell crate tests and cookie/session unit tests pass; unauthenticated protected requests fail closed. Recovery: keep login routes public, disable only the new middleware under explicit unit-test construction, and do not reintroduce a tenant default.

M4 Safe integration database targeting. Make `scripts/test-integration.sh` prefer `HYDRA_TEST_DATABASE_URL`, validate that the selected host is loopback, and export the validated URL for SQLx. Document the override and isolated-service command in `COMMANDS.md` and `TESTING.md`. Validation: isolated loopback Postgres integration run with `HYDRA_TEST_DATABASE_URL` plus a negative non-loopback configuration check. Expected: the isolated run prints `integration tests: ok`; the negative check exits non-zero with a named safety error. Recovery: use the isolated Postgres container and explicit loopback URL; never bypass the host check.

M5 Full local gate and evidence reconciliation. Run security, dependency, focused auth/session tests, integration, E2E, and the full verifier against isolated loopback services; update security/readiness documentation with exact evidence and remaining external blockers. Validation: `bash scripts/security-check.sh`, `bash scripts/dependency-audit.sh`, `bash scripts/test-integration.sh`, `bash scripts/test-e2e.sh`, `bash scripts/verify.sh`, and `bash scripts/check-execplan-state.sh`. Expected: every local command emits its required `: ok` marker, `verify: ok` is terminal, and the production-readiness gate still fails closed on absent staging evidence. Recovery: stop at the first failing gate, record exact output, and do not weaken the gate to claim readiness.

## 9. Concrete Steps
Execute M1 through M5 in order. After each validation, tick only that milestone, append the exact result to the Decision Log and Outcomes, and compare the final diff with this section. Update the index to EP-017 `COMPLETE` only after M5 passes; otherwise keep the plan active with the precise blocker recorded.

## 10. Validation and Acceptance
The plan is complete only when local REST and shell tenant authority is session-derived, production cookie policy is tested, unsafe integration database targets fail closed, all M1-M5 commands pass with expected output, and `scripts/verify.sh` prints `verify: ok`. Staging drills, 24-hour soak, live identity/TLS, external secret recovery, and human sign-off remain outside this plan and must stay open in EP-010.

## 11. Idempotence and Recovery
The changes are additive and rerunnable. Reapplying the state index or tests must not create duplicate plan rows or sessions. No migration or destructive data command is introduced. If a local test service has stale credentials, start an isolated loopback service with explicit credentials rather than changing production-like data or weakening authentication checks.

## 12. Progress
- [x] M1 - Plan activated and state checker extended; `execplan state: ok` recorded on 2026-08-11
- [x] M2 - Local REST session-bound tenancy; `authz_endpoints` 22 passed and `nexus_auth` 12 passed on 2026-08-11
- [x] M3 - Verified shell identity and cookie policy; Shell 2 passed, Fabric auth 27 passed, and strict Shell clippy passed on 2026-08-11
- [x] M4 - Safe integration database targeting; non-loopback rejection passed and isolated integration/failure suites passed on 2026-08-11
- [x] M5 - Full local gate and evidence reconciliation; security, dependency, integration, E2E, full verify, and state gates passed on 2026-08-11; production-readiness correctly failed closed at missing D1 evidence

## 13. Surprises & Discoveries
- The first production-readiness run reached the compile/test gates but failed in SQLx query preparation because an inherited `DATABASE_URL` addressed a local Postgres instance with a different password. This is a validation-safety problem, not evidence that the database code is green.
- The shell session cookie is a random session token, while the old helper attempted to parse that token as a Hydra tenant UUID. The resulting development fallback was not a safe compatibility mechanism.
- The existing external MCP contract expected `422` when a bearer token reached the local CRUD route; after authentication was made fail-closed before local tenant parsing, the correct boundary response is `403`, and the contract assertion was updated accordingly.

## 14. Decision Log
| Date | Decision | Rationale |
|---|---|---|
| 2026-08-11 | Activate EP-017 before EP-016 | Production-boundary hardening has higher readiness value than optional model/A2A/skills interoperability; EP-016 remains deferred but is now unblocked by EP-011 through EP-015. |
| 2026-08-11 | Derive local tenancy from `SessionStore::lookup` | Cookies and request headers are transport inputs, not tenant authority; the persisted session already binds the user to a tenant. |
| 2026-08-11 | Keep the legacy `hydra-dev-admin` header path only behind `allow_development_identity` | Existing local integration fixtures require an explicit development tenant, while Kernel enables this path only for `HYDRA_ENV=dev`; staging and production use persisted sessions. |
| 2026-08-11 | Reject non-loopback integration database targets | Required tests must never silently run against staging or production-like databases, and ambient `DATABASE_URL` caused a reproducible wrong-credential failure. |
| 2026-08-11 | Return `403` before local tenant parsing for external bearers | Local CRUD is a separate session-authenticated compatibility surface; Nexus principals must use the versioned Nexus facade, so a bearer without a local session is denied before any header is interpreted. |

## 15. Outcomes & Retrospective
EP-017 completed on 2026-08-11. `scripts/security-check.sh` emitted `security check: ok` with two pre-approved advisory warnings; `scripts/dependency-audit.sh` emitted `dependency audit: ok`; the isolated integration run emitted `integration tests: ok` and `failure suites: ok`; `scripts/test-e2e.sh` ran both required scenarios and emitted `e2e tests: ok`; the full `scripts/verify.sh` run exited 0 through its terminal `verify: ok` path; and `scripts/check-execplan-state.sh` emitted `execplan state: ok`. The production-readiness gate reran the verifier successfully and then failed closed with `production-readiness:FAIL: drill D1 — no PASS row in OPERATIONS.md`, which is the truthful result because staging evidence is absent.

The implementation now derives local REST and Shell tenancy from verified persisted sessions, protects staging/prod cookies with `Secure`, rejects unsafe integration database hosts before tests execute, and preserves the explicit development identity only behind the Kernel's `HYDRA_ENV=dev` flag. EP-010 remains partial for staging, recovery, live provider, operational, and human-owned evidence; EP-016 remains deferred.
