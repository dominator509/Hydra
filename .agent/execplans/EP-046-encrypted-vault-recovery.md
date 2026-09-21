# EP-046 Encrypted Vault Recovery

Plan status: COMPLETE

## 1. Purpose / Big Picture

Close the code-owned encrypted-vault recovery gap by adding explicit,
owner-controlled backup and restore commands that preserve age ciphertext,
validate keys before publication, and fail closed on accidental or unconfirmed
restore operations. This plan provides local recovery mechanics only; it does
not claim off-box custody, staging evidence, or production readiness.

## 2. Scope

Extend the existing `EncryptedVault` boundary with verified ciphertext copy
operations; add `backup` and confirmation-gated `restore` to `hydra-vault`;
add focused tests; update command, environment, security, deployment,
operations, package, and readiness documentation; run all repository gates.

## 3. Non-goals

- No CRM, Postgres, NATS, migration, adapter, or Nexus protocol changes.
- No cloud/object-store integration, retention policy, key escrow, or vault
  HTTP endpoint.
- No automatic startup restore, secret rotation, or production operation.
- No replacement age format or new cryptographic dependency.
- No removal of the existing `set`, `get-names`, or `rotate` interface.

## 4. Context and Orientation

`crates/bridge-host/src/vault.rs` already validates bounded age documents and
uses atomic writes. `crates/vault-cli/src/main.rs` supports owner-controlled
set, names, and rotate but has no verified backup/restore command. Deployment
and package contracts explicitly state that encrypted-vault backup/restore
remains unproven. SPEC-026 is normative for this plan. The active state ledger
must contain only EP-046 as `ACTIVE`.

## 5. Files to Read First

`AGENTS.md`; `COMMANDS.md`; `.agent/PLANS.md`; `.agent/EXECUTION_RULES.md`;
`.agent/state/execplan-index.md`; `.agent/specs/SPEC-026-encrypted-vault-recovery.md`;
`Cargo.toml`; `crates/bridge-host/Cargo.toml`;
`crates/bridge-host/src/vault.rs`; `crates/bridge-host/src/lib.rs`;
`crates/vault-cli/Cargo.toml`; `crates/vault-cli/src/main.rs`;
`scripts/verify.sh`; `scripts/test-unit.sh`; `COMMANDS.md`; `ENVIRONMENT.md`;
`SECURITY.md`; `DEPLOYMENT.md`; `OPERATIONS.md`;
`NEXUS_PACKAGE_CONTRACT.md`; `PRODUCTION_READINESS.md`.

## 6. Files to Change

- `crates/bridge-host/src/vault.rs`
- `crates/bridge-host/src/lib.rs` only if the recovery API requires an export
- `crates/vault-cli/src/main.rs`
- `crates/vault-cli/Cargo.toml` only if an existing dependency is insufficient
- `crates/bridge-host/tests/vault_recovery.rs` if focused integration coverage
  is needed
- `COMMANDS.md`
- `ENVIRONMENT.md`
- `SECURITY.md`
- `DEPLOYMENT.md`
- `OPERATIONS.md`
- `NEXUS_PACKAGE_CONTRACT.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`
- `.agent/specs/SPEC-026-encrypted-vault-recovery.md`
- `.agent/execplans/EP-046-encrypted-vault-recovery.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`

## 7. Interfaces and Contracts

`hydra-vault backup <destination>` validates the configured source using
`HYDRA_VAULT_KEY` and atomically publishes an encrypted copy without
overwriting the source. `hydra-vault restore <source>` requires
`HYDRA_VAULT_RESTORE_CONFIRM=restore`, validates the source with
`HYDRA_VAULT_KEY`, and atomically publishes the ciphertext to the configured
active vault path. Both commands reject source/destination collisions and
never print values or keys. Existing commands retain their current syntax.

## 8. Milestones

### M1 - Activate and baseline

Add SPEC-026 and EP-046, update the state checker/index, and validate the
single-active invariant. Validation: `bash scripts/check-execplan-state.sh`.
Expected: `execplan state: ok` with EP-046 as the only active plan.
Recovery: repair the index/checker before changing code.

### M2 - Verified ciphertext recovery API

Implement bounded source validation, collision checks, ciphertext-preserving
atomic copy, and explicit error handling using the existing age and filesystem
boundary. Validation: `cargo test -p bridge-host vault -- --nocapture`.
Expected: round-trip, tamper/key failure, collision, and atomic-failure tests
pass without secret-shaped output. Recovery: narrow to a single API test; do
not weaken validation or expose plaintext.

### M3 - Owner CLI commands

Add `backup` and confirmation-gated `restore`, path resolution, bounded output,
and CLI tests. Validation: `cargo test -p hydra-vault -- --nocapture`.
Expected: command behavior and confirmation failures pass. Recovery: exercise
the binary with synthetic temporary files and preserve existing commands.

