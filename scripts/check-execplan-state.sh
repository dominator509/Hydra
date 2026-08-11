#!/usr/bin/env sh
# Validate the authoritative ExecPlan index and the Nexus plan queue.
set -eu

[ -f AGENTS.md ] || { echo "execplan state ERROR: run from repository root." >&2; exit 1; }

INDEX=.agent/state/execplan-index.md
[ -f "$INDEX" ] || { echo "execplan state ERROR: missing $INDEX" >&2; exit 1; }

if ACTIVE_COUNT=$(grep -Ec '^\| EP-[0-9]{3} \| ACTIVE \|' "$INDEX"); then
  :
else
  ACTIVE_COUNT=0
fi
[ "$ACTIVE_COUNT" -le 1 ] || {
  echo "execplan state ERROR: expected at most one ACTIVE index row, found $ACTIVE_COUNT" >&2
  exit 1
}
if [ "$ACTIVE_COUNT" -eq 0 ]; then
  grep -Eq '^\| EP-015 \| COMPLETE \|' "$INDEX" || {
    echo "execplan state ERROR: no ACTIVE plan is valid only after EP-015 is COMPLETE" >&2
    exit 1
  }
fi

for artifact in \
  NEXUS_INTEGRATION_AUDIT.md \
  NEXUS_INTEGRATION.md \
  .agent/specs/SPEC-010-nexus-interoperability.md; do
  [ -f "$artifact" ] || { echo "execplan state ERROR: missing $artifact" >&2; exit 1; }
done

for number in 000 001 002 003 004 005 006 007 008 009 010; do
  grep -Eq "^\| EP-$number \|" "$INDEX" || {
    echo "execplan state ERROR: index missing EP-$number" >&2
    exit 1
  }
done

validate_sections() {
  plan=$1
  previous=0
  section=1
  for title in \
    "Purpose / Big Picture" \
    "Scope" \
    "Non-goals" \
    "Context and Orientation" \
    "Files to Read First" \
    "Files to Change" \
    "Interfaces and Contracts" \
    "Milestones" \
    "Concrete Steps" \
    "Validation and Acceptance" \
    "Idempotence and Recovery" \
    "Progress" \
    "Surprises & Discoveries" \
    "Decision Log" \
    "Outcomes & Retrospective"; do
    line=$(grep -nF "## $section. $title" "$plan" | cut -d: -f1)
    [ -n "$line" ] || {
      echo "execplan state ERROR: $plan missing section $section ($title)" >&2
      exit 1
    }
    [ "$line" -gt "$previous" ] || {
      echo "execplan state ERROR: $plan section $section is out of order" >&2
      exit 1
    }
    previous=$line
    section=$((section + 1))
  done
}

for number in 011 012 013 014 015 016; do
  set -- .agent/execplans/EP-$number-*.md
  [ "$#" -eq 1 ] && [ -f "$1" ] || {
    echo "execplan state ERROR: expected one plan file for EP-$number" >&2
    exit 1
  }
  plan=$1
  state=$(sed -n "s/^| EP-$number | \([A-Z]*\) |.*/\1/p" "$INDEX")
  [ -n "$state" ] || {
    echo "execplan state ERROR: index missing state for EP-$number" >&2
    exit 1
  }
  grep -Fxq "Plan status: $state" "$plan" || {
    echo "execplan state ERROR: $plan status does not match index state $state" >&2
    exit 1
  }
  validate_sections "$plan"
done

grep -Fxq "Plan status: DEFERRED" .agent/execplans/EP-016-nexus-model-a2a-and-skills.md || {
  echo "execplan state ERROR: EP-016 must remain DEFERRED" >&2
  exit 1
}

echo "execplan state: ok"
