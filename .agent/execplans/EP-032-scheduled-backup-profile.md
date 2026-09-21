# EP-032 Scheduled Backup Profile

Plan status: COMPLETE

## 1. Purpose / Big Picture

Hydra has a safe, atomic `scripts/db-backup.sh` helper, but the reference
Compose topology never invokes it on a schedule. This leaves a code-owned
operational gap between a tested backup primitive and a deployable local
backup profile. This plan adds an optional scheduler that uses the existing
PostgreSQL client image and fails closed when a backup fails.

## 2. Scope

- Add a bounded shell scheduler around the existing backup helper.
- Add an explicit `backup` Compose profile with a named local backup volume.
- Keep the scheduler on `data-internal` only and publish no ports.
- Add deterministic once-mode and invalid-interval safety coverage.
- Update command, deployment, operations, and readiness documentation.

## 3. Non-goals

- No purge, retention, rotation, off-box copy, encryption policy, or legal
  retention decision.
- No JetStream snapshot, encrypted-vault backup, restore drill, rollback,
  staging deployment, production database operation, or human sign-off.
- No new Rust, Node/npm, scheduler dependency, image build, or database schema.
- No change to the existing atomic backup helper's archive format or safety
  confirmations.
- No claim that a local backup volume is sufficient for production disaster
  recovery.

## 4. Context and Orientation

`scripts/db-backup.sh` writes and validates a PostgreSQL custom archive before
publishing it atomically. `scripts/test-operational-tools.sh` already stubs
the PostgreSQL client tools and proves the helper's success and failure paths.
The Compose file has an `ops` migration profile but no scheduled backup
service. The scheduler must invoke only the existing helper, use a bounded
interval, exit on a failed backup so Compose restart policy can surface the
failure, and never call restore or delete archives.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/state/execplan-index.md`
- `scripts/db-backup.sh`
- `scripts/test-operational-tools.sh`
- `docker/compose.yaml`
- `DEPLOYMENT.md`
- `OPERATIONS.md`
- `PRODUCTION_READINESS.md`
- `scripts/preflight.sh`
- `scripts/verify.sh`

## 6. Files to Change

- `.agent/execplans/EP-032-scheduled-backup-profile.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `scripts/backup-scheduler.sh`
- `scripts/test-operational-tools.sh`
- `scripts/preflight.sh`
- `docker/compose.yaml`
- `COMMANDS.md`
- `DEPLOYMENT.md`
- `OPERATIONS.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`

## 7. Interfaces and Contracts

- The profile is enabled explicitly with `--profile backup`.
- `HYDRA_BACKUP_INTERVAL_SECONDS` defaults to 86400 and must be an integer
  of at least 60 seconds.
- `HYDRA_BACKUP_DIR` is `/backups` in the profile and is backed by the named
  `hydra-backups` volume.
- The scheduler invokes `HYDRA_BACKUP_SCRIPT`, defaulting to the mounted
  `/usr/local/bin/hydra-db-backup.sh`, and passes no customer data or secrets
  in arguments.
- `HYDRA_BACKUP_ONCE=1` performs one backup and exits; it exists for
  deterministic safety tests and is not set by the Compose service.
- A backup failure terminates the scheduler with a nonzero status. The
  profile's `restart: unless-stopped` makes repeated failure visible rather
  than silently continuing.
- The service has only `data-internal`, no host ports, and no NATS, ingress,
  events, proxy, or egress network access.
- The profile does not delete archives. Operators own backup-volume capacity,
  off-box replication, retention, and restore evidence.

## 8. Milestones

### M1 - Activate and baseline the scheduler contract

Activate EP-032 as the only active plan, add the index/checker coverage, and
run `bash scripts/check-execplan-state.sh` plus `bash scripts/preflight.sh`.

Expected output: `execplan state: ok` and `preflight: ok`.

Recovery: repair only state/index/plan consistency before touching scheduler
behavior.

### M2 - Implement scheduler and Compose profile

Add the scheduler, profile-gated service, named backup volume, interval
configuration, and data-only network boundary. Run shell syntax and the
profile Compose normalization command.

Expected output: shell syntax exits 0 and Compose exits 0.

Recovery: validate interval parsing and the normalized service boundary;
preserve the existing backup helper unchanged if the wrapper fails.

### M3 - Prove helper integration and update contracts

Extend the operational safety suite for once-mode success and invalid
interval failure. Update commands, deployment, operations, decisions, and
readiness documentation. Run `bash scripts/test-operational-tools.sh`,
`bash scripts/check-observability.sh`, and security checks.

Expected output: `operational tools: ok`, `observability policy: ok`, and
`security check: ok`.

