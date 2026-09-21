# SPEC-026 Encrypted Vault Recovery

Status: ACCEPTED for EP-046

## Purpose

Define the owner-controlled recovery seam for Hydra's age-encrypted named
secret vault. This specification covers local/operator tooling only; it does
not create a secret-management service or claim that a staging or production
restore drill has occurred.

## Requirements

1. The active vault remains an age-encrypted artifact. Backup and restore MUST
   validate the artifact with the supplied vault key before publishing it.
2. Backup MUST preserve ciphertext and MUST NOT print, serialize, or log
   decrypted secret values.
3. Backup publication MUST be atomic, owner-readable only where the platform
   supports file permissions, and must not overwrite the source artifact.
4. Restore MUST require an explicit operator confirmation environment value,
   validate the source artifact before changing the active path, and publish
   atomically so a failed restore leaves the active vault unchanged.
5. Restore MUST not target a database, alter CRM state, rotate the owner key,
   or bypass the existing `SecretSource` grant boundary.
6. The CLI MUST accept secret values only through stdin for existing `set`
   behavior. Backup and restore arguments may contain paths but never secret
   values or passphrases.
7. Wrong keys, tampered artifacts, missing artifacts, source/destination path
   collisions, absent confirmation, and invalid output paths MUST fail closed
   with bounded errors that do not disclose secret contents.
8. Existing `set`, `get-names`, `rotate`, kernel loading, and standalone mode
   behavior MUST remain compatible.
9. Tests MUST use synthetic temporary artifacts and prove round-trip,
   ciphertext preservation, wrong-key/tamper rejection, atomic failure
   behavior, explicit confirmation, and no secret-shaped output.
10. Documentation MUST distinguish local executable recovery evidence from the
    still-required owner-key custody, off-box storage, staging restore drill,
    and production-readiness evidence.

## Non-goals

- Off-box replication, retention scheduling, cloud object storage, or key
  escrow.
- Automatic restore on startup or any production database operation.
- Secret rotation as part of backup or restore.
- Printing secret values or exposing a vault HTTP endpoint.

## Acceptance

- `cargo test -p bridge-host vault -- --nocapture` passes the recovery tests.
- `cargo test -p hydra-vault -- --nocapture` passes CLI argument/confirmation
  tests.
- `bash scripts/preflight.sh` and `bash scripts/check-execplan-state.sh`
  pass.
- `bash scripts/verify.sh` exits through `verify: ok` using disposable local
  services.
- Deployment, environment, commands, security, and readiness documentation
  accurately describe the new tooling and remaining operator-owned gaps.

