#!/usr/bin/env sh
# scripts/deploy-staging.sh — Deploy hydra-kernel to staging
#
# Usage:
#   bash scripts/deploy-staging.sh <TAG>          # Deploy tag to staging
#   bash scripts/deploy-staging.sh <TAG> --dry-run  # Print plan only
#
# Environment:
#   REGISTRY       — Container registry URL (default: ghcr.io)
#   STAGING_HOST   — SSH hostname (required for actual deploy)
#   STAGING_USER   — SSH user (default: root)
#   STAGING_SSH_KEY — SSH private key path or content
#   STAGING_SSH_KNOWN_HOSTS — known-hosts file path or content (required)
#   STAGING_DIGEST — pushed image digest, sha256:<64 lowercase hex> (required)
#   STAGING_EGRESS_DIGEST — pushed egress image digest, sha256:<64 lowercase hex> (required)
#   STAGING_SMOKE_URL — public HTTPS staging base URL for ingress health checks

set -eu

TAG="${1:?Usage: deploy-staging.sh <TAG> [--dry-run]}"
DRY_RUN=false
[ "${2:-}" = "--dry-run" ] && DRY_RUN=true

REGISTRY="${REGISTRY:-ghcr.io}"
STAGING_USER="${STAGING_USER:-root}"

fail() {
    echo "deploy-staging ERROR: $1" >&2
    exit 1
}

case "$TAG" in
    ''|*[!A-Za-z0-9._-]*) fail "TAG contains unsupported characters" ;;
esac
case "$REGISTRY" in
    ''|*[!A-Za-z0-9./:_-]*) fail "REGISTRY contains unsupported characters" ;;
esac

IMAGE="${REGISTRY}/hydra/kernel:${TAG}"
EGRESS_IMAGE="${REGISTRY}/hydra/egress-proxy:${TAG}"
COMPOSE_DIR="/opt/hydra/docker"

deploy_plan() {
    echo "============================================"
    echo "  Hydra Staging Deployment Plan"
    echo "============================================"
    echo "  Tag:        ${TAG}"
    echo "  Image:      ${IMAGE}"
    echo "  Egress:     ${EGRESS_IMAGE}"
    echo "  Host:       ${STAGING_HOST:-<not set>}"
    echo "  User:       ${STAGING_USER}"
    echo "  Compose:    ${COMPOSE_DIR}/compose.yaml"
    echo "  Smoke URL:  ${STAGING_SMOKE_URL:-<not set>}"
    echo "--------------------------------------------"
    echo "  Steps:"
    echo "    1. docker compose pull"
    echo "    2. docker compose up -d"
    echo "    3. smoke test (healthz + readyz)"
    echo "============================================"
}

if $DRY_RUN; then
    deploy_plan
    echo "deploy-staging: dry-run ok"
    exit 0
fi

# Validate required variables
: "${STAGING_HOST:?STAGING_HOST must be set for deploy}"
: "${STAGING_SSH_KEY:?STAGING_SSH_KEY must be set for deploy}"
: "${STAGING_SSH_KNOWN_HOSTS:?STAGING_SSH_KNOWN_HOSTS must be set for deploy}"
: "${STAGING_DIGEST:?STAGING_DIGEST must be set for deploy}"
: "${STAGING_EGRESS_DIGEST:?STAGING_EGRESS_DIGEST must be set for deploy}"
: "${STAGING_SMOKE_URL:?STAGING_SMOKE_URL must be set for deploy}"
printf '%s\n' "$STAGING_DIGEST" | grep -Eq '^sha256:[0-9a-f]{64}$' ||
    fail "STAGING_DIGEST must match sha256:<64 lowercase hex>"
printf '%s\n' "$STAGING_EGRESS_DIGEST" | grep -Eq '^sha256:[0-9a-f]{64}$' ||
    fail "STAGING_EGRESS_DIGEST must match sha256:<64 lowercase hex>"
case "$STAGING_SMOKE_URL" in
    https://?*) ;;
    *) fail "STAGING_SMOKE_URL must be an HTTPS URL without credentials" ;;
esac
case "$STAGING_SMOKE_URL" in
    *[!A-Za-z0-9:/._?-]*) fail "STAGING_SMOKE_URL contains unsupported characters" ;;
esac
SMOKE_BASE="${STAGING_SMOKE_URL%/}"

# Write owner-provided SSH material to temporary files without logging it.
SSH_KEY_FILE=$(mktemp)
KNOWN_HOSTS_FILE=$(mktemp)
cleanup() {
    rm -f "$SSH_KEY_FILE" "$KNOWN_HOSTS_FILE"
}
trap cleanup EXIT HUP INT TERM
if [ -f "$STAGING_SSH_KEY" ]; then
    cp "$STAGING_SSH_KEY" "$SSH_KEY_FILE"
else
    printf '%s\n' "$STAGING_SSH_KEY" > "$SSH_KEY_FILE"
fi
if [ -f "$STAGING_SSH_KNOWN_HOSTS" ]; then
    cp "$STAGING_SSH_KNOWN_HOSTS" "$KNOWN_HOSTS_FILE"
else
    printf '%s\n' "$STAGING_SSH_KNOWN_HOSTS" > "$KNOWN_HOSTS_FILE"
fi
[ -s "$SSH_KEY_FILE" ] || fail "STAGING_SSH_KEY is empty"
[ -s "$KNOWN_HOSTS_FILE" ] || fail "STAGING_SSH_KNOWN_HOSTS is empty"
chmod 600 "$SSH_KEY_FILE"
chmod 600 "$KNOWN_HOSTS_FILE"

# Do not let a dead host or half-open connection hold a release job forever.
SSH_CMD="ssh -i ${SSH_KEY_FILE} -o ConnectTimeout=10 -o ConnectionAttempts=1 -o ServerAliveInterval=10 -o ServerAliveCountMax=3 -o StrictHostKeyChecking=yes -o UserKnownHostsFile=${KNOWN_HOSTS_FILE}"
SSH_DEST="${STAGING_USER}@${STAGING_HOST}"

echo "deploy-staging: pulling digest-pinned image ${IMAGE}@${STAGING_DIGEST} on ${STAGING_HOST}..."
$SSH_CMD "$SSH_DEST" \
    "cd ${COMPOSE_DIR} && \
     docker pull '${IMAGE}@${STAGING_DIGEST}' && \
     docker image inspect '${IMAGE}@${STAGING_DIGEST}' >/dev/null && \
     docker image tag '${IMAGE}@${STAGING_DIGEST}' '${IMAGE}' && \
     docker pull '${EGRESS_IMAGE}@${STAGING_EGRESS_DIGEST}' && \
     docker image inspect '${EGRESS_IMAGE}@${STAGING_EGRESS_DIGEST}' >/dev/null && \
     docker image tag '${EGRESS_IMAGE}@${STAGING_EGRESS_DIGEST}' '${EGRESS_IMAGE}'"

echo "deploy-staging: restarting services on ${STAGING_HOST}..."
$SSH_CMD "$SSH_DEST" \
    "cd ${COMPOSE_DIR} && \
     REGISTRY='${REGISTRY}' HYDRA_TAG='${TAG}' docker compose up -d --no-build"

echo "deploy-staging: running smoke test on ${STAGING_HOST}..."
$SSH_CMD "$SSH_DEST" \
    "curl --connect-timeout 5 --max-time 30 -fsS '${SMOKE_BASE}/healthz' >/dev/null && \
     curl --connect-timeout 5 --max-time 30 -fsS '${SMOKE_BASE}/readyz' >/dev/null && \
     echo 'smoke: ok'"

echo "deploy-staging: ok"
