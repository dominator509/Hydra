# EP-054 Deployment Safety and Immutable Artifacts
Plan status: COMPLETE

## 1. Purpose / Big Picture
Make Hydra's release-facing deployment helpers truthful and fail closed.
Carry the already-produced Buildx digest into staging, require pinned artifact
verification and SSH host trust, require both staging health and readiness for
promotion, and remove the mutable production alias from the promotion helper.

## 2. Scope
Only `scripts/deploy-staging.sh`, `scripts/promote-prod.sh`, the tag release
workflow and its local policy/test gate, plus the deployment documentation and
ExecPlan evidence surfaces. No remote connection, registry operation, tag,
push, deployment, or production database operation is authorized.

## 3. Non-goals
No staging deployment, production promotion, GitHub tag run, registry
publication, SSH credential generation, TLS implementation, image rebuild,
Compose topology redesign, or changes to Kernel runtime behavior. Do not claim
real attestation, staging health, or human release approval from local tests.

## 4. Context and Orientation
`DEPLOYMENT.md` requires immutable tested versions and separate authorization.
The release workflow already exposes `steps.push.outputs.digest` to its
attestation but does not pass it to staging. `deploy-staging.sh` accepts only a
tag and discards host-key verification through `accept-new` plus
`/dev/null`. `promote-prod.sh` warns and continues when `curl` is absent,
validates only health, and pushes `latest-prod` in addition to its immutable
tag. EP-054 makes these code paths fail closed without running them.

## 5. Files to Read First
`AGENTS.md`; `COMMANDS.md`; `.agent/PLANS.md`;
`.agent/EXECUTION_RULES.md`; `SPEC-034-deployment-safety.md`;
`DEPLOYMENT.md`; `ROLLBACK.md`; `PRODUCTION_READINESS.md`;
`scripts/deploy-staging.sh`; `scripts/promote-prod.sh`;
`scripts/check-release-policy.sh`; `.github/workflows/release.yml`;
`scripts/preflight.sh`; `scripts/verify.sh`; `TESTING.md`; `DECISIONS.md`.

## 6. Files to Change
`.agent/specs/SPEC-034-deployment-safety.md`;
`.agent/execplans/EP-054-deployment-safety-and-immutable-artifacts.md`;
`.agent/state/execplan-index.md`; `scripts/check-execplan-state.sh`;
`scripts/deploy-staging.sh`; `scripts/promote-prod.sh`;
`scripts/test-deployment-safety.sh`; `scripts/check-release-policy.sh`;
`.github/workflows/release.yml`; `scripts/preflight.sh`; `scripts/verify.sh`;
`COMMANDS.md`; `TESTING.md`; `DEPLOYMENT.md`; `PRODUCTION_READINESS.md`;
`DECISIONS.md`.

## 7. Interfaces and Contracts
Actual staging execution requires `STAGING_DIGEST` and
`STAGING_SSH_KNOWN_HOSTS`; the latter may be a known-hosts file path or its
owner-provided content. Promotion requires `STAGING_DIGEST`, `PROMOTE=yes`,
`curl`, successful health/readiness checks, Docker, and interactive
confirmation. The workflow passes the Buildx digest output and the
`STAGING_SSH_KNOWN_HOSTS` secret. Local dry-run behavior remains safe and
distinct from executed-promotion success.

## 8. Milestones
M1 - Add SPEC-034, activate EP-054, and extend the state checker. Validation:
`bash scripts/check-execplan-state.sh` -> `execplan state: ok`.

M2 - Harden both helpers with validated digest and host trust, fail-closed
health/readiness, and immutable-only promotion. Validation: shell syntax plus
static/safe helper tests.

M3 - Wire workflow digest/known-hosts inputs and extend release policy;
document the operator contract and local-only evidence. Validation:
`bash scripts/test-deployment-safety.sh` -> `deployment safety: ok` and
`bash scripts/check-release-policy.sh` -> `release policy: ok`.

M4 - Run full local verification and reconcile readiness/plan state. Validation:
`bash scripts/verify.sh` -> `verify: ok`, state checker -> `execplan state:
ok`, formatter and diff checks pass.

## 9. Concrete Steps
1. Add reusable POSIX shell validation for safe tag/digest inputs and make
   staging actual execution require a digest and known-hosts record.