Recovery: use the existing fake PostgreSQL clients; never run the scheduler
against a non-test database to debug a shell failure.

### M4 - Full acceptance and truthful completion

Run the mandatory repository gates, review changed files against section 6,
append exact evidence, mark EP-032 COMPLETE, and return to no active plan.

Expected output: required success markers, `verify: ok`, `execplan state: ok`,
and `git diff --check` exit 0.

Recovery: follow AGENTS.md section 7. Missing staging storage, off-box backup,
or restore credentials remain evidence gaps rather than reasons to weaken the
local safety contract.

## 9. Concrete Steps

1. Update the authoritative plan ledger and state checker.
2. Add interval validation, one-shot mode, signal handling, and fail-closed
   backup invocation.
3. Add the profile service, mounted helper scripts, named backup volume, and
   data-only network.
4. Extend fake-client operational tests and validate Compose/shell policy.
5. Document local scheduler use and the remaining off-box/recovery boundary.
6. Run focused checks and the full repository verifier.

## 10. Validation and Acceptance

- Exactly one EP-032 ACTIVE row exists during implementation and no ACTIVE row
  remains after completion.
- The scheduler rejects zero, non-numeric, and sub-60-second intervals.
- Once-mode invokes the existing helper exactly once and propagates failure.
- The Compose profile is normalized successfully, has no host ports, and
  grants the scheduler only `data-internal`.
- The backup profile never runs restore, purge, or deletion commands.
- Existing operational helper tests and all mandatory repository gates pass.
- EP-010 remains partial for off-box backup, restore, JetStream/vault recovery,
  staging drills, and human sign-off.

## 11. Idempotence and Recovery

Each scheduler iteration delegates atomic publication and archive validation
to `db-backup.sh`; a timestamp collision fails rather than overwriting an
archive. Restarting after a successful iteration creates a new timestamped
archive. Restarting after failure retries the next iteration through Compose,
but never reuses or mutates a partial archive. `HYDRA_BACKUP_ONCE=1` makes
focused tests finite and safe.

## 12. Progress

- [x] M1 - Activate and baseline the scheduler contract (`execplan state: ok`; `preflight: ok`)
- [ ] M2 - Implement scheduler and Compose profile
- [x] M3 - Prove helper integration and update contracts (`operational tools: ok`; shell syntax; `observability policy: ok`; `security check: ok`)
- [x] M4 - Full acceptance and truthful completion (`bash scripts/verify.sh` exit 0 in 328.7s through the terminal `verify: ok` path; state/diff checks pending final command)

## 13. Surprises & Discoveries

- The backup helper is already atomic and archive-validating; the missing
  seam is invocation, not a second backup implementation.
- The reference runtime image does not include PostgreSQL client tools, so
  the profile uses the pinned PostgreSQL image rather than expanding the
  Kernel image or adding a package-install step.

## 14. Decision Log

| Date | Context | Decision | Why |
|---|---|---|---|
| 2026-08-12 | A safe backup helper exists but no scheduled invocation exists | Add a separate profile using `postgres:16-alpine` client tools | Keeps the Kernel image minimal and uses the existing tested helper without new dependencies |
| 2026-08-12 | Retention and off-box destination are not specified | Do not add deletion, rotation, or remote-copy behavior | Prevents irreversible data loss and leaves policy/storage ownership explicit |
| 2026-08-12 | Scheduler wrapper tests passed with fake PostgreSQL clients | Keep once-mode test-only and delegate all archive behavior to `db-backup.sh` | Preserves one authoritative backup implementation and makes failures observable |
| 2026-08-12 | Full verifier completed | Close EP-032 with no active plan after the final state and diff checks | `verify.sh` exited 0 and exercised the scheduler safety suite; no production action occurred |

## 15. Outcomes & Retrospective

M1-M4 are complete. The scheduler validates bounded intervals, delegates
archive behavior to the existing atomic helper, supports deterministic
once-mode testing, and exits on helper failure. The backup Compose profile
normalizes with only `data-internal` access and no host ports. Shell syntax,
operational, security, preflight, focused profile, and full
`bash scripts/verify.sh` gates passed; the verifier exited 0 in 328.7 seconds
through its terminal `verify: ok` path. Final state and diff checks follow this
update. Off-box replication, retention policy, restore drills,
JetStream/vault recovery, staging evidence, and human EP-010 sign-off remain
open. No production deployment, push, tag, or production database operation
occurred.

Post-completion documentation reconciliation corrected the earlier readiness
wording that said scheduled backups were absent; retention/purge and recovery
gaps remain accurately open.
