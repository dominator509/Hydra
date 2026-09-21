#!/usr/bin/env sh
# EP-020: Database restore to a generated ephemeral verification database.
#
# Usage:  scripts/db-restore.sh <dump-file>
#
# NEVER restores into the live database. The helper refuses production
# environment markers, requires HYDRA_RESTORE_CONFIRM=ephemeral, generates
# its own target database name, and removes that target on exit.
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

if [ "${HYDRA_RESTORE_CONFIRM:-}" != "ephemeral" ]; then
  echo "ERROR: set HYDRA_RESTORE_CONFIRM=ephemeral for a disposable restore check" >&2
  exit 1
fi

case "${HYDRA_ENV:-dev}" in
  prod|production)
    echo "ERROR: restore verification is refused when HYDRA_ENV is production" >&2
    exit 1
    ;;
esac

if ! pg_restore --list "$DUMP_FILE" >/dev/null; then
  echo "ERROR: dump archive validation failed" >&2
  exit 1
fi

TIMESTAMP=$(date -u +%Y%m%d%H%M%S)
RESTORE_DB="hydra_restore_check_${$}_${TIMESTAMP}"
CREATED=0

cleanup() {
  if [ "$CREATED" -eq 1 ]; then
    if ! dropdb --if-exists --maintenance-db=postgres "$RESTORE_DB"; then
      echo "ERROR: failed to remove ephemeral restore database: $RESTORE_DB" >&2
      return 1
    fi
  fi
}
trap cleanup EXIT HUP INT TERM

createdb --maintenance-db=postgres "$RESTORE_DB"
CREATED=1

if ! pg_restore \
  --exit-on-error \
  --single-transaction \
  --no-owner \
  --dbname="$RESTORE_DB" \
  "$DUMP_FILE"; then
  echo "ERROR: restore verification failed" >&2
  exit 1
fi

echo "restore: ok"
