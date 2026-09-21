#!/usr/bin/env sh
# Schedule validated encrypted-vault artifact copies without exposing key data.
set -eu

VAULT_PATH="${HYDRA_VAULT_PATH:-data/vault.age}"
VAULT_KEY="${HYDRA_VAULT_KEY:-}"
BACKUP_DIR="${HYDRA_VAULT_BACKUP_DIR:-./vault-backups}"
INTERVAL="${HYDRA_VAULT_BACKUP_INTERVAL_SECONDS:-86400}"
BACKUP_COMMAND="${HYDRA_VAULT_BACKUP_COMMAND:-hydra-vault}"

[ -n "$VAULT_KEY" ] || {
  echo "vault backup scheduler ERROR: HYDRA_VAULT_KEY is required" >&2
  exit 1
}
[ -f "$VAULT_PATH" ] || {
  echo "vault backup scheduler ERROR: vault source is missing" >&2
  exit 1
}
case "$INTERVAL" in ''|*[!0-9]*) echo "vault backup scheduler ERROR: interval must be an integer" >&2; exit 1 ;; esac
[ "$INTERVAL" -ge 60 ] || {
  echo "vault backup scheduler ERROR: interval must be at least 60 seconds" >&2
  exit 1
}

if [ -x "$BACKUP_COMMAND" ]; then
  BACKUP_COMMAND_PATH="$BACKUP_COMMAND"
else
  BACKUP_COMMAND_PATH=$(command -v "$BACKUP_COMMAND" 2>/dev/null || true)
fi
[ -n "$BACKUP_COMMAND_PATH" ] || {
  echo "vault backup scheduler ERROR: hydra-vault command is unavailable" >&2
  exit 1
}

umask 077
mkdir -p "$BACKUP_DIR"

run_backup() {
  TIMESTAMP=$(date +%Y%m%d_%H%M%S)
  DESTINATION="$BACKUP_DIR/vault_${TIMESTAMP}.age"
  if [ -e "$DESTINATION" ]; then
    echo "vault backup scheduler ERROR: destination already exists" >&2
    return 1
  fi

  OUTPUT=$(mktemp "${TMPDIR:-/tmp}/hydra-vault-backup.XXXXXX")
  if ! HYDRA_VAULT_PATH="$VAULT_PATH" HYDRA_VAULT_KEY="$VAULT_KEY" \
    "$BACKUP_COMMAND_PATH" backup "$DESTINATION" > "$OUTPUT" 2>&1; then
    rm -f -- "$OUTPUT"
    echo "vault backup scheduler ERROR: encrypted backup command failed" >&2
    return 1
  fi
  rm -f -- "$OUTPUT"
  echo "vault backup: ok"
}

if [ "${HYDRA_VAULT_BACKUP_ONCE:-0}" = "1" ]; then
  run_backup
  exit 0
fi

trap 'exit 0' HUP INT TERM
while :; do
  run_backup
  sleep "$INTERVAL"
done
