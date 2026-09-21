#!/usr/bin/env sh
# Retain only explicitly named Hydra recovery artifacts.
set -eu

BACKUP_DIR="${HYDRA_BACKUP_RETENTION_DIR:-${HYDRA_BACKUP_DIR:-./backups}}"
PREFIX="${HYDRA_BACKUP_RETENTION_PREFIX:-hydra_}"
SUFFIX="${HYDRA_BACKUP_RETENTION_SUFFIX:-.dump}"
MAX_AGE="${HYDRA_BACKUP_RETENTION_DAYS:-0}"
MAX_COUNT="${HYDRA_BACKUP_RETENTION_COUNT:-0}"
APPLY="${HYDRA_BACKUP_RETENTION_APPLY:-0}"
CONFIRM="${HYDRA_BACKUP_RETENTION_CONFIRM:-}"

[ -d "$BACKUP_DIR" ] || {
  echo "backup retention ERROR: artifact directory does not exist: $BACKUP_DIR" >&2
  exit 1
}

case "$PREFIX$SUFFIX" in
  ''|*[!A-Za-z0-9._-]*)
    echo "backup retention ERROR: prefix and suffix may contain only safe filename characters" >&2
    exit 1
    ;;
esac
case "$MAX_AGE" in ''|*[!0-9]*) echo "backup retention ERROR: days must be an integer" >&2; exit 1 ;; esac
case "$MAX_COUNT" in ''|*[!0-9]*) echo "backup retention ERROR: count must be an integer" >&2; exit 1 ;; esac
case "$APPLY" in 0|1) ;; *) echo "backup retention ERROR: apply must be 0 or 1" >&2; exit 1 ;; esac
[ "$MAX_AGE" -gt 0 ] || [ "$MAX_COUNT" -gt 0 ] || {
  echo "backup retention ERROR: configure a positive days or count limit" >&2
  exit 1
}

if [ "$APPLY" = "1" ] && [ "$CONFIRM" != "prune" ]; then
  echo "backup retention ERROR: set HYDRA_BACKUP_RETENTION_CONFIRM=prune for apply mode" >&2
  exit 1
fi

TMP_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/hydra-retention.XXXXXX")
ALL_FILE="$TMP_ROOT/all"
CANDIDATE_FILE="$TMP_ROOT/candidates"
trap 'rm -rf "$TMP_ROOT"' EXIT HUP INT TERM

find "$BACKUP_DIR" -maxdepth 1 -type f -name "${PREFIX}*${SUFFIX}" -print | sort > "$ALL_FILE"
: > "$CANDIDATE_FILE"

if [ "$MAX_AGE" -gt 0 ]; then
  find "$BACKUP_DIR" -maxdepth 1 -type f -name "${PREFIX}*${SUFFIX}" -mtime +"$MAX_AGE" -print >> "$CANDIDATE_FILE"
fi

TOTAL=$(wc -l < "$ALL_FILE" | tr -d '[:space:]')
if [ "$MAX_COUNT" -gt 0 ] && [ "$TOTAL" -gt "$MAX_COUNT" ]; then
  head -n "$((TOTAL - MAX_COUNT))" "$ALL_FILE" >> "$CANDIDATE_FILE"
fi

SORTED_CANDIDATES="$TMP_ROOT/sorted-candidates"
sort -u "$CANDIDATE_FILE" > "$SORTED_CANDIDATES"
CANDIDATE_COUNT=$(wc -l < "$SORTED_CANDIDATES" | tr -d '[:space:]')

if [ "$APPLY" = "1" ]; then
  while IFS= read -r candidate; do
    [ -n "$candidate" ] || continue
    case "$candidate" in
      "$BACKUP_DIR"/"$PREFIX"*"$SUFFIX") ;;
      *) echo "backup retention ERROR: candidate escaped artifact directory" >&2; exit 1 ;;
    esac
    rm -f -- "$candidate"
  done < "$SORTED_CANDIDATES"
  echo "backup retention: applied ($CANDIDATE_COUNT artifact(s))"
else
  echo "backup retention: preview ($CANDIDATE_COUNT candidate(s)); no files deleted"
fi
