# EP-024 Production Identity and Bootstrap Hardening

Plan status: COMPLETE

## 1. Purpose / Big Picture

Remove the remaining fixed-development-credential path from every runtime environment without inventing an owner secret or performing deployment. The historical `0007_auth.sql` migration seeds `admin` with a documented development password in every database, and the normal Shell login calls `SessionStore::authenticate` without a seed-source guard. Hydra must preserve local development ergonomics through its separately gated bearer fixture while permanently retiring the migration-owned seed identity from form login and session lookup.

## 2. Scope

- Add an additive auth-user status/source migration that marks the known development seed disabled while preserving the row and audit history.
- Make `SessionStore` reject the migration-owned development seed in every environment; normal operator-created users remain authenticated by their stored credentials.
- Preserve the Kernel's separate `HYDRA_ENV=dev` bearer fixture without using it to enable the seeded database account.
- Prove login and session lookup fail closed for the seed in every environment while active operator credentials remain usable.
- Update security, environment, deployment, operations, readiness, commands, decision, audit, and plan-state documentation.

## 3. Non-goals

- No deletion or hard purge of users, sessions, CRM records, audit rows, or bindings.
- No hard-coded production password, bootstrap bearer, owner secret, or credential generation outside the existing dev-only fixture.
- No public owner/bootstrap endpoint, direct Nexus provisioning path, or binding-management UI in this plan; owner bootstrap remains an explicit follow-up requiring its own trust contract.
- No change to OAuth/OIDC Nexus authentication, MCP, Governor, bridge ABI, TOKENKILLER, or event semantics.
- No production database, staging deployment, push, merge, tag, or release.

## 4. Context and Orientation

`migrations/0007_auth.sql` unconditionally inserts the fixed `admin` seed and comments its password as `hydra-dev`. `crates/shell/src/routes/login.rs` calls `SessionStore::authenticate`; `crates/fabric/src/auth/session.rs` therefore needs a migration-owned source policy. The bearer `hydra-dev-admin` path is already gated by `AppState::allow_development_identity`, but that does not protect form login or existing sessions created from the seeded row. A migration-backed diagnostic also showed that the historical Argon2 hash does not validate the documented password, so the seed must not be re-enabled even in development. `NEXUS_PACKAGE_CONTRACT.md` correctly states that owner/bootstrap provisioning is not yet exposed; this plan closes only the fixed-credential exposure.

## 5. Files to Read First

`AGENTS.md`; `COMMANDS.md`; `.agent/PLANS.md`; `.agent/EXECUTION_RULES.md`; `.agent/state/execplan-index.md`; `SECURITY.md`; `ENVIRONMENT.md`; `DEPLOYMENT.md`; `OPERATIONS.md`; `PRODUCTION_READINESS.md`; `NEXUS_PACKAGE_CONTRACT.md`; `DECISIONS.md`; `migrations/0007_auth.sql`; `crates/fabric/src/auth/session.rs`; `crates/fabric/src/auth/mod.rs`; `crates/shell/src/routes/login.rs`; `crates/kernel/src/main.rs`; `crates/kernel/src/config.rs`; current Fabric and Store auth tests.

## 6. Files to Change

