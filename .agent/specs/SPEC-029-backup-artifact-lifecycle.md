# SPEC-029 Backup Artifact Lifecycle

Status: ACCEPTED for EP-049 implementation.

## Purpose

Define the narrow, local recovery-artifact boundary Hydra can implement
without pretending to provide off-box durability or a JetStream snapshot API.
Postgres dumps and age-encrypted vault copies are recovery artifacts, not
canonical CRM state. Postgres, the append-only audit/outbox, and JetStream
remain authoritative for their respective responsibilities.

## Normative requirements

1. Postgres backup publication remains atomic and archive-validated.
2. Retention operates only on files in an explicitly configured directory and
   only on Hydra-generated filename patterns. It must never delete database
   rows, vault source files, JetStream data, or arbitrary directory contents.
3. Retention is preview-only unless an explicit apply acknowledgement is
   present. Invalid intervals, directories, patterns, and limits fail closed.
4. Scheduled vault backups must call the existing `hydra-vault backup`
   operation, keep the age key in the environment, never print secret values,
   refuse destination collisions, and preserve the active encrypted source.
5. A scheduler must fail if its first backup or retention operation fails; it
   may not convert a failed recovery artifact into a successful health signal.
6. Standalone mode remains unchanged. Backup profiles are opt-in and do not
   receive public ports or unrestricted network access.
7. Off-box copy, key custody, WAL, JetStream snapshot/restore, retention
   policy approval, and staging recovery evidence are not claimed by this
   specification.

## Acceptance

The operational test harness proves preview/apply boundaries, filename and
path safety, scheduler failure propagation, vault artifact validation, and
secret non-disclosure. Shell syntax, Compose profiles, preflight, security,
dependency, and the full repository verifier must pass.