2. Materialize private temporary key/known-hosts files without printing their
   contents; use strict host checking and remove them on exit.
3. Pull the exact staging image by digest, verify its local image reference,
   then run the existing Compose and health/readiness flow.
4. Make promotion require `curl`, health plus readiness, and the same digest;
   tag and push only `${TAG}-prod`, never `latest-prod`.
5. Distinguish safe dry-run output from an executed promotion marker.
6. Pass digest and known-hosts through the release workflow and enforce the
   contract in the local policy/test gates.

## 10. Validation and Acceptance
Run:

1. `bash scripts/check-execplan-state.sh`
2. `sh -n scripts/deploy-staging.sh scripts/promote-prod.sh scripts/test-deployment-safety.sh`
3. `bash scripts/test-deployment-safety.sh`
4. `bash scripts/check-release-policy.sh`
5. `cargo fmt --all -- --check`
6. `git diff --check`
7. The resource-safe full verifier from `AGENTS.md`.

Expected markers are `deployment safety: ok`, `release policy: ok`, and the
full verifier's terminal `verify: ok`. No command may contact a registry,
SSH host, staging URL, or production service.

## 11. Idempotence and Recovery
Static policy and dry-run tests have no external side effects. Re-running an
actual deployment remains operator-controlled and digest-pinned. If a remote
pull, host-key check, health/readiness check, or image inspection fails, the
helper exits before Compose promotion and reports the bounded failure. A
failed promotion never creates a mutable alias or silently continues.

## 12. Progress
- [x] M1 - SPEC, plan, and status activation
- [x] M2 - Fail-closed helper implementation
- [x] M3 - Workflow, policy, tests, and documentation
- [x] M4 - Full validation and truthful reconciliation

## 13. Surprises & Discoveries
- 2026-08-12 - The release workflow already captures a Buildx digest for
  attestation but did not pass it into the staging deployment helper.
- 2026-08-12 - Promotion skipped staging validation when `curl` was missing,
  and pushed a mutable `latest-prod` alias in addition to the versioned tag.

## 14. Decision Log
- 2026-08-12 - Require the existing Buildx digest output rather than inventing
  a second artifact identity. The deployment helper must verify the exact
  digest before Compose or promotion.
- 2026-08-12 - Require owner-supplied known-hosts and strict SSH checking;
  `accept-new` with a discarded file is not sufficient for a release boundary.
- 2026-08-12 - Remove `latest-prod` from the promotion helper. Consumers must
  use the immutable production tag until a separately reviewed alias policy
  exists.

## 15. Outcomes & Retrospective
EP-054 is complete. Deployment tooling now binds actual staging and promotion
to the Buildx image digest, uses owner-provided strict SSH host trust, requires
health and readiness before promotion, and avoids the mutable `latest-prod`
alias. Dry-run output is distinct from executed-promotion output.

Evidence:

- `bash scripts/check-execplan-state.sh` -> `execplan state: ok` while EP-054
  was active.
- `bash scripts/test-deployment-safety.sh` -> `deployment safety: ok`.
- `bash scripts/check-release-policy.sh` -> `release policy: ok`.
- `bash scripts/preflight.sh` -> `preflight: ok`; shell syntax, format, and
  diff checks passed.
- The resource-safe `bash scripts/verify.sh` exited 0 in 586.5 seconds on
  2026-08-12 and included the deployment safety gate.

The first post-completion audit found and corrected a stale `PROD_LATEST`
reference that could have caused an authorized promotion to fail after its
immutable push. The regression gate was strengthened to reject both the alias
name and the stale variable reference. The corrected focused gates and full
verifier now pass.

Final evidence:

- `bash scripts/check-execplan-state.sh` -> `execplan state: ok` during the
  correction cycle.
- `bash scripts/test-deployment-safety.sh` -> `deployment safety: ok`.
- `bash scripts/check-release-policy.sh` -> `release policy: ok`.
- `bash scripts/preflight.sh` -> `preflight: ok`; shell syntax, format, and
  diff checks passed.
- The corrected resource-safe `bash scripts/verify.sh` exited 0 in 676.7
  seconds on 2026-08-12 and included the deployment safety gate.

No registry, SSH host, staging URL, tag, push, deployment, promotion, or
production database operation was performed. Signed tag-run attestations,
real staging validation, rollback, and human release evidence remain EP-010
gates.
