#!/usr/bin/env sh
# EP-020/EP-032: regression tests for backup/restore and scheduler safety contracts.
set -eu

[ -f AGENTS.md ] || {
  echo "operational tools ERROR: run from repository root." >&2
  exit 1
}

ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
TMP_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/hydra-ops.XXXXXX")
STUB_BIN="$TMP_ROOT/bin"
LOG_FILE="$TMP_ROOT/fake-postgres.log"
CREATED_DB_FILE="$TMP_ROOT/created-db"

cleanup() {
  rm -rf "$TMP_ROOT"
}
trap cleanup EXIT HUP INT TERM

mkdir -p "$STUB_BIN"
: > "$LOG_FILE"

cat > "$STUB_BIN/postgres-client" <<'EOF'
#!/usr/bin/env sh
set -eu

NAME=$(basename "$0")
printf '%s' "$NAME" >> "$HYDRA_FAKE_LOG"
for ARG in "$@"; do
  printf ' %s' "$ARG" >> "$HYDRA_FAKE_LOG"
done
printf '\n' >> "$HYDRA_FAKE_LOG"

case "$NAME" in
  pg_dump)
    if [ "${HYDRA_FAKE_DUMP_FAIL:-0}" = "1" ]; then
      exit 1
    fi
    OUTPUT=""
    for ARG in "$@"; do
      case "$ARG" in
        --file=*) OUTPUT=${ARG#--file=} ;;
      esac
    done
    [ -n "$OUTPUT" ] || exit 1
    printf 'fake archive\n' > "$OUTPUT"
    ;;
  pg_restore)
    if [ "${HYDRA_FAKE_RESTORE_FAIL:-0}" = "1" ]; then
      exit 1
    fi
    ARCHIVE=""
    for ARG in "$@"; do
      case "$ARG" in
        --list|--dbname=*) ;;
        -*) ;;
        *) ARCHIVE=$ARG ;;
      esac
    done
    [ -n "$ARCHIVE" ] && [ -f "$ARCHIVE" ]
    ;;
  createdb)
    if [ "${HYDRA_FAKE_CREATEDB_FAIL:-0}" = "1" ]; then
      exit 1
    fi
    DB=""
    for ARG in "$@"; do
      case "$ARG" in
        --*) ;;
        *) DB=$ARG ;;
      esac
    done
    [ -n "$DB" ]
    printf '%s' "$DB" > "$HYDRA_FAKE_CREATED_DB"
    ;;
  dropdb)
    DB=""
    for ARG in "$@"; do
      case "$ARG" in
        --*) ;;
        *) DB=$ARG ;;
      esac
    done
    [ -n "$DB" ]
    ;;
  *)
    exit 1
    ;;
esac
EOF

for CLIENT in pg_dump pg_restore createdb dropdb; do
  cp "$STUB_BIN/postgres-client" "$STUB_BIN/$CLIENT"
  chmod 700 "$STUB_BIN/$CLIENT"
done

cat > "$STUB_BIN/hydra-vault" <<'EOF'
#!/usr/bin/env sh
set -eu
[ "${1:-}" = "backup" ] && [ -n "${2:-}" ]
[ "${HYDRA_FAKE_VAULT_FAIL:-0}" = "1" ] && exit 1
printf 'encrypted vault artifact\n' > "$2"
printf 'fake helper output key=%s\n' "$HYDRA_VAULT_KEY"
EOF
chmod 700 "$STUB_BIN/hydra-vault"

fail() {
  echo "operational tools ERROR: $1" >&2
  exit 1
}

run_with_stubs() {
  HYDRA_FAKE_LOG="$LOG_FILE" \
  HYDRA_FAKE_CREATED_DB="$CREATED_DB_FILE" \
  PATH="$STUB_BIN:$PATH" \
  "$@"
}

BACKUP_DIR="$TMP_ROOT/backups"
BACKUP_OUTPUT="$TMP_ROOT/backup.out"
if ! run_with_stubs env HYDRA_BACKUP_DIR="$BACKUP_DIR" sh "$ROOT/scripts/db-backup.sh" > "$BACKUP_OUTPUT"; then
  fail "atomic backup success path failed"
fi

