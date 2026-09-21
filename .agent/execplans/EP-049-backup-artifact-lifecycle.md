# EP-049 Backup Artifact Lifecycle

Plan status: COMPLETE

## 1. Purpose / Big Picture

Close the next code-owned recovery gap after EP-048 by making local Postgres
backup retention and encrypted-vault backup scheduling explicit, safe, and
testable. Preserve the distinction between locally generated artifacts and
operator-owned off-box durability, JetStream snapshot/restore, staging drills,
and production readiness.

## 2. Scope

- Activate EP-049 as the only active plan.
- Add SPEC-029 and state/index evidence.
- Add bounded retention and vault backup scheduler helpers.
- Add opt-in Compose profiles and operational safety tests.
- Update commands, operations, deployment, security, readiness, and ADR
  documentation with exact boundaries.
- Run the full repository verifier against disposable local services.

## 3. Non-goals

- No deletion of CRM rows, hard deletes, or production data.
- No off-box/cloud replication, WAL archiving, legal retention policy, or
  operator key-management service.
- No JetStream snapshot/restore implementation without a supported client or
  server API; no filesystem copy of a live NATS data directory.
- No production deployment, staging drill, push, tag, registry publication,
  or production database operation.
- No broad Compose or backup architecture rewrite.

## 4. Context and Orientation

