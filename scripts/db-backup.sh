#!/usr/bin/env sh
# EP-008: Database backup — pg_dump with timestamped filename.
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
# Output:  backups/hydra_YYYYMMDD_HHMMSS.dump

set -eu

BACKUP_DIR="./backups"
mkdir -p "$BACKUP_DIR"

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
DUMP_FILE="${BACKUP_DIR}/hydra_${TIMESTAMP}.dump"

pg_dump -Fc \
  --no-owner \
  --no-acl \
  --file="$DUMP_FILE" 2>&1

echo "backup: ok"
