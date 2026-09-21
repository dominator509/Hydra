# SPEC-034 Deployment Safety and Immutable Artifact Contract
Status: Accepted | Owner: Hydra maintainers | Phase: 6 | ExecPlans: EP-054

## Goal
Ensure Hydra's checked-in staging and promotion helpers cannot silently
deploy a mutable image, bypass host identity verification, or report a
successful promotion when required validation was skipped.

## Normative requirements

1. A non-dry-run staging deployment must receive a valid image digest in the
   form `sha256:` followed by 64 lowercase hexadecimal characters.
2. The staging helper must pull and verify the digest-pinned image before
   starting Compose. A mutable tag may select the local image name but must
   not be the artifact proof.
3. SSH host verification must use a supplied known-hosts record with
   `StrictHostKeyChecking=yes`. Accept-new plus a discarded known-hosts file is
   not an acceptable staging trust boundary.
4. A production promotion must fail closed if `curl` is unavailable or if
   either staging health or readiness fails. It must validate the same pinned
   digest before tagging.
5. Promotion must publish only the immutable production tag. A mutable
   `latest-prod` alias is not created by the helper.
6. A dry-run must report `promote-gate: dry-run ok`, not the same success
   marker as an executed promotion.
7. The tag-triggered workflow must pass the Buildx digest output and the
   owner-controlled SSH known-hosts secret to the staging helper.
8. Static and safe behavioral tests must cover the above without network,
   registry, SSH, production, or staging access.

## Compatibility and safety boundary

Existing tag, registry, SSH, health, and Compose concepts remain. The new
digest and known-hosts inputs are required only for actual staging/promotion,
not for local dry-runs. No production deployment or image publication is
performed by this plan.
