# EP-038 - Hermetic Smoke and Clean-Room Verification

Plan status: COMPLETE

## 1. Purpose / Big Picture

Make the required Kernel smoke gate hermetic. The current smoke test starts a
real Kernel against the caller's root `DATABASE_URL`, so `verify.sh` can fail
or pass based on hidden migration state. EP-038 gives the smoke harness a
unique Store-migrated schema, passes a schema-scoped connection URL to the
child, and cleans up every outcome while preserving the real health/readiness
and fail-closed admission paths.

## 2. Scope

Extend the existing Store testkit with a schema-scoped database URL helper,
make `smoke_healthz` own a disposable `TestDb` lifecycle, and update the
command/testing contracts. Add regression coverage for cleanup/error-safe
control flow through the existing real-child smoke assertions.

## 3. Non-goals

- No production database, migration rollback, or deployment.
- No changes to `/healthz`, `/readyz`, `/readyz/details`, rate limiting,
  authentication, or Kernel runtime behavior.
- No new dependency, migration, crate, shared test schema, or NATS mutation.
- No bypass of the Store-backed limiter, readiness checks, or real Kernel
  process.
- No unrelated verifier or CI refactor.

## 4. Context and Orientation

`scripts/smoke-test.sh` runs `crates/kernel/tests/smoke_healthz.rs` when no
external smoke URL is supplied. That test currently forwards the root
`DATABASE_URL` directly to the child. `store::TestDb` already creates a unique
schema and applies all migrations for integration tests, while SQLx
`PgConnectOptions` can serialize a connection URL with a restricted
`search_path`. The smallest correction is to reuse those existing contracts.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/specs/SPEC-018-hermetic-smoke-verification.md`
- `scripts/smoke-test.sh`
- `crates/kernel/tests/smoke_healthz.rs`
- `crates/store/src/testkit.rs`
- `crates/store/src/lib.rs`
- `crates/kernel/Cargo.toml`
- `crates/kernel/src/main.rs`
- `crates/fabric/src/rate.rs`
- `TESTING.md`
- `PRODUCTION_READINESS.md`

## 6. Files to Change (== Expected Changed Files)

- `.agent/specs/SPEC-018-hermetic-smoke-verification.md`
- `.agent/execplans/EP-038-hermetic-smoke-and-clean-room-verification.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `crates/store/src/testkit.rs`
- `crates/kernel/tests/smoke_healthz.rs`
- `COMMANDS.md`
- `TESTING.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`

## 7. Interfaces and Contracts

- `TestDb::scoped_database_url()` returns a SQLx URL using the unique test
  schema and `public`, preserving the caller's loopback connection authority.
- The smoke child receives only that scoped URL; its output remains redacted
  and must never include credentials.
- `TestDb::cleanup()` remains the only schema cleanup path and is invoked after
  child shutdown on every test outcome.
- Existing HTTP status/body assertions remain unchanged.

## 8. Milestones

### M1 - Contract and active-plan state

Add SPEC-018 and activate EP-038 in the status index and checker. Validate
with `bash scripts/preflight.sh` and `bash scripts/check-execplan-state.sh`;
expect `preflight: ok` and `execplan state: ok`.

### M2 - Schema-scoped Store URL

Add a validated testkit helper that serializes the existing connection
options with `search_path=<unique_schema>,public`. Keep credentials out of
logs. Validate with offline compilation and focused Store tests.

### M3 - Hermetic real-child smoke

Create `TestDb` in the smoke test, pass its scoped URL to Kernel, preserve the
NATS endpoint, and guarantee child/schema cleanup for all result paths. Run
the smoke test once against a fresh disposable database and once after a
second invocation to prove repeatability.

### M4 - Documentation and full gates

Document the clean-room smoke boundary and record the decision. Run format,
lint, typecheck, focused smoke, security, dependency, state, and full
verification against disposable loopback services. Do not count an external
smoke URL as evidence for the in-repo hermetic path.

## 9. Concrete Steps

1. Confirm SQLx's pinned `PgConnectOptions` URL serialization API and the
   existing TestDb schema lifecycle.
2. Add the smallest schema-scoped URL helper with no secret-bearing output.
3. Refactor the smoke test into an outcome-safe child/database lifecycle.
4. Run the focused smoke test against a fresh disposable database before any
   full gate.
5. Update docs and status only after the focused and full validations pass.

## 10. Validation and Acceptance

- `bash scripts/preflight.sh` -> `preflight: ok`.
- `bash scripts/check-execplan-state.sh` -> `execplan state: ok`.
- `cargo fmt --all -- --check` exits 0.
- Focused `smoke_healthz` passes with a fresh loopback database.
- Repeated focused smoke passes without root-schema migration state.
- `bash scripts/security-check.sh` -> `security check: ok`.
- `bash scripts/dependency-audit.sh` -> `dependency audit: ok`.
- `bash scripts/verify.sh` -> `verify: ok`.
- `git diff --check` exits 0.
- No credentials appear in smoke failures or child output.
- No production database, deployment, push, tag, or merge occurs.

## 11. Idempotence and Recovery

Each run creates a unique schema and applies migrations idempotently within
that schema. Cleanup is attempted after the child is stopped even when an
endpoint or assertion fails. If cleanup itself fails, the test reports that
failure rather than claiming a clean run. A rerun creates a different schema
and cannot reuse prior smoke state.

## 12. Progress

- [x] M1 - Contract and active-plan state (`preflight: ok`; `execplan state: ok`)
- [x] M2 - Schema-scoped Store URL (offline compile passed; SQLx URL option omission corrected)
- [x] M3 - Hermetic real-child smoke (fresh empty root database: 2/2 passes; cleanup verified by repeatability)
- [x] M4 - Documentation and full gates (`verify.sh` exited 0 through the documented `verify: ok` success path)

## 13. Surprises & Discoveries

Record exact API, process, cleanup, and gate findings here. Do not turn a
pre-existing root database into required evidence.

- SQLx 0.8.6 `PgConnectOptions::to_url_lossy()` omits the `options` startup
  field even when `PgConnectOptions::options()` was used. The child URL must
  therefore append the encoded `options=-c search_path=...` query explicitly.
- The first clean-room run reached the real Kernel but returned the expected
  fail-closed `503 Rate Limiter Unavailable`, proving the omitted URL option
  was material. After the URL fix, two runs passed against the fresh empty
  database.

## 14. Decision Log

Record decisions here as milestones execute. In particular, document why the
existing Store TestDb is reused and why health/readiness behavior is not
changed.

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-08-12 | Reuse `store::TestDb` and encode the schema-scoped startup option in the child URL | It already owns unique schema creation and embedded migrations; SQLx URL serialization omits startup options, so explicit bounded encoding is the smallest dependency-free correction. |
| 2026-08-12 | Keep health/readiness and Store-backed admission unchanged | The test must validate the real fail-closed runtime path rather than make smoke green by bypassing rate limiting or readiness. |

## 15. Outcomes & Retrospective

EP-038 is complete. `cargo fmt --all -- --check` and `git diff --check` passed;
the focused real-child smoke passed twice against a newly initialized empty
loopback PostgreSQL database without root migrations. The first run exposed
and the URL fix corrected the SQLx startup-option serialization defect. The
disposable database cluster was stopped and port 55438 was closed afterward.
The required gates also passed: `preflight: ok`, `execplan state: ok`,
`security check: ok`, `dependency audit: ok`, and the unchanged full
`bash scripts/verify.sh` exited 0 in 1016.8 seconds through its terminal
`verify: ok` path. The disposable cluster was stopped after validation.
EP-010 remains PARTIALLY PASSED because production/staging evidence is still
external.
