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

set -eu

TAG="${1:?Usage: deploy-staging.sh <TAG> [--dry-run]}"
DRY_RUN=false
[ "${2:-}" = "--dry-run" ] && DRY_RUN=true

REGISTRY="${REGISTRY:-ghcr.io}"
STAGING_USER="${STAGING_USER:-root}"

IMAGE="${REGISTRY}/hydra/kernel:${TAG}"
COMPOSE_DIR="/opt/hydra/docker"

deploy_plan() {
    echo "============================================"
    echo "  Hydra Staging Deployment Plan"
    echo "============================================"
    echo "  Tag:        ${TAG}"
    echo "  Image:      ${IMAGE}"
    echo "  Host:       ${STAGING_HOST:-<not set>}"
    echo "  User:       ${STAGING_USER}"
    echo "  Compose:    ${COMPOSE_DIR}/compose.yaml"
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

# Write SSH key to temp file
SSH_KEY_FILE=$(mktemp)
trap 'rm -f "$SSH_KEY_FILE"' EXIT
echo "${STAGING_SSH_KEY}" > "$SSH_KEY_FILE"
chmod 600 "$SSH_KEY_FILE"

SSH_CMD="ssh -i ${SSH_KEY_FILE} -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile=/dev/null"
SSH_DEST="${STAGING_USER}@${STAGING_HOST}"

echo "deploy-staging: pulling image ${IMAGE} on ${STAGING_HOST}..."
$SSH_CMD "$SSH_DEST" \
    "cd ${COMPOSE_DIR} && \
     REGISTRY=${REGISTRY} HYDRA_TAG=${TAG} docker compose pull"

echo "deploy-staging: restarting services on ${STAGING_HOST}..."
$SSH_CMD "$SSH_DEST" \
    "cd ${COMPOSE_DIR} && \
     REGISTRY=${REGISTRY} HYDRA_TAG=${TAG} docker compose up -d"

echo "deploy-staging: running smoke test on ${STAGING_HOST}..."
$SSH_CMD "$SSH_DEST" \
    "curl -fsS http://localhost:8080/healthz >/dev/null && \
     curl -fsS http://localhost:8080/readyz >/dev/null && \
     echo 'smoke: ok'"

echo "deploy-staging: ok"