EP-032 added an opt-in Postgres backup scheduler, but it writes indefinitely
to a local named volume and has no retention or vault scheduling. EP-046 added
validated ciphertext-preserving vault backup/restore, but backup remains an
explicit owner operation. The current async-nats dependency supports stream
configuration and acknowledged publishing, but no client-side snapshot/restore
API; JetStream recovery therefore remains an explicit deployment/operator gap.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/state/execplan-index.md`
- `SPEC-028-age-vault-dependency.md`
- `scripts/db-backup.sh`
- `scripts/backup-scheduler.sh`
- `scripts/test-operational-tools.sh`
- `crates/vault-cli/src/main.rs`
- `crates/bridge-host/src/vault.rs`
- `docker/Dockerfile`
- `docker/compose.yaml`
- `OPERATIONS.md`
- `DEPLOYMENT.md`
- `SECURITY.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`
- `.agent/specs/SPEC-029-backup-artifact-lifecycle.md`

## 6. Files to Change

- `scripts/backup-retention.sh`
- `scripts/vault-backup-scheduler.sh`
- `scripts/backup-scheduler.sh`
- `scripts/test-operational-tools.sh`
- `docker/compose.yaml`
- `COMMANDS.md`
- `OPERATIONS.md`
- `DEPLOYMENT.md`
- `SECURITY.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`
- `.agent/specs/SPEC-029-backup-artifact-lifecycle.md`
- `.agent/execplans/EP-049-backup-artifact-lifecycle.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`

## 7. Interfaces and Contracts

- `backup-retention.sh` accepts a configured directory, glob-safe artifact
  prefix/suffix, age/count limits, and an explicit apply acknowledgement.
- Preview is the default and reports candidates without deleting. Apply must
  require `HYDRA_BACKUP_RETENTION_CONFIRM=prune` and a positive explicit
  `HYDRA_BACKUP_RETENTION_APPLY=1`.
- `vault-backup-scheduler.sh` uses `HYDRA_VAULT_PATH`,
  `HYDRA_VAULT_KEY`, `HYDRA_VAULT_BACKUP_DIR`,
  `HYDRA_VAULT_BACKUP_INTERVAL_SECONDS`, and `HYDRA_VAULT_BACKUP_ONCE`.
- Vault destinations are generated under the configured backup directory and
  are passed to the existing `hydra-vault backup` binary; values and keys are
  never command arguments or output.
- The Postgres backup scheduler may invoke retention only when the operator
  has explicitly enabled apply mode; otherwise it remains preview-only.

## 8. Milestones

### M1 - Activate contract and state

Add SPEC-029, EP-049, the index row/transition, and state-checker coverage.
Run preflight and the state checker. Expected: `preflight: ok` and
`execplan state: ok`.

### M2 - Implement fail-closed artifact lifecycle

Add retention and vault scheduling helpers. Validate paths, filenames,
intervals, limits, confirmation, destination collisions, and subprocess
failures. Extend the operational harness with fake tools and secret-output
assertions. Expected: `operational tools: ok`.

### M3 - Wire opt-in profiles and policy documentation

Add a separate vault-backup Compose profile, optional retention invocation,
internal-only volumes/networks, command rows, and truthful operational/security
documentation. Validate shell syntax and Compose configuration.

### M4 - Focused safety validation

Run operational, preflight, security, dependency, and relevant vault tests.
Expected: all required success markers with no masking.

### M5 - Full acceptance and closeout

Run the resource-safe full verifier against disposable loopback Postgres and
JetStream, then the state checker and diff check. Expected: `verify: ok`,
`execplan state: ok`, and no production action.

## 9. Concrete Steps

1. Confirm EP-048 is complete and no plan is active.
2. Add SPEC-029 and activate EP-049 in the authoritative index.
3. Implement retention as a narrowly scoped artifact-file operation with
   preview default and explicit apply confirmation.
4. Implement the encrypted-vault scheduler by invoking the existing CLI and
   preserving its validation/secret boundary.
5. Extend tests and opt-in Compose wiring without changing standalone mode.
6. Update all operational and security documentation, including explicit
   JetStream/off-box limitations.
7. Run focused gates, then the full verifier, and close only on all signals.

## 10. Validation and Acceptance

EP-049 is accepted only when:

- exactly one active plan is represented and the state checker passes;
- retention preview cannot delete, apply requires explicit confirmation, and
  only matching artifacts under the configured directory are eligible;
- backup and vault schedulers fail closed on malformed configuration or helper
  failure and publish no partial artifact;
- vault backup output contains no secret or key material;
- opt-in Compose profiles validate and do not publish internal services;
- security, dependency, preflight, focused tests, and full verification pass;
- documentation does not claim off-box, JetStream snapshot, staging, or
  production recovery evidence; and
- no production action occurs.

## 11. Idempotence and Recovery

Repeated preview is read-only. Applying retention is deterministic for a
fixed directory and timestamp set. A vault backup collision fails without
overwriting the destination. Scheduler retries create a new timestamped
artifact and never replace an existing one. If any safety test fails, leave
EP-049 active and correct only the bounded helper or fixture.

## 12. Progress

- [x] M1 - Contract and active-plan state (`preflight: ok`; `execplan state: ok`, 2026-08-12)
- [x] M2 - Fail-closed artifact lifecycle (`operational tools: ok`; retention preview/apply, path safety, scheduler failure propagation, vault key non-disclosure, 2026-08-12)
- [x] M3 - Opt-in profiles and policy documentation (backup and vault-backup Compose config exited 0; shell syntax passed; commands, deployment, operations, security, readiness, and ADR boundaries updated, 2026-08-12)
- [x] M4 - Focused safety validation (`preflight: ok`; `security check: ok`; `dependency audit: ok`; hydra-vault library/recovery tests passed; `git diff --check` passed, 2026-08-12)
- [x] M5 - Full acceptance and closeout (resource-safe `bash scripts/verify.sh`: exit 0 in 717.5s through `verify: ok`; `execplan state: ok`, 2026-08-12)

## 13. Surprises & Discoveries

- The current async-nats client exposes stream configuration and publish/
  consumer operations but no snapshot/restore method; live NATS data-directory
  copying is intentionally out of scope.
- The runtime image already builds `hydra-vault`, so a separate opt-in vault
  backup profile can reuse the existing non-root image without a new package.
- The full verifier exercises the operational harness before Rust build and
  integration gates; it passed after the new retention and scheduler tests
  were added, with no production service started.

## 14. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-12 | Select a local artifact-lifecycle seam before external recovery work | Retention and vault scheduling are code-owned and testable without inventing cloud credentials or destructive staging operations. |
| 2026-08-12 | Preview is default; deletion requires two explicit controls | Backup cleanup is destructive to recovery artifacts and must fail closed under malformed or incomplete operator configuration. |
| 2026-08-12 | Do not copy the live JetStream data directory | The current client has no snapshot API and raw filesystem copying could produce an invalid or misleading recovery artifact. |

## 15. Outcomes & Retrospective

EP-049 is complete. Local Postgres artifact retention is preview-first with
two explicit apply controls, the encrypted-vault scheduler reuses the
validated CLI in a network-isolated profile, and operational/full repository
gates pass without secret disclosure or partial-artifact publication. No
production action occurred. Off-box replication, JetStream snapshot/restore,
key custody, legal retention policy, staging drills, and EP-010 human evidence
remain open.
