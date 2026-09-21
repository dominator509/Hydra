#!/usr/bin/env sh
# Static contract check for the explicit outbound proxy boundary.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)

fail() {
  echo "egress policy:FAIL: $1" >&2
  exit 1
}

require_literal() {
  file=$1
  needle=$2
  if ! grep -Fq -- "$needle" "$ROOT/$file"; then
    fail "$file is missing required egress contract"
  fi
}

require_literal "crates/kernel/src/config.rs" "HYDRA_EGRESS_PROXY_URL is required in staging or prod"
require_literal "crates/kernel/src/config.rs" "HYDRA_EGRESS_PROXY_URL must be an absolute http(s) proxy URI without embedded credentials"
require_literal "crates/kernel/src/main.rs" "egress_proxy_url: config.egress_proxy_url.clone()"
require_literal "crates/kernel/src/runtime_services.rs" "NexusModelProvider::new_with_proxy"
require_literal "crates/kernel/src/runtime_services.rs" "DeepSeekProvider::new_with_proxy"
require_literal "crates/kernel/src/runtime_services.rs" "OpenAiCompatProvider::new_with_proxy"
require_literal "crates/kernel/src/runtime_services.rs" "AnthropicProvider::new_with_proxy"
require_literal "crates/llm-router/src/lib.rs" "reqwest::Proxy::all"
require_literal "crates/llm-router/src/providers/anthropic.rs" "new_with_proxy"
require_literal "crates/llm-router/src/providers/deepseek.rs" "new_with_proxy"
require_literal "crates/llm-router/src/providers/nexus.rs" "new_with_proxy"
require_literal "crates/llm-router/src/providers/openai_compat.rs" "new_with_proxy"
require_literal "crates/fabric/src/auth/oidc.rs" "reqwest::Proxy::all"
require_literal "crates/fabric/src/egress.rs" "new_with_proxy"
require_literal "crates/bridge-host/src/host.rs" "new_with_proxy"
require_literal "crates/kernel/src/runtime_services.rs" "ReqwestEgressClient::new_with_proxy"
if grep -Fq -- "Arc::new(bridge_host::DenyEgressClient)" "$ROOT/crates/kernel/src/runtime_services.rs"; then
  fail "Kernel bridge lifecycle must not use the unconditional deny egress client"
fi
require_literal "docker/compose.yaml" "HYDRA_EGRESS_PROXY_URL:"
require_literal "docker/compose.yaml" "dockerfile: docker/egress-proxy.Dockerfile"
require_literal "docker/compose.yaml" 'hydra/egress-proxy:${HYDRA_TAG:-latest}'
require_literal "docker/egress-proxy.Dockerfile" "RUN apk add --no-cache tinyproxy"
require_literal "docker/egress-proxy.Dockerfile" "COPY docker/egress-proxy.conf /etc/tinyproxy/tinyproxy.conf"
if grep -Fq -- "apk add --no-cache tinyproxy" docker/compose.yaml; then
  fail "Compose installs Tinyproxy at service startup instead of using the versioned image"
fi
require_literal "docker/compose.yaml" "condition: service_healthy"
require_literal "docker/compose.yaml" "command -v tinyproxy"
require_literal "docker/compose.yaml" "busybox nc -z -w 3 127.0.0.1 8888"
require_literal ".env.example" "HYDRA_EGRESS_PROXY_URL="
require_literal "docker/nexus.env.example" "HYDRA_EGRESS_PROXY_URL="
require_literal "scripts/preflight.sh" "check-egress-policy"
require_literal "scripts/verify.sh" "check-egress-policy.sh"

echo "egress policy: ok"
