# EP-055 Immutable Release Tags
Plan status: COMPLETE

## 1. Purpose / Big Picture
Remove the mutable `latest` Hydra image alias from the tag-triggered release
workflow so published artifacts, staging deployment, and rollback all use the
explicit immutable release tag and the already-captured digest.

## 2. Scope
Only the release workflow, its static policy/test gates, documentation, and
ExecPlan status/spec surfaces. Local Compose defaults and development image
behavior remain unchanged. No external release action is authorized.

## 3. Non-goals
No tag creation, registry login/publication, attestation, staging or
production deployment, Compose redesign, image rebuild, rollback execution,
or change to the Kernel.

## 4. Context and Orientation
EP-054 made staging and production promotion digest-bound and removed the
`latest-prod` alias from the promotion helper. At activation, the release
workflow still passed both `${TAG}` and `latest` to Buildx, while
`NEXUS_PACKAGE_CONTRACT.md`, `DEPLOYMENT.md`, and `ROLLBACK.md` required
explicit immutable versions. EP-055 closes that contradiction without
changing local Compose defaults.

## 5. Files to Read First
`AGENTS.md`; `COMMANDS.md`; `.agent/PLANS.md`;
`.agent/EXECUTION_RULES.md`; `SPEC-035-immutable-release-tags.md`;
`.github/workflows/release.yml`; `scripts/check-release-policy.sh`;
`scripts/test-deployment-safety.sh`; `DEPLOYMENT.md`; `ROLLBACK.md`;
`NEXUS_PACKAGE_CONTRACT.md`; `docker/compose.yaml`; `docker/nexus.env.example`;
`PRODUCTION_READINESS.md`; `DECISIONS.md`.

## 6. Files to Change
`.agent/specs/SPEC-035-immutable-release-tags.md`;
`.agent/execplans/EP-055-immutable-release-tags.md`;
`.agent/state/execplan-index.md`; `scripts/check-execplan-state.sh`;
`.github/workflows/release.yml`; `scripts/check-release-policy.sh`;
`scripts/test-deployment-safety.sh`; `COMMANDS.md`; `TESTING.md`;
`DEPLOYMENT.md`; `ROLLBACK.md`; `NEXUS_PACKAGE_CONTRACT.md`;
`PRODUCTION_READINESS.md`; `DECISIONS.md`.

## 7. Interfaces and Contracts
The release Buildx step retains one Hydra image tag:
`${REGISTRY}/hydra/kernel:${steps.tag.outputs.tag}`. Its digest output and
attestation fields remain unchanged. Local Compose continues to accept
`HYDRA_TAG` and its development default. Policy/test gates inspect only the
release job and cannot contact GitHub or a registry.

## 8. Milestones
M1 - Add SPEC-035, activate EP-055, and extend the state checker. Validation:
`bash scripts/check-execplan-state.sh` -> `execplan state: ok`.

M2 - Remove the mutable release alias and add a negative policy/regression
check. Validation: `bash scripts/test-deployment-safety.sh` and
`bash scripts/check-release-policy.sh` pass.

M3 - Reconcile deployment, rollback, package, testing, and readiness docs and
record ADR-0066. Validation: `git diff --check` and policy gates pass.

M4 - Run the full resource-safe verifier and complete the plan. Validation:
`bash scripts/verify.sh` -> `verify: ok`; state checker -> `execplan state:
ok`; no tag, registry, or deployment action occurred.

## 9. Concrete Steps
1. Delete only the `:latest` Hydra tag from the release Buildx `tags` block.
2. Require the release policy and deployment safety gate to reject that exact
   mutable Hydra alias while allowing `ubuntu-latest` and unrelated base image
   references.
3. State in deployment, rollback, and package docs that release consumers use
   explicit version tags/digests; retain local Compose defaults.
4. Add the decision and exact local evidence to the status/readiness ledger.

## 10. Validation and Acceptance
Run:

1. `bash scripts/check-execplan-state.sh`
2. `bash scripts/test-deployment-safety.sh`
3. `bash scripts/check-release-policy.sh`
4. `cargo fmt --all -- --check`
5. `git diff --check`
6. The resource-safe full verifier from `AGENTS.md`.

Expected markers: `execplan state: ok`, `deployment safety: ok`, `release
policy: ok`, and terminal `verify: ok`. No external release or deployment
command is allowed.

## 11. Idempotence and Recovery
Policy and documentation edits are repeatable and have no external side
effects. If a consumer depended on `latest`, it must be migrated to an
explicit version or digest by the owner; this plan does not recreate a mutable
alias. Rollback remains the prior explicit version tag.

## 12. Progress
- [x] M1 - SPEC, plan, and status activation
- [x] M2 - Remove mutable release alias and add regression gate
- [x] M3 - Documentation and decision reconciliation
- [x] M4 - Full validation and truthful completion

## 13. Surprises & Discoveries
- 2026-08-12 - EP-054 removed `latest-prod` from promotion but the release
  workflow still published the mutable `hydra/kernel:latest` alias.
- 2026-08-12 - Local Compose uses `HYDRA_TAG` and the Nexus example sets
  `HYDRA_TAG=local`; those development paths must remain unchanged.

## 14. Decision Log
- 2026-08-12 - Remove only the release workflow's Hydra `:latest` tag rather
  than changing Compose defaults. This separates local convenience from the
  immutable release boundary and preserves development compatibility.

## 15. Outcomes & Retrospective
M1-M4 are complete in the current worktree. The release workflow publishes
only the explicit version tag, both policy gates reject a reintroduced Hydra
`latest` alias, and deployment/package/rollback/testing/readiness documents
agree with that contract. Focused gates passed, and the resource-safe full
verifier exited 0 after 795.1 seconds on 2026-08-12. Registry publication,
tag-run attestation, staging, rollback, and operator release evidence remain
outside local acceptance and EP-010 remains partial.
