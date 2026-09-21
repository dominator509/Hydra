# EP-020 Operational Readiness and Recovery Safety

Plan status: COMPLETE

## 1. Purpose / Big Picture

Close the highest-value code-owned production-readiness gaps that remain after EP-019 without pretending that staging drills, external identity, or human launch evidence occurred. The kernel will expose a non-secret structured readiness view while preserving the existing `/healthz` and `/readyz` contracts. Operator backup and restore helpers will become atomic, explicitly test-scoped, and regression-tested without touching a live database. Required local gates will exercise these contracts.

## 2. Scope

- Add `/readyz/details` as a deterministic JSON readiness projection.
- Preserve `/healthz` and `/readyz` response compatibility.
- Make readiness fail closed when a configured adapter lifecycle cannot actually be constructed.
- Make PostgreSQL backup archives atomic and self-verified before publication.
- Make restore verification use a generated ephemeral database name, require an explicit confirmation token, refuse production environment markers, and clean up its own target.
- Add a shell-level operational-tools regression suite using fake PostgreSQL client binaries only.
- Make the operational-tools suite a required preflight/full-verifier gate and update the operator contracts.
- Keep EP-010 and `PRODUCTION_READINESS.md` explicitly partial; no staging evidence is fabricated.

## 3. Non-goals

- No production deployment, staging deployment, real production database operation, destructive drill, push, merge, tag, or release.
- No claim that D1-D5, the 24-hour soak, real TLS/IdP, human review, performance, accessibility, observability, restore, rollback, or launch sign-off has passed.
- No distributed rate-limit backend, retention scheduler, JetStream snapshot system, SBOM/signing system, or external binding bootstrap CLI in this plan.
- No breaking change to existing health endpoint bodies, public CRM routes, event contracts, or database schema.
- No new dependency or migration.

## 4. Context and Orientation

EP-010 remains `PARTIAL` in `.agent/state/execplan-index.md` and `PRODUCTION_READINESS.md`. EP-012 through EP-019 made the Nexus control plane, runtime wiring, vault, bridge lifecycle seam, events, and local E2E truthful, but the current `/readyz` surface only returns the first failed dependency as text and does not make configured runtime capability state visible. The current backup helper writes directly to its final path, and the restore helper drops/recreates a fixed database without requiring an explicit ephemeral-test acknowledgement. This plan addresses those local, code-owned risks while leaving operator-owned evidence open.

## 5. Files to Read First

`AGENTS.md`; `COMMANDS.md`; `.agent/PLANS.md`; `.agent/EXECUTION_RULES.md`; `.agent/state/execplan-index.md`; `PRODUCTION_READINESS.md`; `OPERATIONS.md`; `DEPLOYMENT.md`; `ENVIRONMENT.md`; `crates/kernel/src/main.rs`; `crates/kernel/src/config.rs`; `crates/kernel/src/event_status.rs`; `crates/kernel/src/runtime_services.rs`; `crates/kernel/tests/smoke_healthz.rs`; `scripts/db-backup.sh`; `scripts/db-restore.sh`; `scripts/preflight.sh`; `scripts/verify.sh`; `DECISIONS.md`.

## 6. Files to Change

