#!/usr/bin/env sh
# Validate release and nightly workflow contracts without contacting GitHub.
set -eu

release=.github/workflows/release.yml
nightly=.github/workflows/nightly.yml

fail() {
  echo "release policy ERROR: $1" >&2
  exit 1
}

[ -f "$release" ] || fail "missing $release"
[ -f "$nightly" ] || fail "missing $nightly"

require_in_text() {
  text=$1
  pattern=$2
  description=$3
  if ! printf '%s\n' "$text" | grep -Fq -- "$pattern"; then
    fail "$description"
  fi
}

require_in_file() {
  file=$1
  pattern=$2
  description=$3
  if ! grep -Fq -- "$pattern" "$file"; then
    fail "$description"
  fi
}

build_job=$(awk '
  /^  build-and-push:/ { in_build=1 }
  /^  deploy-staging:/ { in_build=0 }
  in_build { print }
' "$release")

deploy_job=$(awk '
  /^  deploy-staging:/ { in_deploy=1 }
  in_deploy { print }
' "$release")

require_in_text "$build_job" "permissions:" "build job permissions are not declared"
require_in_text "$build_job" "contents: read" "contents read permission is missing"
require_in_text "$build_job" "packages: write" "packages write permission is missing"
require_in_text "$build_job" "id-token: write" "OIDC id-token permission is missing"
require_in_text "$build_job" "attestations: write" "attestations write permission is missing"
require_in_text "$build_job" "id: push" "Buildx digest step id is missing"
require_in_text "$build_job" "uses: docker/build-push-action@v6" "pinned Buildx action reference is missing"
require_in_text "$build_job" "push: true" "release image push is missing"
require_in_text "$build_job" "provenance: mode=max" "explicit maximum BuildKit provenance is missing"
require_in_text "$build_job" "sbom: true" "explicit BuildKit SBOM generation is missing"
require_in_text "$build_job" "uses: actions/attest@v4" "signed artifact attestation action is missing"
require_in_text "$build_job" "subject-name:" "attestation subject name is missing"
require_in_text "$build_job" 'subject-digest: ${{ steps.push.outputs.digest }}' "attestation does not bind the pushed digest"
require_in_text "$build_job" "push-to-registry: true" "registry attestation publication is missing"
require_in_text "$build_job" "create-storage-record: false" "organization-only storage records must be disabled for this repository"
require_in_text "$build_job" "outputs:" "build job must expose the pushed digest to deployment"
require_in_text "$build_job" 'image_digest: ${{ steps.push.outputs.digest }}' "build digest output is not wired"
require_in_text "$build_job" 'egress_image_digest: ${{ steps.push-egress.outputs.digest }}' "egress build digest output is not wired"
require_in_text "$build_job" "docker/egress-proxy.Dockerfile" "egress image build is missing"
require_in_text "$build_job" "id: push-egress" "egress Buildx digest step id is missing"
require_in_text "$build_job" 'subject-name: ${{ env.REGISTRY }}/hydra/egress-proxy' "egress attestation subject name is missing"
require_in_text "$build_job" 'subject-digest: ${{ steps.push-egress.outputs.digest }}' "egress attestation does not bind the pushed digest"
if printf '%s\n' "$build_job" | grep -Fq -- '${{ env.REGISTRY }}/hydra/kernel:latest'; then
  fail "release workflow publishes a mutable Hydra latest alias"
fi
require_in_text "$deploy_job" 'STAGING_DIGEST: ${{ needs.build-and-push.outputs.image_digest }}' "staging deployment does not receive the pushed digest"
require_in_text "$deploy_job" 'STAGING_EGRESS_DIGEST: ${{ needs.build-and-push.outputs.egress_image_digest }}' "staging deployment does not receive the egress digest"
require_in_text "$deploy_job" 'STAGING_SSH_KNOWN_HOSTS: ${{ secrets.STAGING_SSH_KNOWN_HOSTS }}' "staging deployment does not receive pinned host trust"
require_in_text "$deploy_job" 'STAGING_SMOKE_URL: ${{ secrets.STAGING_SMOKE_URL }}' "staging deployment does not receive a public ingress smoke URL"
require_in_file "scripts/deploy-staging.sh" "StrictHostKeyChecking=yes" "staging deployment does not require strict SSH host checking"
require_in_file "scripts/deploy-staging.sh" "STAGING_SMOKE_URL must be set for deploy" "staging deployment does not require an ingress smoke URL"
require_in_file "scripts/deploy-staging.sh" "https://?*)" "staging deployment does not enforce HTTPS smoke validation"
require_in_file "scripts/deploy-staging.sh" "STAGING_EGRESS_DIGEST" "staging deployment does not require the egress digest"
require_in_file "scripts/promote-prod.sh" "STAGING_EGRESS_DIGEST" "promotion does not require the egress digest"
require_in_file "scripts/promote-prod.sh" "PROD_EGRESS_IMAGE" "promotion does not promote the egress image"
require_in_file "scripts/promote-prod.sh" "curl is required for staging health validation" "promotion does not fail closed when curl is unavailable"
require_in_file "scripts/promote-prod.sh" "validate_https_url" "promotion does not validate staging URLs"
require_in_file "scripts/promote-prod.sh" 'validate_https_url "STAGING_URL" "$STAGING_URL"' "promotion does not enforce HTTPS health validation"
require_in_file "scripts/promote-prod.sh" 'validate_https_url "STAGING_READY_URL" "$STAGING_READY_URL"' "promotion does not enforce HTTPS readiness validation"
if grep -Eq -- 'PROD_LATEST|latest-prod' scripts/promote-prod.sh; then
  fail "promotion helper creates or references a mutable production alias"
fi

require_in_file "$release" "run: bash scripts/verify.sh" "release verification gate is missing"
require_in_file "$nightly" "bash scripts/test-performance.sh" "shared nightly performance gate is missing"
require_in_file "scripts/test-performance.sh" "scripts/test-nightly-conformance.sh" "performance gate does not include named conformance soak"
require_in_file "scripts/test-performance.sh" "scripts/cache-hit-audit.sh" "performance gate does not include TOKENKILLER cache audit"
if grep -Fq -- "cargo test -p bridge-host --test conformance -- --ignored" "$nightly"; then
  fail "nightly workflow duplicates the raw ignored-test command instead of using the wrapper"
fi

for workflow in "$release" "$nightly"; do
  if grep -Fq -- "continue-on-error" "$workflow"; then
    fail "$workflow contains continue-on-error"
  fi
  if grep -Fq -- "|| true" "$workflow"; then
    fail "$workflow contains a masked success path"
  fi
done

if grep -Fq -- "attest-build-provenance" "$release"; then
  fail "release workflow uses the legacy attestation wrapper"
fi

echo "release policy: ok"
