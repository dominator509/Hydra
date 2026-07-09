#!/usr/bin/env sh
# scripts/promote-prod.sh — Promote a staging-tested image to production
#
# Usage:
#   PROMOTE=yes bash scripts/promote-prod.sh <TAG>
#   bash scripts/promote-prod.sh <TAG>           # Dry-run (promote-gate)
#
# Guards:
#   - Refuses unless PROMOTE=yes is set in environment
#   - Validates staging smoke test passed
#   - Requires tty confirmation (read -p)
#
# Environment:
#   REGISTRY       — Container registry URL (default: ghcr.io)
#   PROMOTE        — Must be "yes" to proceed past the gate

set -eu

TAG="${1:?Usage: promote-prod.sh <TAG>}"
REGISTRY="${REGISTRY:-ghcr.io}"

STAGING_IMAGE="${REGISTRY}/hydra/kernel:${TAG}"
PROD_IMAGE="${REGISTRY}/hydra/kernel:${TAG}-prod"
PROD_LATEST="${REGISTRY}/hydra/kernel:latest-prod"

echo "============================================"
echo "  Hydra Production Promotion"
echo "============================================"
echo "  Tag:           ${TAG}"
echo "  Staging image: ${STAGING_IMAGE}"
echo "  Prod image:    ${PROD_IMAGE}"
echo "  PROMOTE:       ${PROMOTE:-no}"
echo "============================================"

# --- Gate: PROMOTE=yes required ---
if [ "${PROMOTE:-}" != "yes" ]; then
    echo ""
    echo "promote-gate: PROMOTE not set to 'yes'. Skipping promotion."
    echo "promote-gate: Set PROMOTE=yes to proceed."
    echo ""
    echo "To promote, run:"
    echo "  PROMOTE=yes bash scripts/promote-prod.sh ${TAG}"
    echo ""
    echo "promote-gate: ok"
    exit 0
fi

# --- Validate staging deployment ---
echo ""
echo "promote: validating staging deployment..."
if command -v curl >/dev/null 2>&1; then
    STAGING_URL="${STAGING_URL:-https://staging.hydra.internal/healthz}"
    if curl -fsS --max-time 10 "${STAGING_URL}" >/dev/null 2>&1; then
        echo "promote: staging healthz OK at ${STAGING_URL}"
    else
        echo "promote: ERROR — staging healthz failed at ${STAGING_URL}"
        echo "promote: Aborting promotion. Verify staging is healthy first."
        exit 1
    fi
else
    echo "promote: WARNING — curl not available, skipping staging health check."
    echo "promote: Ensure staging was manually verified before promoting."
fi

# --- Require tty confirmation ---
if [ ! -t 0 ]; then
    echo "promote: ERROR — no tty available. This script requires interactive confirmation."
    echo "promote: Run on a terminal or use a runner with tty enabled."
    exit 1
fi

echo ""
echo "============================================"
echo "  PRODUCTION PROMOTION CONFIRMATION"
echo "============================================"
echo "  You are about to promote: ${STAGING_IMAGE}"
echo "  to production tag:        ${PROD_IMAGE}"
echo "============================================"
printf "  Type 'yes' to confirm: "
read -r CONFIRM

if [ "${CONFIRM}" != "yes" ]; then
    echo "promote: confirmation failed (got '${CONFIRM}'). Aborting."
    exit 1
fi

# --- Tag and push production image ---
echo ""
echo "promote: pulling staging image..."
docker pull "${STAGING_IMAGE}"

echo "promote: tagging as ${PROD_IMAGE}..."
docker tag "${STAGING_IMAGE}" "${PROD_IMAGE}"

echo "promote: tagging as ${PROD_LATEST}..."
docker tag "${STAGING_IMAGE}" "${PROD_LATEST}"

echo "promote: pushing production images..."
docker push "${PROD_IMAGE}"
docker push "${PROD_LATEST}"

echo ""
echo "promote: ok"
echo "promote: Production image ${PROD_IMAGE} is now available."
echo "promote: Deploy to production hosts using deploy-prod.sh (or manual docker compose up)."
