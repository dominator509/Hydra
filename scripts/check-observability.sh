#!/usr/bin/env sh
# Validate the optional Prometheus/Alertmanager profile without starting services.
set -eu

[ -f AGENTS.md ] || {
  echo "observability policy ERROR: run from repository root." >&2
  exit 1
}

ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)

PYTHON_BIN=${PYTHON_BIN:-}
if [ -z "$PYTHON_BIN" ]; then
  for candidate in python3 python python.exe; do
    if command -v "$candidate" >/dev/null 2>&1; then
      PYTHON_BIN=$candidate
      break
    fi
  done
fi
[ -n "$PYTHON_BIN" ] || {
  echo "observability policy ERROR: Python 3 is required to parse YAML configuration." >&2
  exit 1
}

"$PYTHON_BIN" - "$ROOT" <<'PY'
from pathlib import Path
import sys

try:
    import yaml
except ImportError as exc:
    raise SystemExit(f"observability policy ERROR: PyYAML is required: {exc}")

root = Path(sys.argv[1])


def load(relative):
    path = root / relative
    if not path.is_file():
        raise SystemExit(f"observability policy ERROR: missing {relative}")
    with path.open(encoding="utf-8") as handle:
        value = yaml.safe_load(handle)
    if not isinstance(value, dict):
        raise SystemExit(f"observability policy ERROR: {relative} must be a YAML object")
    return value


compose = load("docker/compose.yaml")
services = compose.get("services", {})
networks = compose.get("networks", {})
required_services = {"prometheus", "alertmanager"}
if not required_services.issubset(services):
    raise SystemExit("observability policy ERROR: observability services are incomplete")

observability = networks.get("observability-internal")
if not isinstance(observability, dict) or observability.get("internal") is not True:
    raise SystemExit("observability policy ERROR: observability network must be internal")

prometheus = services["prometheus"]
alertmanager = services["alertmanager"]
for name, service in (("prometheus", prometheus), ("alertmanager", alertmanager)):
    if service.get("profiles") != ["observability"]:
        raise SystemExit(f"observability policy ERROR: {name} must be profile-gated")
    if service.get("ports"):
        raise SystemExit(f"observability policy ERROR: {name} must not publish host ports")

if prometheus.get("networks") != ["ingress-internal", "observability-internal"]:
    raise SystemExit("observability policy ERROR: Prometheus network boundary changed")
if alertmanager.get("networks") != ["observability-internal"]:
    raise SystemExit("observability policy ERROR: Alertmanager network boundary changed")

prometheus_config = load("docker/prometheus.yml")
rule_files = prometheus_config.get("rule_files")
if rule_files != ["/etc/prometheus/alerts.yaml"]:
    raise SystemExit("observability policy ERROR: Prometheus rule path changed")
alerting = prometheus_config.get("alerting", {}).get("alertmanagers", [])
targets = alerting[0]["static_configs"][0]["targets"] if alerting else []
if targets != ["alertmanager:9093"]:
    raise SystemExit("observability policy ERROR: Prometheus Alertmanager target changed")
scrapes = prometheus_config.get("scrape_configs", [])
if len(scrapes) != 1 or scrapes[0].get("job_name") != "hydra-kernel":
    raise SystemExit("observability policy ERROR: expected one Kernel scrape job")
scrape_targets = scrapes[0]["static_configs"][0]["targets"]
if scrape_targets != ["kernel:8080"]:
    raise SystemExit("observability policy ERROR: Prometheus must scrape only kernel:8080")

alertmanager_config = load("docker/alertmanager.yml")
route = alertmanager_config.get("route", {})
if route.get("receiver") != "hydra-operator":
    raise SystemExit("observability policy ERROR: Alertmanager default receiver changed")
receivers = alertmanager_config.get("receivers", [])
if receivers != [{"name": "hydra-operator"}]:
    raise SystemExit("observability policy ERROR: default receiver must have no outbound destination")

rules = load("docker/alerts.yaml")
groups = rules.get("groups", [])
rule_names = {
    rule.get("alert")
    for group in groups
    for rule in group.get("rules", [])
    if isinstance(rule, dict)
}
required_rules = {"TkNukeAbortsCritical", "TkCacheHitRatioWarning", "TkCacheHitRatioPage"}
if not required_rules.issubset(rule_names):
    raise SystemExit("observability policy ERROR: required TOKENKILLER alert rules are missing")

print("observability policy: ok")
PY
