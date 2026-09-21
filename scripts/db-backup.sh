#!/usr/bin/env sh
# EP-020: Database backup — atomic, archive-validated pg_dump.
#
# Usage:  scripts/db-backup.sh
#
# Dependencies:
#   - pg_dump (PostgreSQL client tools)
#   - PGHOST / PGPORT / PGDATABASE / PGUSER / PGPASSWORD env vars (or ~/.pgpass)
#
# Docker: This script requires a running PostgreSQL instance.  Start deps with:
#   docker compose -f docker/compose.yaml up -d postgres
#
# Output:  ${HYDRA_BACKUP_DIR:-backups}/hydra_YYYYMMDD_HHMMSS.dump

set -eu

umask 077

BACKUP_DIR="${HYDRA_BACKUP_DIR:-./backups}"
mkdir -p "$BACKUP_DIR"

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
DUMP_FILE="${BACKUP_DIR}/hydra_${TIMESTAMP}.dump"
TEMP_FILE="${DUMP_FILE}.tmp.$$"

if [ -e "$DUMP_FILE" ]; then
  echo "ERROR: backup path already exists: $DUMP_FILE" >&2
  exit 1
fi

cleanup() {
  if [ -e "$TEMP_FILE" ]; then
    rm -f "$TEMP_FILE"
  fi
}
trap cleanup EXIT HUP INT TERM

if ! pg_dump -Fc \
  --no-owner \
  --no-acl \
  --file="$TEMP_FILE"; then
  echo "ERROR: pg_dump failed; no backup was published" >&2
  exit 1
fi

if ! pg_restore --list "$TEMP_FILE" >/dev/null; then
  echo "ERROR: pg_restore archive validation failed; no backup was published" >&2
  exit 1
fi

if [ -e "$DUMP_FILE" ]; then
  echo "ERROR: backup path appeared during validation: $DUMP_FILE" >&2
  exit 1
fi

mv "$TEMP_FILE" "$DUMP_FILE"
trap - EXIT HUP INT TERM

echo "backup: ok"