- `.agent/execplans/EP-024-production-identity-bootstrap-hardening.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `migrations/0016_auth_seed_hardening.sql` (new)
- `crates/fabric/src/auth/session.rs`
- `crates/fabric/src/auth/mod.rs` only if the constructor/config surface requires a public export
- `crates/fabric/tests/auth_seed_hardening.rs` (new)
- `crates/kernel/tests/runtime_wiring.rs` only if the environment wiring needs an executable assertion
- `SECURITY.md`
- `ENVIRONMENT.md`
- `DEPLOYMENT.md`
- `OPERATIONS.md`
- `PRODUCTION_READINESS.md`
- `NEXUS_INTEGRATION_AUDIT.md` (append current verification only)
- `DECISIONS.md`
- `COMMANDS.md`
- `.sqlx/` only if checked queries are introduced

## 7. Interfaces and Contracts

- `SessionStore::new(pool)` remains backward-compatible and defaults to fail-closed seed behavior.
- The migration-owned `development_seed` source is never accepted by `SessionStore`; local development uses the separately gated `Authorization: Bearer hydra-dev-admin` fixture.
- A seeded account is identified by the additive migration-owned source marker, not by a caller-supplied username, tenant, header, or token.
- Disabled seed credentials cannot authenticate or create sessions outside dev, and sessions belonging to a disabled seed cannot be looked up outside dev.
- Existing non-seed users with active credentials retain their current role and tenant behavior.
- The migration is additive and preserves the seed row; it must include a paired `-- revert:` note.

## 8. Milestones

### M1 - Activate and verify the fixed-credential finding

Record EP-023 to EP-024 transition, run preflight/state checks, and prove the unconditional seed plus unguarded login call path from repository evidence.

Validation: `bash scripts/check-execplan-state.sh` and `bash scripts/preflight.sh`

Expected: `execplan state: ok` and `preflight: ok`.

Recovery: if the state checker disagrees, repair the index/plan/checker before Rust edits; do not alter historical EP-010 claims.

### M2 - Additive auth-seed hardening migration

Add a source marker and disabled timestamp to `hydra_user`, disable only the known development seed by its migration-owned identity, and keep normal users active. Refresh migrations through TestDb.

Validation: `cargo test -p fabric --test auth_seed_hardening -- --nocapture`

Expected: migration-backed seed and active-user fixtures are available; no user rows are deleted.

Recovery: inspect schema/query errors and use a narrow Store/Fabric test; do not rewrite `0007_auth.sql` or erase the seed row.

### M3 - Seed-source filtered SessionStore

Filter the migration-owned seed source in both authentication and lookup. Keep the Kernel's existing dev bearer gate unchanged and add integration assertions for all-environment seed denial and active-user compatibility.

Validation: `cargo test -p fabric --test auth_seed_hardening -- --nocapture` and `cargo check -p hydra-kernel --tests --offline`

Expected: seed login/session is rejected in every environment; ordinary active users remain unaffected.

Recovery: keep the default constructor fail-closed and isolate any type/lifetime issue; never infer identity authority from a username, environment-only exception, or request header.

### M4 - Documentation and security gate reconciliation

Document the seeded-account boundary, owner-bootstrap limitation, migration behavior, and remaining staging gaps. Add the command and ADR-0034; append current audit/readiness status without rewriting baseline evidence.

Validation: `bash scripts/check-execplan-state.sh` and `bash scripts/preflight.sh`

Expected: `execplan state: ok` and `preflight: ok`.

Recovery: update the owning contract; do not claim owner custody, staging identity, or production readiness from local fixtures.

### M5 - Full local acceptance

Run focused auth tests, checked SQL metadata if needed, formatting/lint/security/dependency/integration/E2E gates, and the full verifier against explicitly isolated services. Complete only when the fixed seed is fail-closed outside dev and all repository gates pass.

Validation: explicit isolated `bash scripts/verify.sh`

Expected: terminal `verify: ok`; no production or staging action.

Recovery: follow AGENTS.md §7; preserve any external owner/staging evidence as open.

## 9. Concrete Steps

1. Activate EP-024 and validate state/preflight.
2. Add the additive user source/disable migration.
3. Implement the SessionStore seed-source policy; preserve the existing separately gated dev bearer fixture.
4. Add migration-backed tests for all-environment seed denial, active users, and existing disabled sessions.
5. Update security/deployment/readiness/audit/decision/command documentation.
6. Run focused and full isolated gates.
7. Reconcile EP-024 and leave EP-010 partial.

## 10. Validation and Acceptance

- The fixed development seed is not accepted by form login or session lookup in any environment.
- Explicit dev mode retains only the existing separately gated bearer fixture; it does not enable the seeded database account.
- Existing active non-seed users remain functional and tenant-scoped.
- The migration is additive, preserves the seed row, and has a revert note.
- No credential, owner secret, or bootstrap token is added to tracked files.
- `bash scripts/preflight.sh`, focused auth tests, and the isolated full verifier pass.
- The audit and readiness docs distinguish local evidence from owner-operated bootstrap/staging evidence.

## 11. Idempotence and Recovery

The migration uses `ADD COLUMN IF NOT EXISTS`, an idempotent constraint block, and a narrow deterministic update for the known seed marker; rerunning the SQL is safe. Session policy changes are constructor-driven and deterministic. If interrupted, resume the first unchecked milestone. Never delete or reset auth/CRM data to recover.

## 12. Progress

- [x] M1 - Fixed-credential finding verified and EP-024 activated (`bash scripts/check-execplan-state.sh` -> `execplan state: ok`; `bash scripts/preflight.sh` -> `preflight: ok`).
- [x] M2 - Additive seed-hardening migration and migration-backed tests complete (`cargo test -p fabric --test auth_seed_hardening --offline -- --nocapture` -> `cargo test: 2 passed`).
- [x] M3 - SessionStore seed-source filtering and Kernel bearer-boundary review complete (`cargo check -p hydra-kernel --tests --offline` -> zero errors; seed and active-user assertions pass).
- [x] M4 - Documentation, ADR, and state/test wiring complete (`bash scripts/check-execplan-state.sh` -> `execplan state: ok`; `bash scripts/preflight.sh` -> `preflight: ok`).
- [x] M5 - Full local acceptance and truthful reconciliation complete (isolated `bash scripts/verify.sh` -> exit 0 in 758.3s; required success markers emitted, including `verify: ok`).

## 13. Surprises & Discoveries

The historical migration hash does not validate the documented `hydra-dev` password. That makes re-enabling the database seed both unsafe and non-functional; the implementation permanently rejects the `development_seed` source and keeps the dev bearer fixture as the only local convenience path.

## 14. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-11 | Preserve the known seed row but permanently disable its credential path | Removing a historical auth row would erase evidence, while the documented password does not match its stored hash. A source marker plus disabled timestamp fails closed without destructive deletion; local development uses the separately gated bearer fixture. |
| 2026-08-11 | Keep owner/bootstrap provisioning out of EP-024 | A production owner setup operation needs a separate trust, custody, audit, and recovery contract; inventing one while fixing the seed would expand the security boundary unsafely. |
| 2026-08-11 | Keep the migration constraint creation idempotent | The additive migration uses a guarded `pg_constraint` check so rerun/recovery does not fail on an already-present source constraint. |

## 15. Outcomes & Retrospective

EP-024 is complete for the code-owned fixed-credential boundary. The additive migration preserves the historical seed row, `SessionStore` rejects its source in authentication and lookup, active operator credentials remain usable, and the separately gated dev bearer remains independent. EP-010 remains partial until real owner identity custody, staging identity/TLS, recovery drills, privacy/security review, and human sign-off exist.