### M4 - Documentation and policy gates

Update command/environment/security/deployment/operations/package/readiness
docs and ADR-0057. Validation: `bash scripts/preflight.sh`,
`bash scripts/check-execplan-state.sh`, and `git diff --check`.
Expected: `preflight: ok`, `execplan state: ok`, and no diff errors.
Recovery: keep readiness partial if any operator-owned evidence remains absent.

### M5 - Full local acceptance

Run focused tests, security/dependency gates, and the unchanged full verifier
against disposable loopback services. Validation: `bash scripts/verify.sh`.
Expected: terminal `verify: ok`; no production database, deployment, push,
tag, or external credential operation. Recovery: follow AGENTS.md section 7
and leave EP-046 active until every required gate passes.

## 9. Concrete Steps

1. Activate EP-046 and validate the state ledger.
2. Add verified encrypted artifact copy operations and tests.
3. Add CLI backup/restore and tests.
4. Update all affected operator/security/readiness documentation and ADR.
5. Run focused gates, full verification, and reconcile the state ledger.

## 10. Validation and Acceptance

- Backup validates the source key and preserves ciphertext.
- Restore requires explicit confirmation and validates before atomic publish.
- No path collision or failed validation can replace the active artifact.
- No secret values, passphrases, or raw encrypted payloads appear in output.
- Existing vault commands and Kernel `SecretSource` behavior remain green.
- Focused tests, preflight, state, diff, security, dependency, and full verify
  pass with their required markers.
- Remaining owner custody, off-box, staging, and EP-010 evidence stays open.

## 11. Idempotence and Recovery

Repeated backup to the same destination is rejected rather than silently
overwritten. Repeated restore from a validated artifact is safe and atomic;
the output ciphertext remains byte-identical because recovery copies the
validated artifact rather than decrypting and re-encrypting it. A failed
validation or write leaves the active vault untouched. Tests use unique
temporary paths and synthetic keys only.

## 12. Progress

- [x] M1 - EP-046 activated and state validation passed.
- [x] M2 - Verified ciphertext recovery API (`cargo test -p bridge-host vault -- --nocapture` -> `cargo test: 6 passed`, 2026-08-12).
- [x] M3 - Owner CLI commands (`cargo test -p hydra-vault -- --nocapture` -> `cargo test: 2 passed`, including the real backup/restore binary path, 2026-08-12).
- [x] M4 - Documentation and policy gates (`preflight: ok`, `execplan state: ok`, security/dependency markers passed, and `git diff --check` passed, 2026-08-12).
- [x] M5 - Full local acceptance (`bash scripts/verify.sh` exited 0 in 612.6 seconds through the terminal `verify: ok` path using disposable loopback services and the reduced-debug profile, 2026-08-12).

## 13. Surprises & Discoveries

Record exact filesystem, platform-permission, CLI, and validation findings.
Do not count a command as passed if it only checks source text or skips the
new recovery behavior.

- 2026-08-12: The existing atomic writer already creates parent directories
  and uses platform-specific replace semantics. Backup adds a non-replacing
  publication path; Unix uses an atomic hard-link publication so a racing
  existing destination cannot be replaced.
- 2026-08-12: The real CLI integration test is available through Cargo's
  `CARGO_BIN_EXE_hydra-vault` fixture and proves confirmation failure,
  ciphertext preservation, and absence of the synthetic secret in output.
- 2026-08-12: The unchanged full verifier completed with exit code 0 after
  including the new BridgeHost vault tests and all existing workspace gates.
  The command used `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0` because the
  earlier full-debug run hit a Windows linker disk-pressure failure; no source
  or tracked files were removed.

## 14. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-12 | Copy validated ciphertext instead of decrypting/re-encrypting during backup or restore | Preserves the owner-encrypted artifact exactly, limits plaintext exposure, and reuses the existing atomic file boundary. |
| 2026-08-12 | Require `HYDRA_VAULT_RESTORE_CONFIRM=restore` for restore | A vault restore changes the active secret artifact and must be an explicit owner operation rather than an accidental CLI invocation. |

## 15. Outcomes & Retrospective

EP-046 is complete. Hydra now has locally tested owner-controlled
`hydra-vault backup` and confirmation-gated `restore` commands that validate
age artifacts, preserve ciphertext, reject collisions, and publish atomically
without printing secret values. Existing vault loading, rotation, and Kernel
SecretSource behavior remain green. Owner-key custody, off-box protection,
capacity/retention, JetStream snapshots, staged restore timing, and EP-010
human evidence remain open. No production deployment, database operation,
push, or tag occurred.
