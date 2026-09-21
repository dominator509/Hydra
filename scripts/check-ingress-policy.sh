#!/usr/bin/env sh
# Validate the reference public ingress boundary without starting services.
set -eu

[ -f AGENTS.md ] || {
  echo "ingress policy ERROR: run from repository root." >&2
  exit 1
}

CADDYFILE=${HYDRA_CADDYFILE:-docker/Caddyfile}
[ -f "$CADDYFILE" ] || {
  echo "ingress policy ERROR: missing $CADDYFILE" >&2
  exit 1
}

grep -Fq -- 'HYDRA_ENV: "${HYDRA_ENV:-dev}"' docker/compose.yaml || {
  echo "ingress policy ERROR: Caddy must receive the Hydra environment" >&2
  exit 1
}
grep -Fq -- 'tls internal is development-only' docker/compose.yaml || {
  echo "ingress policy ERROR: non-development Caddy TLS guard is missing" >&2
  exit 1
}

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
  echo "ingress policy ERROR: Python 3 is required to inspect Caddyfile blocks." >&2
  exit 1
}

"$PYTHON_BIN" - "$CADDYFILE" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
lines = path.read_text(encoding="utf-8").splitlines()


def block_after(index):
    depth = 0
    body = []
    started = False
    for line in lines[index:]:
        depth += line.count("{")
        if started:
            body.append(line)
        if "{" in line:
            started = True
        depth -= line.count("}")
        if started and depth == 0:
            return body
    return []


matcher_index = next(
    (index for index, line in enumerate(lines) if line.strip() == "@private_metrics path /metrics*"),
    None,
)
if matcher_index is None:
    raise SystemExit("ingress policy ERROR: missing @private_metrics /metrics* matcher")

metrics_handle = next(
    (index for index, line in enumerate(lines) if line.strip() == "handle @private_metrics {"),
    None,
)
if metrics_handle is None:
    raise SystemExit("ingress policy ERROR: missing dedicated metrics handle")
metrics_body = block_after(metrics_handle)
if not any("respond" in line and "404" in line for line in metrics_body):
    raise SystemExit("ingress policy ERROR: metrics handle must respond 404")

catch_all = [
    index
    for index, line in enumerate(lines)
    if line.strip() == "handle {"
]
if len(catch_all) != 1:
    raise SystemExit("ingress policy ERROR: expected exactly one catch-all handle")
catch_all_body = block_after(catch_all[0])
if not any(line.strip().startswith("reverse_proxy kernel:8080") for line in catch_all_body):
    raise SystemExit("ingress policy ERROR: catch-all handle must proxy Kernel")

for index, line in enumerate(lines):
    if line.strip().startswith("reverse_proxy kernel:8080") and index not in range(catch_all[0], catch_all[0] + len(catch_all_body) + 1):
        raise SystemExit("ingress policy ERROR: Kernel proxy must remain inside the catch-all handle")

if matcher_index > metrics_handle:
    raise SystemExit("ingress policy ERROR: metrics matcher must precede its handle")
if metrics_handle > catch_all[0]:
    raise SystemExit("ingress policy ERROR: metrics denial must precede catch-all proxy")

print("ingress policy: ok")
PY
