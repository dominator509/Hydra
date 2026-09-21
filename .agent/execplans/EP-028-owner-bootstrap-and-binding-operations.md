# EP-028 Owner Bootstrap and Binding Operations

Plan status: COMPLETE

## 1. Purpose / Big Picture

Provide a narrow, owner-operated control surface for the two durable
identities that the running Hydra/Nexus boundary depends on: active local
Hydra operators and explicit external business-to-Hydra tenant bindings.
Today Store has the binding records and the runtime can resolve them, but an
operator would need direct database access to create or revoke them. The
repository also has no supported way to create an active operator after the
historical development seed was disabled. Add a Rust-only `hydra-admin` CLI
and Store-owned operator repository without adding an HTTP authority path,
hard-delete behavior, or secrets to logs.

## 2. Scope

- Add Store-owned, parameterized operator-user creation, listing, and
  disable/enable operations.
- Hash passwords in the owner CLI before handing the encoded hash to Store;
  accept the password only from stdin and never print it.
- Add `hydra-admin user` commands for create, list, and status.
- Add `hydra-admin binding` commands for create and status changes using the
  existing additive binding table and soft status transitions.
- Require an explicit `HYDRA_ADMIN_CONFIRM=I_UNDERSTAND` environment value
  for every mutation command.
- Revoke active sessions when an operator is disabled.
- Keep all SQL in Store and keep the tool independent of the running Kernel.
- Add deterministic CLI parsing/secret-handling tests and isolated Store
  integration tests.
- Document bootstrap, confirmation, environment, recovery, and remaining
  owner custody requirements.

## 3. Non-goals

- No HTTP user-management endpoint or Nexus tool.
- No password reset email, identity-provider synchronization, or MFA system.
- No deletion of users, bindings, sessions, audit records, or CRM data.
- No automatic database migrations from the admin tool.
- No production deployment, staging mutation, push, merge, tag, or real
  production database operation.
- No change to Nexus authentication, Governor policy, MCP, REST, event, or
  adapter execution behavior.

## 4. Context and Orientation

Migration `0007_auth.sql` defines `hydra_user`, `hydra_role`, and sessions;
`0016_auth_seed_hardening.sql` preserves but disables the known development
seed. `crates/fabric/src/auth/password.rs` already provides Argon2id hashing,
and `crates/store/src/external_bindings.rs` already owns binding SQL and
disabled/revoked status semantics. The new CLI must use those boundaries
rather than issuing SQL itself or reviving the seed. A local owner tool is
intentionally separate from the authenticated Nexus control plane: it is a
bootstrap/recovery seam, not a general-purpose application API.

## 5. Files to Read First

`AGENTS.md`; `COMMANDS.md`; `.agent/PLANS.md`;
`.agent/EXECUTION_RULES.md`; `.agent/state/execplan-index.md`;
`migrations/0007_auth.sql`; `migrations/0008_external_tenant_binding.sql`;
`migrations/0016_auth_seed_hardening.sql`; `crates/store/src/lib.rs`;
`crates/store/src/external_bindings.rs`; `crates/fabric/src/auth/password.rs`;
`crates/fabric/src/auth/session.rs`; `crates/vault-cli/src/main.rs`;
`ENVIRONMENT.md`; `SECURITY.md`; `OPERATIONS.md`;
`PRODUCTION_READINESS.md`; `DECISIONS.md`.

## 6. Files to Change

