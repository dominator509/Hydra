#!/usr/bin/env sh
# Shared, fail-closed evidence semantics for the EP-010 readiness gate.

READINESS_EVIDENCE_ERROR=""

readiness_evidence_trimmed_field() {
  printf '%s\n' "$1" | awk -F'|' -v field="$2" '
    {
      value=$field
      sub(/^[[:space:]]+/, "", value)
      sub(/[[:space:]]+$/, "", value)
      print value
    }
  '
}

readiness_evidence_last_drill_pass() {
  awk -F'|' -v drill="$1" '
    function trim(value) {
      sub(/^[[:space:]]+/, "", value)
      sub(/[[:space:]]+$/, "", value)
      return value
    }
    trim($2) == "Drill" && trim($3) == "Date" && trim($4) == "Status" && trim($5) == "Metric/Evidence" && trim($6) == "Operator" {
      in_table=1
      next
    }
    in_table && $0 !~ /^[|]/ { exit }
    in_table && trim($2) == drill && trim($4) == "PASS" { row=$0; found=1 }
    END {
      if (!found) exit 1
      print row
    }
  ' "$2"
}

readiness_evidence_exact_launch_row() {
  awk -F'|' -v check="$1" '
    function trim(value) {
      sub(/^[[:space:]]+/, "", value)
      sub(/[[:space:]]+$/, "", value)
      return value
    }
    trim($2) == check { row=$0; count++ }
    END {
      if (count != 1) exit 1
      print row
    }
  ' "$2"
}

readiness_evidence_check_date() {
  label=$1
  date_value=$2
  now_epoch=$3
  max_age_days=$4

  case "$date_value" in
    20[0-9][0-9]-[0-9][0-9]-[0-9][0-9]) ;;
    *)
      READINESS_EVIDENCE_ERROR="$label - date must be YYYY-MM-DD"
      return 1
      ;;
  esac

  if parsed_epoch=$(date -u -d "$date_value" +%s 2>/dev/null); then
    :
  else
    READINESS_EVIDENCE_ERROR="$label - date '$date_value' cannot be parsed"
    return 1
  fi
  if [ "$parsed_epoch" -gt "$now_epoch" ]; then
    READINESS_EVIDENCE_ERROR="$label - evidence date '$date_value' is in the future"
    return 1
  fi
  age_days=$(( (now_epoch - parsed_epoch) / 86400 ))
  if [ "$age_days" -gt "$max_age_days" ]; then
    READINESS_EVIDENCE_ERROR="$label - evidence is $age_days days old (> $max_age_days, last evidence on $date_value)"
    return 1
  fi
}

readiness_evidence_placeholder() {
  case "$1" in
    ""|"-"|"TBD"|"PENDING"|"BLOCKED"|"N/A"|"n/a") return 0 ;;
    *) return 1 ;;
  esac
}

readiness_evidence_check() {
  operations_file=$1
  readiness_file=$2
  now_epoch=$3
  max_age_days=$4
  READINESS_EVIDENCE_ERROR=""

  [ -f "$operations_file" ] || {
    READINESS_EVIDENCE_ERROR="drills - $operations_file not found"
    return 1
  }
  [ -f "$readiness_file" ] || {
    READINESS_EVIDENCE_ERROR="launch-table - $readiness_file not found"
    return 1
  }

  for drill in D1 D2 D3 D4 D5; do
    if drill_row=$(readiness_evidence_last_drill_pass "$drill" "$operations_file"); then
      :
    else
      READINESS_EVIDENCE_ERROR="drill $drill - no exact PASS row in $operations_file"
      return 1
    fi
    drill_date=$(readiness_evidence_trimmed_field "$drill_row" 3)
    drill_evidence=$(readiness_evidence_trimmed_field "$drill_row" 5)
    drill_operator=$(readiness_evidence_trimmed_field "$drill_row" 6)
    readiness_evidence_check_date "drill $drill" "$drill_date" "$now_epoch" "$max_age_days" || return 1
    if readiness_evidence_placeholder "$drill_evidence"; then
      READINESS_EVIDENCE_ERROR="drill $drill - metric/evidence is a placeholder"
      return 1
    fi
    if readiness_evidence_placeholder "$drill_operator"; then
      READINESS_EVIDENCE_ERROR="drill $drill - operator is a placeholder"
      return 1
    fi
  done

  for check in \
    "production-readiness-check.sh" \
    "Restore drill (D1)" \
    "Rollback drill (D2)" \
    "Nuke/cache/autonomy drills (D3-D5)" \
    "24h staging soak" \
    "Security review" \
    "Performance review" \
    "Privacy/data review" \
    "Accessibility review" \
    "Observability review" \
    "Sign-off"; do
    if launch_row=$(readiness_evidence_exact_launch_row "$check" "$readiness_file"); then
      :
    else
      READINESS_EVIDENCE_ERROR="launch-table - expected exactly one row for '$check' in $readiness_file"
      return 1
    fi
    launch_owner=$(readiness_evidence_trimmed_field "$launch_row" 3)
    launch_date=$(readiness_evidence_trimmed_field "$launch_row" 4)
    launch_result=$(readiness_evidence_trimmed_field "$launch_row" 5)
    if [ "$launch_result" != "PASS" ]; then
      READINESS_EVIDENCE_ERROR="launch-table - '$check' result must be PASS, found '$launch_result'"
      return 1
    fi
    if readiness_evidence_placeholder "$launch_owner"; then
      READINESS_EVIDENCE_ERROR="launch-table - '$check' owner is a placeholder"
      return 1
    fi
    readiness_evidence_check_date "launch check '$check'" "$launch_date" "$now_epoch" "$max_age_days" || return 1
  done
}