BACKUP_COUNT=0
for BACKUP_FILE in "$BACKUP_DIR"/*.dump; do
  if [ -f "$BACKUP_FILE" ]; then
    BACKUP_COUNT=$((BACKUP_COUNT + 1))
  fi
done
[ "$BACKUP_COUNT" = "1" ] || fail "expected one published backup, found $BACKUP_COUNT"
grep -q '^backup: ok$' "$BACKUP_OUTPUT" || fail "backup success marker missing"

: > "$LOG_FILE"
if run_with_stubs env HYDRA_BACKUP_DIR="$TMP_ROOT/invalid-backups" HYDRA_FAKE_RESTORE_FAIL=1 sh "$ROOT/scripts/db-backup.sh" >/dev/null 2>&1; then
  fail "archive validation failure was accepted"
fi
INVALID_COUNT=0
for INVALID_FILE in "$TMP_ROOT/invalid-backups"/*.dump; do
  if [ -f "$INVALID_FILE" ]; then
    INVALID_COUNT=$((INVALID_COUNT + 1))
  fi
done
[ "$INVALID_COUNT" = "0" ] || fail "invalid archive was published"

RESTORE_ARCHIVE="$TMP_ROOT/input.dump"
printf 'fake archive\n' > "$RESTORE_ARCHIVE"

: > "$LOG_FILE"
if run_with_stubs env HYDRA_ENV=dev HYDRA_RESTORE_CONFIRM= sh "$ROOT/scripts/db-restore.sh" "$RESTORE_ARCHIVE" >/dev/null 2>&1; then
  fail "restore ran without explicit ephemeral confirmation"
fi
if grep -q '^createdb ' "$LOG_FILE"; then
  fail "restore invoked createdb before confirmation"
fi

: > "$LOG_FILE"
if run_with_stubs env HYDRA_ENV=production HYDRA_RESTORE_CONFIRM=ephemeral sh "$ROOT/scripts/db-restore.sh" "$RESTORE_ARCHIVE" >/dev/null 2>&1; then
  fail "production-marked restore was accepted"
fi
if grep -q '^createdb ' "$LOG_FILE"; then
  fail "production-marked restore invoked createdb"
fi

: > "$LOG_FILE"
RESTORE_OUTPUT="$TMP_ROOT/restore.out"
if ! run_with_stubs env HYDRA_ENV=dev HYDRA_RESTORE_CONFIRM=ephemeral sh "$ROOT/scripts/db-restore.sh" "$RESTORE_ARCHIVE" > "$RESTORE_OUTPUT"; then
  fail "ephemeral restore success path failed"
fi
CREATED_DB=$(sed -n '1p' "$CREATED_DB_FILE")
case "$CREATED_DB" in
  hydra_restore_check_[0-9]*_[0-9]*) ;;
  *) fail "restore target was not generated safely: $CREATED_DB" ;;
esac
if ! grep -q "^createdb --maintenance-db=postgres $CREATED_DB$" "$LOG_FILE"; then
  fail "createdb did not use the generated target and maintenance database"
fi
if ! grep -q "^dropdb --if-exists --maintenance-db=postgres $CREATED_DB$" "$LOG_FILE"; then
  fail "restore target cleanup was not recorded"
fi
grep -q '^restore: ok$' "$RESTORE_OUTPUT" || fail "restore success marker missing"

SCHEDULED_BACKUP_DIR="$TMP_ROOT/scheduled-backups"
SCHEDULER_OUTPUT="$TMP_ROOT/scheduler.out"
if ! run_with_stubs env \
  HYDRA_BACKUP_DIR="$SCHEDULED_BACKUP_DIR" \
  HYDRA_BACKUP_SCRIPT="$ROOT/scripts/db-backup.sh" \
  HYDRA_BACKUP_ONCE=1 \
  HYDRA_BACKUP_INTERVAL_SECONDS=60 \
  sh "$ROOT/scripts/backup-scheduler.sh" > "$SCHEDULER_OUTPUT"; then
  fail "backup scheduler once-mode success path failed"
fi
SCHEDULED_COUNT=0
for SCHEDULED_FILE in "$SCHEDULED_BACKUP_DIR"/*.dump; do
  if [ -f "$SCHEDULED_FILE" ]; then
    SCHEDULED_COUNT=$((SCHEDULED_COUNT + 1))
  fi
done
[ "$SCHEDULED_COUNT" = "1" ] || fail "scheduler should publish one archive in once-mode"
grep -q '^backup: ok$' "$SCHEDULER_OUTPUT" || fail "scheduler backup success marker missing"

RETENTION_DIR="$TMP_ROOT/retention"
mkdir -p "$RETENTION_DIR"
printf 'old\n' > "$RETENTION_DIR/hydra_20240101_000000.dump"
printf 'middle\n' > "$RETENTION_DIR/hydra_20240102_000000.dump"
printf 'new\n' > "$RETENTION_DIR/hydra_20240103_000000.dump"
printf 'keep\n' > "$RETENTION_DIR/not-a-hydra-backup.dump"

RETENTION_OUTPUT="$TMP_ROOT/retention.out"
if ! run_with_stubs env \
  HYDRA_BACKUP_RETENTION_DIR="$RETENTION_DIR" \
  HYDRA_BACKUP_RETENTION_COUNT=1 \
  HYDRA_BACKUP_RETENTION_DAYS=0 \
  sh "$ROOT/scripts/backup-retention.sh" > "$RETENTION_OUTPUT"; then
  fail "retention preview failed"
fi
grep -q '^backup retention: preview (2 candidate(s)); no files deleted$' "$RETENTION_OUTPUT" || fail "retention preview marker missing"
[ -f "$RETENTION_DIR/hydra_20240101_000000.dump" ] || fail "retention preview deleted an artifact"
[ -f "$RETENTION_DIR/not-a-hydra-backup.dump" ] || fail "retention preview touched an unrelated file"

if run_with_stubs env \
  HYDRA_BACKUP_RETENTION_DIR="$RETENTION_DIR" \
  HYDRA_BACKUP_RETENTION_COUNT=1 \
  HYDRA_BACKUP_RETENTION_APPLY=1 \
  sh "$ROOT/scripts/backup-retention.sh" >/dev/null 2>&1; then
  fail "retention apply ran without explicit confirmation"
fi

if ! run_with_stubs env \
  HYDRA_BACKUP_RETENTION_DIR="$RETENTION_DIR" \
  HYDRA_BACKUP_RETENTION_COUNT=1 \
  HYDRA_BACKUP_RETENTION_APPLY=1 \
  HYDRA_BACKUP_RETENTION_CONFIRM=prune \
  sh "$ROOT/scripts/backup-retention.sh" > "$TMP_ROOT/retention-apply.out"; then
  fail "retention apply failed with explicit confirmation"
fi
grep -q '^backup retention: applied (2 artifact(s))$' "$TMP_ROOT/retention-apply.out" || fail "retention apply marker missing"
[ ! -e "$RETENTION_DIR/hydra_20240101_000000.dump" ] || fail "old artifact was not pruned"
[ -f "$RETENTION_DIR/hydra_20240103_000000.dump" ] || fail "newest artifact was pruned"
[ -f "$RETENTION_DIR/not-a-hydra-backup.dump" ] || fail "retention removed an unrelated file"

VAULT_SOURCE="$TMP_ROOT/vault.age"
VAULT_BACKUP_DIR="$TMP_ROOT/vault-backups"
printf 'encrypted source\n' > "$VAULT_SOURCE"
VAULT_OUTPUT="$TMP_ROOT/vault-scheduler.out"
if ! run_with_stubs env \
  HYDRA_VAULT_PATH="$VAULT_SOURCE" \
  HYDRA_VAULT_KEY='test-vault-key-never-print' \
  HYDRA_VAULT_BACKUP_DIR="$VAULT_BACKUP_DIR" \
  HYDRA_VAULT_BACKUP_COMMAND="$STUB_BIN/hydra-vault" \
  HYDRA_VAULT_BACKUP_ONCE=1 \
  HYDRA_VAULT_BACKUP_INTERVAL_SECONDS=60 \
  sh "$ROOT/scripts/vault-backup-scheduler.sh" > "$VAULT_OUTPUT"; then
  fail "vault backup scheduler once-mode failed"
fi
grep -q '^vault backup: ok$' "$VAULT_OUTPUT" || fail "vault backup success marker missing"
if grep -q 'test-vault-key-never-print' "$VAULT_OUTPUT"; then
  fail "vault scheduler leaked key material"
fi
VAULT_BACKUP_COUNT=0
for VAULT_BACKUP in "$VAULT_BACKUP_DIR"/*.age; do
  if [ -f "$VAULT_BACKUP" ]; then
    VAULT_BACKUP_COUNT=$((VAULT_BACKUP_COUNT + 1))
  fi
done
[ "$VAULT_BACKUP_COUNT" = "1" ] || fail "vault scheduler should publish one artifact"

if run_with_stubs env \
  HYDRA_VAULT_PATH="$VAULT_SOURCE" \
  HYDRA_VAULT_KEY='test-vault-key-never-print' \
  HYDRA_VAULT_BACKUP_DIR="$TMP_ROOT/failed-vault-backups" \
  HYDRA_VAULT_BACKUP_COMMAND="$STUB_BIN/hydra-vault" \
  HYDRA_VAULT_BACKUP_ONCE=1 \
  HYDRA_VAULT_BACKUP_INTERVAL_SECONDS=60 \
  HYDRA_FAKE_VAULT_FAIL=1 \
  sh "$ROOT/scripts/vault-backup-scheduler.sh" >/dev/null 2>&1; then
  fail "vault scheduler swallowed helper failure"
fi

if run_with_stubs env \
  HYDRA_BACKUP_SCRIPT="$ROOT/scripts/db-backup.sh" \
  HYDRA_BACKUP_ONCE=1 \
  HYDRA_BACKUP_INTERVAL_SECONDS=59 \
  sh "$ROOT/scripts/backup-scheduler.sh" >/dev/null 2>&1; then
  fail "scheduler accepted an interval below the safety minimum"
fi

if run_with_stubs env \
  HYDRA_BACKUP_DIR="$TMP_ROOT/failed-scheduled-backups" \
  HYDRA_BACKUP_SCRIPT="$ROOT/scripts/db-backup.sh" \
  HYDRA_BACKUP_ONCE=1 \
  HYDRA_BACKUP_INTERVAL_SECONDS=60 \
  HYDRA_FAKE_DUMP_FAIL=1 \
  sh "$ROOT/scripts/backup-scheduler.sh" >/dev/null 2>&1; then
  fail "scheduler swallowed backup failure"
fi

echo "operational tools: ok"