- `.agent/execplans/EP-028-owner-bootstrap-and-binding-operations.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `Cargo.toml`
- `crates/admin-cli/Cargo.toml`
- `crates/admin-cli/src/main.rs`
- `crates/store/src/operators.rs`
- `crates/store/src/lib.rs`
- `crates/store/tests/operators.rs`
- `COMMANDS.md`
- `ENVIRONMENT.md`
- `SECURITY.md`
- `OPERATIONS.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`

## 7. Interfaces and Contracts

- Binary name: `hydra-admin`.
- `DATABASE_URL` selects an already-migrated database; the tool never runs
  migrations implicitly.
- Mutations require `HYDRA_ADMIN_CONFIRM=I_UNDERSTAND`.
- `hydra-admin user create TENANT_ID USERNAME ROLE DISPLAY_NAME` reads the
  password from stdin, creates an `operator` auth-source row, and adds one
  validated role. The output contains IDs and status only.
- `hydra-admin user list TENANT_ID` reports non-secret user metadata and
  status; it never reports password hashes or session tokens.
- `hydra-admin user status TENANT_ID USER_ID enabled|disabled` changes only
  `disabled_at`; disabling also revokes that user's sessions.
- `hydra-admin binding create PROVIDER EXTERNAL_TENANT EXTERNAL_BUSINESS
  HYDRA_TENANT_ID` creates an active binding through `ExternalBindingsRepo`.
- `hydra-admin binding status BINDING_ID active|disabled|revoked` uses the
  existing in-place status transition and never deletes a binding.
- Store validates tenant/user/binding identifiers, bounded text, supported
  roles/statuses, and Argon2id hash shape. The CLI validates confirmation and
  parses UUIDs before opening a mutation transaction.
- User creation is transactional: user and role are committed together;
  failure leaves no partial operator.

## 8. Milestones

### M1 - Activate plan and verify existing boundaries

Read the listed authority files, confirm the seed is disabled, confirm the
existing binding repository, add EP-028 to the state checker/index, and run
the state checker.

Validation: `bash scripts/check-execplan-state.sh`.

Expected: `execplan state: ok` with exactly one active plan.

Recovery: restore the state/index/plan status agreement before any code edit;
do not proceed with two active plans.

### M2 - Implement Store-owned operator operations and admin CLI

Add the Store repository, workspace crate, CLI parsing/confirmation/stdin
handling, and focused unit tests. Refresh SQLx metadata after the new queries.

Validation: `cargo fmt --all`; `cargo test -p hydra-admin --offline -- --nocapture`;
`cargo check -p hydra-admin --offline`.

Expected: formatting, CLI tests, and compile all pass; no password or token is
printed by tests.

Recovery: narrow to parser, password input, or Store query compilation; keep
the CLI from opening a database until confirmation and argument validation
have passed.

### M3 - Validate Store integration and fail-safe mutation behavior

Run the isolated Store operator suite against the loopback test database and
verify duplicate usernames, disabled-session revocation, role validation,
binding status changes, and no hard-delete path.

Validation: `cargo test -p store --test operators --offline -- --nocapture`
with `DATABASE_URL` set to the isolated loopback test database.

Expected: all operator tests pass and only test-scoped data is changed.

Recovery: use a fresh test database/schema; never target a shared or
production database and never remove failing tests.

### M4 - Reconcile docs and run full gates

Document the owner tool and its boundary, add the ADR, refresh checked SQLx
metadata if needed, run preflight/lint/security/dependency/full verification,
and record exact outputs and remaining owner custody gaps.

Validation: `bash scripts/preflight.sh`; `bash scripts/lint.sh`;
`bash scripts/security-check.sh`; `bash scripts/dependency-audit.sh`;
`bash scripts/verify.sh`; `bash scripts/check-execplan-state.sh`.

Expected: required markers include `preflight: ok`, `lint: ok`,
`security check: ok`, `dependency audit: ok`, `verify: ok`, and
`execplan state: ok`.

Recovery: apply the documented bounded retry rule; if a required gate cannot
run after recovery, stop with the exact external/toolchain blocker.

## 9. Concrete Steps

1. Activate EP-028 in the state index and checker, then run M1 validation.
2. Add the Store operator repository and admin CLI using existing workspace
   dependencies; do not add Node/npm or a second SQL path.
3. Add focused and isolated integration tests, refresh SQLx metadata, and
   execute M2/M3 validations.
4. Update command, environment, security, operations, readiness, and ADR
   documentation.
5. Run all M4 gates, reconcile the plan/index, and leave EP-010 partial.

## 10. Validation and Acceptance

- Exactly one active plan while EP-028 is in progress; final checker is
  green with no active plan after completion.
- `hydra-admin` compiles without a Node/npm dependency.
- Mutation commands fail before database access without the exact confirmation
  value.
- Passwords are read from stdin, Argon2id encoded, and absent from output,
  logs, durable event data, and test diagnostics.
- New operators use `auth_source=operator`; the historical seed remains
  disabled and no command can enable it.
- User creation is atomic, username uniqueness is enforced by the database,
  roles are validated, and disable revokes sessions without deleting users.
- Binding creation and status changes use Store and preserve the existing
  unique external identity and soft status semantics.
- SQLx metadata is current; focused tests, security/dependency gates, and
  `bash scripts/verify.sh` pass.
- No production deployment, real production database operation, or push
  occurred.

## 11. Idempotence and Recovery

User creation and binding creation are intentionally not silently idempotent:
reusing an existing username or external identity returns a conflict rather
than creating or mutating a different record. Status commands are repeatable
for the requested state; disabling repeatedly remains safe and revokes any
remaining sessions. If interrupted, rerun the relevant read/list command and
then the milestone validation. The CLI never runs migrations or performs
cleanup outside the requested tenant/user/binding row.

## 12. Progress

- [x] M1 - Existing owner-operation gap verified and EP-028 activated.
- [x] M2 - Store repository and `hydra-admin` CLI implemented and validated.
- [x] M3 - Isolated Store mutation and fail-safe behavior tests pass.
- [x] M4 - Documentation, full gates, and truthful reconciliation complete.

## 13. Surprises & Discoveries

The first draft used SQLx APIs that are not available in this vendored
workspace version. The implementation was narrowed to the repository's
existing query_as! and FromRow patterns, then checked SQLx metadata was
refreshed with the isolated loopback database. The focused Store test was
also strengthened after review to seed and verify real session revocation
rather than only checking disabled_at.

The first cold full verifier attempt reached the integration build but hit a
transient parallel MSVC link.exe failure. A single-target, one-job retry
linked successfully. A serialized full retry then timed out in the release
build, and concurrent smoke/cache checks timed out while sharing Cargo's
target directory. Rerunning the release build, smoke, cache, integration,
and E2E stages sequentially with the isolated environment passed. The final
warm normal-parallel full verifier passed.

## 14. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-11 | Use a local `hydra-admin` binary instead of HTTP provisioning | Bootstrap and binding custody must not widen the authenticated Nexus surface or create a remotely reachable authority path. |
| 2026-08-11 | Require `HYDRA_ADMIN_CONFIRM=I_UNDERSTAND` for mutations | Makes owner intent explicit and keeps accidental invocation fail closed. |
| 2026-08-11 | Keep SQL in Store and reuse Fabric's Argon2id helper | Preserves the six-layer import law and avoids a second password-hashing implementation. |

## 15. Outcomes & Retrospective

EP-028 is complete. Store owns operator SQL and reuses the existing binding
repository; the owner CLI uses one Tokio runtime, reads passwords from stdin,
requires explicit mutation confirmation, and prints only non-secret metadata.
Validation passed:

- cargo fmt --all
- cargo check -p hydra-admin --offline
- cargo test -p hydra-admin --offline -- --nocapture (4 passed)
- cargo test -p store --test operators --offline -- --nocapture against the
  isolated loopback database (3 passed)
- cargo sqlx prepare --workspace -- --all-targets
- bash scripts/preflight.sh -> preflight: ok
- bash scripts/lint.sh -> lint: ok
- bash scripts/security-check.sh -> security check: ok
- bash scripts/dependency-audit.sh -> dependency audit: ok
- bash scripts/test-integration.sh -> integration tests: ok and failure
  suites: ok
- bash scripts/test-e2e.sh -> e2e tests: ok (3 passed)
- bash scripts/build.sh -> build: ok
- bash scripts/smoke-test.sh -> smoke test: ok
- bash scripts/cache-hit-audit.sh -> cache-hit audit: ok (ratio=0.9717)
- bash scripts/verify.sh -> exit 0 in 343.1 seconds with verify: ok
- bash scripts/check-execplan-state.sh -> execplan state: ok

The transient link, timeout, and missing-environment diagnostics were
recovered without masking a required check. No production deployment, push,
merge, tag, or production database operation occurred. EP-010 remains
partial pending owner credential custody, staging identity/TLS, recovery,
operational drills, and human sign-off.
