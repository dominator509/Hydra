#!/usr/bin/env sh
# Run the existing atomic backup helper at a bounded interval.
set -eu

INTERVAL="${HYDRA_BACKUP_INTERVAL_SECONDS:-86400}"
case "$INTERVAL" in
  ''|*[!0-9]*)
    echo "backup scheduler ERROR: HYDRA_BACKUP_INTERVAL_SECONDS must be an integer" >&2
    exit 1
    ;;
esac
if [ "$INTERVAL" -lt 60 ]; then
  echo "backup scheduler ERROR: HYDRA_BACKUP_INTERVAL_SECONDS must be at least 60" >&2
  exit 1
fi

BACKUP_SCRIPT="${HYDRA_BACKUP_SCRIPT:-/usr/local/bin/hydra-db-backup.sh}"
if [ ! -f "$BACKUP_SCRIPT" ]; then
  echo "backup scheduler ERROR: backup helper not found: $BACKUP_SCRIPT" >&2
  exit 1
fi

run_backup() {
  sh "$BACKUP_SCRIPT"
}

RETENTION_SCRIPT="${HYDRA_BACKUP_RETENTION_SCRIPT:-}"
if [ -n "$RETENTION_SCRIPT" ] && [ ! -f "$RETENTION_SCRIPT" ]; then
  echo "backup scheduler ERROR: retention helper not found: $RETENTION_SCRIPT" >&2
  exit 1
fi

run_retention() {
  if [ -n "$RETENTION_SCRIPT" ]; then
    sh "$RETENTION_SCRIPT"
  fi
}

run_cycle() {
  run_backup
  run_retention
}

if [ "${HYDRA_BACKUP_ONCE:-0}" = "1" ]; then
  run_cycle
  exit 0
fi

trap 'exit 0' HUP INT TERM
while :; do
  run_cycle
  sleep "$INTERVAL"
done
