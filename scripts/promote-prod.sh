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
#   STAGING_DIGEST  — Pushed image digest, sha256:<64 lowercase hex>
#   STAGING_EGRESS_DIGEST — Pushed egress image digest, sha256:<64 lowercase hex>
#   STAGING_URL     — HTTPS staging health URL (default: https://staging.hydra.internal/healthz)
#   STAGING_READY_URL — HTTPS staging readiness URL (derived from STAGING_URL by default)

set -eu

TAG="${1:?Usage: promote-prod.sh <TAG>}"
REGISTRY="${REGISTRY:-ghcr.io}"

STAGING_IMAGE="${REGISTRY}/hydra/kernel:${TAG}"
PROD_IMAGE="${REGISTRY}/hydra/kernel:${TAG}-prod"
STAGING_EGRESS_IMAGE="${REGISTRY}/hydra/egress-proxy:${TAG}"
PROD_EGRESS_IMAGE="${REGISTRY}/hydra/egress-proxy:${TAG}-prod"

fail() {
    echo "promote ERROR: $1" >&2
    exit 1
}

validate_https_url() {
    name=$1
    value=$2
    case "$value" in
        https://?*) ;;
        *) fail "$name must be an HTTPS URL without credentials" ;;
    esac
    case "$value" in
        *[!A-Za-z0-9:/._?-]*) fail "$name contains unsupported characters" ;;
    esac
}

case "$TAG" in
    ''|*[!A-Za-z0-9._-]*) fail "TAG contains unsupported characters" ;;
esac
case "$REGISTRY" in
    ''|*[!A-Za-z0-9./:_-]*) fail "REGISTRY contains unsupported characters" ;;
esac

echo "============================================"
echo "  Hydra Production Promotion"
echo "============================================"
echo "  Tag:           ${TAG}"
echo "  Staging image: ${STAGING_IMAGE}"
echo "  Prod image:    ${PROD_IMAGE}"
echo "  Staging egress: ${STAGING_EGRESS_IMAGE}"
echo "  Prod egress:    ${PROD_EGRESS_IMAGE}"
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
    echo "promote-gate: dry-run ok"
    exit 0
fi

# Validate the exact artifact before contacting staging or Docker.
: "${STAGING_DIGEST:?STAGING_DIGEST must be set when PROMOTE=yes}"
printf '%s\n' "$STAGING_DIGEST" | grep -Eq '^sha256:[0-9a-f]{64}$' ||
    fail "STAGING_DIGEST must match sha256:<64 lowercase hex>"
: "${STAGING_EGRESS_DIGEST:?STAGING_EGRESS_DIGEST must be set when PROMOTE=yes}"
printf '%s\n' "$STAGING_EGRESS_DIGEST" | grep -Eq '^sha256:[0-9a-f]{64}$' ||
    fail "STAGING_EGRESS_DIGEST must match sha256:<64 lowercase hex>"

# --- Validate staging deployment ---
echo ""
echo "promote: validating staging deployment..."
command -v curl >/dev/null 2>&1 || fail "curl is required for staging health validation"
command -v docker >/dev/null 2>&1 || fail "docker is required for digest validation and promotion"
STAGING_URL="${STAGING_URL:-https://staging.hydra.internal/healthz}"
STAGING_READY_URL="${STAGING_READY_URL:-${STAGING_URL%/healthz}/readyz}"
validate_https_url "STAGING_URL" "$STAGING_URL"
validate_https_url "STAGING_READY_URL" "$STAGING_READY_URL"
curl -fsS --max-time 10 "${STAGING_URL}" >/dev/null 2>&1 ||
    fail "staging healthz failed at ${STAGING_URL}"
echo "promote: staging healthz OK at ${STAGING_URL}"
curl -fsS --max-time 10 "${STAGING_READY_URL}" >/dev/null 2>&1 ||
    fail "staging readyz failed at ${STAGING_READY_URL}"
echo "promote: staging readyz OK at ${STAGING_READY_URL}"

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
echo "  pinned digest:             ${STAGING_DIGEST}"
echo "  egress image:               ${STAGING_EGRESS_IMAGE}"
echo "  egress digest:              ${STAGING_EGRESS_DIGEST}"
echo "============================================"
printf "  Type 'yes' to confirm: "
read -r CONFIRM

if [ "${CONFIRM}" != "yes" ]; then
    echo "promote: confirmation failed (got '${CONFIRM}'). Aborting."
    exit 1
fi

# --- Tag and push production image ---
echo ""
echo "promote: pulling digest-pinned staging image..."
docker pull "${STAGING_IMAGE}@${STAGING_DIGEST}"
docker image inspect "${STAGING_IMAGE}@${STAGING_DIGEST}" >/dev/null
docker pull "${STAGING_EGRESS_IMAGE}@${STAGING_EGRESS_DIGEST}"
docker image inspect "${STAGING_EGRESS_IMAGE}@${STAGING_EGRESS_DIGEST}" >/dev/null

echo "promote: tagging as ${PROD_IMAGE}..."
docker tag "${STAGING_IMAGE}@${STAGING_DIGEST}" "${PROD_IMAGE}"
echo "promote: tagging egress proxy as ${PROD_EGRESS_IMAGE}..."
docker tag "${STAGING_EGRESS_IMAGE}@${STAGING_EGRESS_DIGEST}" "${PROD_EGRESS_IMAGE}"

echo "promote: pushing production images..."
docker push "${PROD_IMAGE}"
docker push "${PROD_EGRESS_IMAGE}"

echo ""
echo "promote: ok"
echo "promote: Production image ${PROD_IMAGE} is now available."
echo "promote: Deploy to production hosts using deploy-prod.sh (or manual docker compose up)."