- `.agent/execplans/EP-020-operational-readiness-and-recovery-safety.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `crates/kernel/src/main.rs`
- `crates/kernel/tests/smoke_healthz.rs`
- `scripts/db-backup.sh`
- `scripts/db-restore.sh`
- `scripts/test-operational-tools.sh`
- `scripts/preflight.sh`
- `scripts/verify.sh`
- `COMMANDS.md`
- `ENVIRONMENT.md`
- `OPERATIONS.md`
- `DEPLOYMENT.md`
- `PRODUCTION_READINESS.md`
- `NEXUS_INTEGRATION_AUDIT.md`
- `.agent/execplans/EP-010-production-readiness.md`
- `DECISIONS.md`

## 7. Interfaces and Contracts

- `GET /healthz` remains HTTP 200 with body `ok` when the process is alive.
- `GET /readyz` remains HTTP 200 with body `ok` when required dependencies are ready and returns the existing short dependency body on failure.
- `GET /readyz/details` returns JSON with `status` (`ready` or `not_ready`) and named checks for `postgres`, `nats`, `events`, and `bridge_lifecycle`. It must not return URLs, credentials, tokens, prompts, customer records, or filesystem paths.
- The `events` check is required only when Nexus integration is enabled. The `bridge_lifecycle` check is required only when `HYDRA_ADAPTERS_PATH` is configured; an unavailable configured lifecycle fails readiness rather than advertising a usable capability.
- `HYDRA_BACKUP_DIR` optionally selects the backup directory; the default remains `./backups`. A backup is written to a private temporary file, archive-validated with `pg_restore --list`, and atomically renamed only after validation.
- `scripts/db-restore.sh` accepts one archive path only. It requires `HYDRA_RESTORE_CONFIRM=ephemeral`, refuses `HYDRA_ENV=prod` or `production`, generates its own `hydra_restore_check_<pid>_<timestamp>` database name, uses the `postgres` maintenance database, restores with `--exit-on-error --single-transaction`, and removes the generated target on exit.
- `scripts/test-operational-tools.sh` uses only temporary directories and fake `pg_dump`, `pg_restore`, `createdb`, and `dropdb` commands. It never connects to PostgreSQL.
- The state checker validates EP-020's required sections and status in addition to EP-011 through EP-019.

## 8. Milestones

### M1 - Activate the plan and record the operational contract

Update the state checker, authoritative index, transition log, this plan, decision log, and affected readiness documentation. Keep EP-010's historical evidence intact and add only current-status appendices or corrections that are supported by the repository.

Validation: `bash scripts/check-execplan-state.sh`

Expected result: `execplan state: ok`.

Recovery: if the checker names a missing section, status, or duplicate active row, repair the index/plan pair with `apply_patch` and rerun the same command. Do not continue to code while state is invalid.

### M2 - Implement structured, fail-closed kernel readiness

Add the details route and readiness projection in the kernel. Preserve the short `/readyz` path and test both legacy and details responses, including the configured-but-unavailable bridge lifecycle case through focused unit/integration coverage.

Validation: `cargo test -p hydra-kernel --test smoke_healthz -- --nocapture`.

Expected result: the smoke test passes and confirms HTTP 200 `ok` for `/healthz`, `/readyz`, and a `ready` JSON `/readyz/details` response in the isolated local environment.

Recovery: first failure gets the smallest route/test correction; second same-root failure uses `cargo test -p hydra-kernel --bin hydra-kernel readyz -- --nocapture`; third failure is recorded in Surprises and the design is simplified without weakening fail-closed behavior.

### M3 - Harden and regression-test backup/restore helpers

Make backup publication atomic and archive-validated. Make restore explicit, unique-target, cleanup-safe, and production-marker refusing. Add the fake-client regression suite and make it assert both success and refusal paths.

Validation: `bash scripts/test-operational-tools.sh`.

Expected result: `operational tools: ok`.

Recovery: inspect the exact fake-client transcript, narrow the script behavior to the documented PostgreSQL flags, and rerun the suite. Never point the test at a real database.

### M4 - Make the new local checks mandatory and document operator behavior

Add the operational-tools suite to preflight and the full verifier. Update `COMMANDS.md`, `ENVIRONMENT.md`, `OPERATIONS.md`, `DEPLOYMENT.md`, `PRODUCTION_READINESS.md`, `NEXUS_INTEGRATION_AUDIT.md`, EP-010's current reality section, and `DECISIONS.md` so the new contract and remaining external gates are explicit.

Validation: `bash scripts/preflight.sh`.

Expected result: `preflight: ok`.

Recovery: if a documentation or script inventory check fails, update the owning command/documentation contract before rerunning preflight; do not remove the required check.

### M5 - Full local acceptance and diff review

Run the operational suite, focused kernel test, state checker, and the unchanged full verifier against loopback-only test services. Review the changed-file set against this plan and record exact outcomes and remaining EP-010 gaps.

Validation: `bash scripts/verify.sh`.

Expected result: terminal `verify: ok`; no production deployment or production database operation.

Recovery: follow AGENTS.md §7 bounded retry. If a required external service is unavailable, use the documented isolated local service path; stop only when the §4 external blocker condition is met and record exact evidence.

## 9. Concrete Steps

1. Activate EP-020 in the authoritative index and extend the state checker before touching runtime or scripts.
2. Implement readiness details with a shared internal check function so `/readyz` and `/readyz/details` cannot drift.
3. Add focused smoke assertions for both compatible and structured responses.
4. Harden backup and restore scripts with private temporary files, archive validation, explicit restore confirmation, unique target naming, and cleanup.
5. Add fake PostgreSQL client tests that cover success, archive-validation failure, missing confirmation, and production-marker refusal.
6. Add the operational test to `preflight.sh` and `verify.sh`.
7. Update operator and readiness documentation without changing the evidence boundary.
8. Run focused gates, then the full verifier, and record every result in Progress and Outcomes.

## 10. Validation and Acceptance

- `bash scripts/check-execplan-state.sh` prints `execplan state: ok`.
- `bash scripts/test-operational-tools.sh` prints `operational tools: ok` and never invokes a real PostgreSQL client.
- `bash scripts/preflight.sh` prints `preflight: ok` and requires the operational test artifact.
- `/healthz` and `/readyz` preserve the existing smoke contract.
- `/readyz/details` exposes only deterministic non-secret readiness data and accurately reports configured bridge lifecycle availability.
- `bash scripts/verify.sh` prints `verify: ok` without masked failures.
- The changed-file set is contained in section 6, or every justified formatter/generated file is recorded in the Decision Log.
- EP-010 remains partial and no staging/production evidence is relabeled as local proof.

## 11. Idempotence and Recovery

State edits are deterministic and the checker is read-only. Readiness is computed from current dependency/runtime state on every request. Backup reruns create a new timestamped archive and never publish an unvalidated partial file. Restore creates a unique generated database and removes only that generated name; rerunning after interruption does not target a prior restore database. The fake-client suite owns all temporary files and removes them on exit. If interrupted, inspect the index Progress checklist, rerun the first unchecked milestone, and never clean unrelated worktree changes.

## 12. Progress

- [x] M1 - Plan active, state checker/index, and operational contract recorded (`bash scripts/check-execplan-state.sh` -> `execplan state: ok`; 2026-08-11)
- [x] M2 - Structured kernel readiness implemented and tested (`cargo test -p hydra-kernel --bin hydra-kernel configured_unavailable_bridge_lifecycle_is_not_ready` -> 1 passed; isolated `smoke_healthz` -> 1 passed; 2026-08-11)
- [x] M3 - Atomic backup, ephemeral restore, and fake-client regression suite implemented (`sh -n` for all three helpers; `bash scripts/test-operational-tools.sh` -> `operational tools: ok`; 2026-08-11)
- [x] M4 - Required gates and operator documentation updated (`bash scripts/preflight.sh` -> `preflight: ok`; 2026-08-11)
- [x] M5 - Full local acceptance, diff review, and outcomes recorded (`cargo fmt --all -- --check`; readiness unit/smoke; `bash scripts/check-execplan-state.sh`; `git diff --check`; final `bash scripts/verify.sh` -> exit 0 with terminal `verify: ok` path in 277.3s; 2026-08-11)

## 13. Surprises & Discoveries

- 2026-08-11: The local Nexus/E2E gates are green, but the production-readiness gate correctly remains blocked on dated D1-D5 staging evidence.
- 2026-08-11: `/readyz` currently checks Postgres, NATS, and required event infrastructure but does not expose a structured diagnostic projection or configured lifecycle availability.
- 2026-08-11: The existing restore helper's fixed database name and unconditional drop/recreate behavior are not sufficient as an operator safety contract.
- 2026-08-11: The first focused smoke invocation used PowerShell job-style `^&` quoting and fell back to the test's `localhost:5432` default; the corrected explicit environment invocation passed against the isolated `127.0.0.1:55433` database and WSL JetStream endpoint.
- 2026-08-11: The first full verifier run exposed that Windows Git Bash resolved `find` to the Windows `find.exe` inside the fake-client test. The test now counts dump files with POSIX glob iteration and does not depend on the host `find` implementation.

## 14. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-11 | Keep `/readyz` text-compatible and add `/readyz/details` | Existing container/smoke consumers depend on `ok`; operators still need structured, non-secret diagnostics. |
| 2026-08-11 | Treat configured but unavailable bridge lifecycle as not ready | A configured capability must not be advertised while its handler/runtime is unavailable; standalone mode remains unaffected when it is not configured. |
| 2026-08-11 | Require an explicit ephemeral restore confirmation and generate the database name internally | The helper must fail closed against accidental live restore targets and must never drop a caller-selected database. |
| 2026-08-11 | Use fake PostgreSQL clients for script regression tests | The shell behavior can be validated without a shared or production database, preserving AGENTS.md §13. |
| 2026-08-11 | Treat the corrected explicit loopback environment as the M2 evidence | The failed first invocation did not exercise the implementation because its environment assignment was not applied; the rerun used only the isolated test services. |

## 15. Outcomes & Retrospective

Completed 2026-08-11. EP-020 added a shared readiness evaluator, a backward-compatible `/readyz` implementation, and a non-secret `/readyz/details` JSON projection covering Postgres, NATS, required events, and explicitly configured bridge lifecycle. Configured but unavailable bridge lifecycle now fails readiness instead of presenting a usable capability. The focused configured-unavailable unit test and isolated kernel smoke test both passed.

The backup helper now uses a private temporary archive, validates it with `pg_restore --list`, and atomically publishes only the validated file. The restore helper requires `HYDRA_RESTORE_CONFIRM=ephemeral`, refuses production environment markers, generates its own target database name, uses the `postgres` maintenance database, restores with `--exit-on-error --single-transaction`, and cleans up only its generated target. `bash scripts/test-operational-tools.sh` passed using fake PostgreSQL clients and no database connection.

The operational suite is now required by preflight and the full verifier. The final verifier passed with `preflight: ok`, `operational tools: ok`, `lint: ok`, `format check: ok`, `typecheck: ok`, workspace/unit/integration/E2E tests, `build: ok`, `security check: ok`, `dependency audit: ok`, smoke, cache-hit audit, and terminal `verify: ok`. `bash scripts/check-execplan-state.sh` and `git diff --check` also passed. The initial full run exposed and fixed a Windows Git Bash `find.exe` portability issue in the fake-client test; the corrected test is now POSIX-glob based.

EP-010 remains partial by design. No production deployment, production database operation, staging drill, real restore/rollback, JetStream or vault recovery exercise, 24-hour soak, live provider/IdP/TLS validation, human review, release provenance, or launch sign-off occurred. The worktree contained pre-existing EP-016 through EP-019 changes and generated SQLx/test artifacts; they were not reverted or misattributed as EP-020 changes.
