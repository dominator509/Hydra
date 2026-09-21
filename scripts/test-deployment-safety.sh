#!/usr/bin/env sh
# Static and safe behavioral checks for release-facing deployment helpers.
set -eu

[ -f AGENTS.md ] || { echo "deployment safety ERROR: run from repository root." >&2; exit 1; }

fail() {
    echo "deployment safety ERROR: $1" >&2
    exit 1
}

for file in scripts/deploy-staging.sh scripts/promote-prod.sh .github/workflows/release.yml; do
    [ -f "$file" ] || fail "missing $file"
done

grep -Fq -- "STAGING_DIGEST" scripts/deploy-staging.sh || fail "staging helper does not require a digest"
grep -Fq -- "STAGING_EGRESS_DIGEST" scripts/deploy-staging.sh || fail "staging helper does not require the egress digest"
grep -Fq -- "STAGING_SSH_KNOWN_HOSTS" scripts/deploy-staging.sh || fail "staging helper does not require known hosts"
grep -Fq -- "StrictHostKeyChecking=yes" scripts/deploy-staging.sh || fail "staging helper does not use strict host checking"
grep -Fq -- "ConnectTimeout=10" scripts/deploy-staging.sh || fail "staging helper has no bounded SSH connect timeout"
grep -Fq -- "ServerAliveCountMax=3" scripts/deploy-staging.sh || fail "staging helper has no bounded SSH keepalive"
grep -Fq -- "--connect-timeout 5 --max-time 30" scripts/deploy-staging.sh || fail "staging smoke checks have no bounded curl timeout"
grep -Fq -- "STAGING_SMOKE_URL" scripts/deploy-staging.sh || fail "staging helper does not require an ingress smoke URL"
grep -Fq -- "STAGING_SMOKE_URL: \${{ secrets.STAGING_SMOKE_URL }}" .github/workflows/release.yml || fail "release workflow does not pass the ingress smoke URL"
grep -Fq -- "https://?*)" scripts/deploy-staging.sh || fail "staging helper does not enforce HTTPS smoke validation"
if grep -Fq -- "http://localhost:8080/healthz" scripts/deploy-staging.sh; then
    fail "staging helper still probes the host-unpublished Kernel port"
fi
grep -Fq -- "docker pull '\${IMAGE}@\${STAGING_DIGEST}'" scripts/deploy-staging.sh || fail "staging helper does not pull by digest"
grep -Fq -- "docker image inspect '\${IMAGE}@\${STAGING_DIGEST}'" scripts/deploy-staging.sh || fail "staging helper does not inspect the pinned image"
grep -Fq -- "curl is required for staging health validation" scripts/promote-prod.sh || fail "promotion helper has no curl fail-closed path"
grep -Fq -- "validate_https_url" scripts/promote-prod.sh || fail "promotion helper does not validate staging URLs"
grep -Fq -- 'validate_https_url "STAGING_URL" "$STAGING_URL"' scripts/promote-prod.sh || fail "promotion helper does not enforce HTTPS health validation"
grep -Fq -- 'validate_https_url "STAGING_READY_URL" "$STAGING_READY_URL"' scripts/promote-prod.sh || fail "promotion helper does not enforce HTTPS readiness validation"
grep -Fq -- "staging readyz failed" scripts/promote-prod.sh || fail "promotion helper does not require readiness"
grep -Fq -- "docker pull \"\${STAGING_IMAGE}@\${STAGING_DIGEST}\"" scripts/promote-prod.sh || fail "promotion helper does not pull by digest"
grep -Fq -- "promote-gate: dry-run ok" scripts/promote-prod.sh || fail "promotion dry-run marker is ambiguous"
grep -Fq -- "STAGING_DIGEST: \${{ needs.build-and-push.outputs.image_digest }}" .github/workflows/release.yml || fail "workflow digest output is not passed to staging"
grep -Fq -- "STAGING_EGRESS_DIGEST: \${{ needs.build-and-push.outputs.egress_image_digest }}" .github/workflows/release.yml || fail "workflow egress digest output is not passed to staging"
grep -Fq -- "STAGING_SSH_KNOWN_HOSTS: \${{ secrets.STAGING_SSH_KNOWN_HOSTS }}" .github/workflows/release.yml || fail "workflow host trust is not passed to staging"
grep -Fq -- 'EGRESS_IMAGE="${REGISTRY}/hydra/egress-proxy:${TAG}"' scripts/deploy-staging.sh || fail "staging helper does not name the egress image"
grep -Fq -- 'STAGING_EGRESS_DIGEST' scripts/promote-prod.sh || fail "promotion helper does not require the egress digest"
grep -Fq -- 'PROD_EGRESS_IMAGE' scripts/promote-prod.sh || fail "promotion helper does not promote the egress image"
grep -Fq -- 'STAGING_EGRESS_IMAGE}@${STAGING_EGRESS_DIGEST}' scripts/promote-prod.sh || fail "promotion helper does not pull the egress image by digest"
if grep -Eq -- 'PROD_LATEST|latest-prod' scripts/promote-prod.sh; then
    fail "mutable production alias remains in promotion helper"
fi
if sed -n '/^  build-and-push:/,/^  deploy-staging:/p' .github/workflows/release.yml |
    grep -Fq -- '${{ env.REGISTRY }}/hydra/kernel:latest'; then
    fail "release workflow publishes a mutable Hydra latest alias"
fi

dry_run=$(PROMOTE=no sh scripts/promote-prod.sh v-test)
printf '%s\n' "$dry_run" | grep -Fq -- "promote-gate: dry-run ok" || fail "safe promotion dry-run did not pass"
printf '%s\n' "$dry_run" | grep -Fq -- "promote: ok" && fail "dry-run reported executed promotion"

if PROMOTE=yes STAGING_DIGEST=invalid sh scripts/promote-prod.sh v-test >/tmp/hydra-promote-invalid.out 2>&1; then
    rm -f /tmp/hydra-promote-invalid.out
    fail "invalid promotion digest was accepted"
fi
grep -Fq -- "STAGING_DIGEST" /tmp/hydra-promote-invalid.out || fail "invalid digest failure was not explicit"
rm -f /tmp/hydra-promote-invalid.out

if PROMOTE=yes \
    STAGING_DIGEST=sha256:0000000000000000000000000000000000000000000000000000000000000000 \
    STAGING_EGRESS_DIGEST=sha256:1111111111111111111111111111111111111111111111111111111111111111 \
    STAGING_URL=http://staging.example/healthz \
    STAGING_READY_URL=https://staging.example/readyz \
    sh scripts/promote-prod.sh v-test >/tmp/hydra-promote-insecure.out 2>&1; then
    rm -f /tmp/hydra-promote-insecure.out
    fail "insecure staging health URL was accepted"
fi
grep -Fq -- "STAGING_URL must be an HTTPS URL without credentials" /tmp/hydra-promote-insecure.out ||
    fail "insecure staging URL failure was not explicit"
rm -f /tmp/hydra-promote-insecure.out

echo "deployment safety: ok"
