# SPEC-035 Immutable Release Tags
Status: Accepted | Owner: Hydra maintainers | Phase: 6 | ExecPlans: EP-055

## Goal
Keep the published Hydra release surface immutable and aligned with the
digest-bound staging and promotion contract.

## Normative requirements

1. The tag-triggered release workflow publishes the version tag derived from
   `GITHUB_REF` and does not publish `hydra/kernel:latest` or another mutable
   release alias.
2. The Buildx digest output remains available for attestation and staging.
3. Local Compose development defaults remain unchanged; removing a release
   alias must not remove local build/test usability.
4. Deployment and rollback documentation instruct operators to use an
   explicit immutable version tag.
5. The release policy gate fails if a mutable Hydra release alias is added
   back to the image-push job.

## Boundary

This is a workflow and documentation contract. It does not publish an image,
create a tag, deploy staging, promote production, or establish a signed
attestation. Registry and operator evidence remain EP-010 gates.
