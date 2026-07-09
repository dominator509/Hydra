#!/usr/bin/env sh
# EP-008: Database restore to a verification database.
#
# Usage:  scripts/db-restore.sh <dump-file>
#
# NEVER restores into the live database.  Always restores into a fresh
# database named hydra_restore_check (created on the fly) and validates
# the archive structure with pg_restore --no-owner.
#
# Dependencies:
#   - pg_restore, createdb, dropdb (PostgreSQL client tools)
#   - PGHOST / PGPORT / PGUSER / PGPASSWORD env vars (or ~/.pgpass)
#
# Docker: This script requires a running PostgreSQL instance.  Start deps with:
#   docker compose -f docker/compose.yaml up -d postgres

set -eu

if [ $# -lt 1 ]; then
  echo "Usage: $0 <dump-file>" >&2
  exit 1
fi

DUMP_FILE="$1"

if [ ! -f "$DUMP_FILE" ]; then
  echo "ERROR: dump file not found: $DUMP_FILE" >&2
  exit 1
fi

RESTORE_DB="hydra_restore_check"

# Drop and recreate the restore-check database to start clean.
dropdb --if-exists "$RESTORE_DB" 2>/dev/null || true
createdb "$RESTORE_DB"

pg_restore --no-owner --dbname="$RESTORE_DB" "$DUMP_FILE" 2>&1

echo "restore: ok"
