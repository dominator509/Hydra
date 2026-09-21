# EP-021 Release Provenance and Truthful Nightly Gates

Plan status: COMPLETE

## 1. Purpose / Big Picture

Close the next code-owned EP-010 release and validation gaps without confusing workflow policy with executed release evidence. The tag-triggered workflow will explicitly request BuildKit provenance and SBOM output, create a signed GitHub artifact attestation for the immutable image digest, and fail closed when the attestation service or required permissions are unavailable. The nightly workflow will delegate ignored conformance execution to a checked-in wrapper that proves the required soak test was discovered and passed instead of accepting an empty test set as success.

## 2. Scope

- Add explicit release job permissions for image publication and signed artifact attestations.
- Enable `provenance: mode=max` and `sbom: true` on the release image build.
- Attest the pushed image digest with `actions/attest@v4` and `push-to-registry: true`.
- Add a local static release-policy checker that validates the workflow contract and rejects masked required checks.
- Add a nightly conformance wrapper that fails when the required ignored soak test is absent, fails, or is not discovered.
- Calibrate the test-only 10,000-record soak grant so its existing fuel-exhaustion assertion measures the workload rather than an undersized fixture budget.
- Make the policy checker a required full-verifier gate and the wrapper a required nightly workflow step.
- Update release, package, security, operations, readiness, and plan-state documentation truthfully.
- Preserve EP-010 as partial until an authorized tag run, staging evidence, and human-owned production gates exist.

## 3. Non-goals

- No tag creation, image push, registry publication, staging deployment, production deployment, or production database operation.
- No change to Hydra runtime behavior, CRM data, event schemas, migrations, or adapter ABI.
- No change to production BridgeHost fuel policy; only the named conformance fixture may be adjusted.
- No claim that a signed attestation, SBOM, or release digest has been produced in this local run.
- No replacement of GitHub's hosted attestation service with a guessed or unsigned fallback.
- No full staging soak, D1-D5 drill, real TLS/IdP validation, recovery drill, performance benchmark, accessibility review, or human launch sign-off.
- No new Rust, Node, npm, or workflow dependency beyond the explicitly reviewed GitHub action reference.

## 4. Context and Orientation

At EP-021 activation, EP-010 remained partial. EP-020 had closed local readiness projection and backup/restore-helper safety gaps, but `DEPLOYMENT.md`, `NEXUS_PACKAGE_CONTRACT.md`, and `PRODUCTION_READINESS.md` still identified release provenance as undefined. The pre-EP-021 `.github/workflows/release.yml` pushed images without explicit provenance/SBOM inputs or a signed attestation, and the pre-EP-021 `.github/workflows/nightly.yml` ran ignored conformance tests directly without proving that a required ignored test was discovered. EP-021 makes those contracts executable and keeps external execution evidence separate from repository policy.

## 5. Files to Read First

`AGENTS.md`; `COMMANDS.md`; `.agent/PLANS.md`; `.agent/EXECUTION_RULES.md`; `.agent/state/execplan-index.md`; `.agent/execplans/EP-010-production-readiness.md`; `.agent/execplans/EP-020-operational-readiness-and-recovery-safety.md`; `.github/workflows/release.yml`; `.github/workflows/nightly.yml`; `scripts/preflight.sh`; `scripts/verify.sh`; `PRODUCTION_READINESS.md`; `DEPLOYMENT.md`; `NEXUS_PACKAGE_CONTRACT.md`; `SECURITY.md`; `OPERATIONS.md`; `DECISIONS.md`.

## 6. Files to Change

- `.agent/execplans/EP-021-release-provenance-and-truthful-nightly-gates.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `scripts/check-release-policy.sh`
- `scripts/test-nightly-conformance.sh`
- `crates/bridge-host/tests/conformance.rs`
- `scripts/preflight.sh`
- `scripts/verify.sh`
- `.github/workflows/release.yml`
- `.github/workflows/nightly.yml`
- `COMMANDS.md`
- `DEPLOYMENT.md`
- `NEXUS_PACKAGE_CONTRACT.md`
- `SECURITY.md`
- `OPERATIONS.md`
- `PRODUCTION_READINESS.md`
- `NEXUS_INTEGRATION_AUDIT.md`
- `.agent/execplans/EP-010-production-readiness.md`
- `DECISIONS.md`

## 7. Interfaces and Contracts

- `bash scripts/check-release-policy.sh` prints `release policy: ok` only when the release workflow contains explicit BuildKit `provenance: mode=max`, `sbom: true`, digest capture, `actions/attest@v4`, image subject name/digest, registry publication, and required least-privilege permissions.
- `bash scripts/test-nightly-conformance.sh` runs the required ignored bridge conformance target, preserves the command's exit status, proves a positive test count, proves `c9_soak_10k` was discovered, proves `test result: ok.`, and prints `nightly conformance: ok` only after all checks pass.
- The nightly workflow invokes the wrapper rather than duplicating the raw ignored-test command. A missing or empty ignored test set is a failure.
- The release build captures the immutable digest from the Buildx step and passes the untagged fully qualified image name plus `sha256:` digest to `actions/attest@v4`. Attestation failure fails the release job; no unsigned fallback is allowed.
- Release workflow permissions are limited to `contents: read`, `packages: write`, `id-token: write`, and `attestations: write` on the image job. `create-storage-record: false` is explicit because this checkout is a private personal repository and the action's storage-record path is organization-owned; no `artifact-metadata` permission is requested.
- Local policy validation proves repository configuration only. It never asserts that GitHub created an attestation or that a registry accepted the image.
- The plan-state checker validates EP-021's required sections/status in addition to EP-011 through EP-020.

## 8. Milestones

### M1 - Activate EP-021 and record the release evidence boundary

Add EP-021 to the authoritative index, extend the state checker, record the transition from EP-020, and document that local policy is not a tag-run attestation.

Validation: `bash scripts/check-execplan-state.sh`

Expected result: `execplan state: ok`.

Recovery: if the checker reports a missing section, status, or duplicate active row, repair the index/plan pair with `apply_patch` and rerun the checker before touching workflows.

### M2 - Add fail-fast nightly conformance discovery

Implement the POSIX shell wrapper with captured output, preserved cargo status, positive test-count detection, required `c9_soak_10k` discovery, and an explicit successful result marker. If the named soak exposes a fixture-budget failure, adjust only its explicit test grant while retaining the fuel-headroom assertion. Replace the direct nightly command with the wrapper.

Validation: `bash scripts/test-nightly-conformance.sh`

Expected result: the ignored soak test runs and the script prints `nightly conformance: ok`.

Recovery: inspect the captured cargo output and run the exact bridge conformance target once as a narrower diagnostic. If no ignored test is discovered, fail and repair the test target or workflow; never weaken the discovery check.

### M3 - Configure signed release provenance and static policy validation

Add least-privilege release permissions, explicit BuildKit provenance/SBOM settings, digest capture, and `actions/attest@v4`. Add the static policy checker and make it reject masked required workflow checks.

Validation: `bash scripts/check-release-policy.sh`

Expected result: `release policy: ok`.

Recovery: compare each named contract to the official action input names and the exact workflow lines; do not replace attestation with a warning or a continue-on-error path.

### M4 - Wire mandatory local gates and update the evidence ledger

Add both scripts to preflight inventory, run the release-policy checker from `verify.sh`, update `COMMANDS.md`, and reconcile release/SBOM/provenance language in deployment, security, operations, package, audit, EP-010, and production-readiness documents. Update `DECISIONS.md` with the action and fail-closed decision.

Validation: `bash scripts/preflight.sh`

Expected result: `preflight: ok`.

Recovery: update the owning command or documentation contract when inventory checks fail; do not remove a required gate or claim an unexecuted tag run.

### M5 - Full local acceptance and status reconciliation

Run the wrapper, policy checker, state checker, shell syntax checks, `git diff --check`, and the full verifier against the existing isolated local test services. Record exact results and preserve all remaining EP-010 operator/human gaps.

Validation: `bash scripts/verify.sh`

Expected result: terminal `verify: ok`; no tag, push, registry publication, deployment, or production database operation.

Recovery: follow AGENTS.md §7 bounded retry. If the full verifier encounters a service problem, reuse the documented isolated Postgres/NATS path; stop only for an AGENTS.md §4 condition.

## 9. Concrete Steps

1. Activate EP-021 in the index and extend the state checker before changing workflows.
2. Add the nightly wrapper and run its focused validation.
3. Calibrate the named test-only soak grant if required, then replace the direct nightly ignored-test command with the wrapper.
4. Add release permissions, BuildKit provenance/SBOM inputs, digest capture, and signed attestation.
5. Add the static policy checker and make it a full-verifier gate.
6. Add both script names to preflight and document their commands and expected outputs.
7. Reconcile release and production-readiness documentation without fabricating tag-run evidence.
8. Run all milestone validations in order, then full verification and a changed-file review.

## 10. Validation and Acceptance

- `bash scripts/check-execplan-state.sh` prints `execplan state: ok`.
- `bash scripts/test-nightly-conformance.sh` discovers and passes `c9_soak_10k` and prints `nightly conformance: ok`.
- `bash scripts/check-release-policy.sh` prints `release policy: ok` and rejects missing provenance, SBOM, attestation, permissions, or masked required checks.
- `bash scripts/preflight.sh` prints `preflight: ok` and requires both new script artifacts.
- `bash scripts/verify.sh` prints terminal `verify: ok` without masked failures.
- The nightly workflow cannot claim success from an empty ignored test set.
- The release workflow cannot publish an image without explicit provenance/SBOM policy and a signed attestation attempt.
- Documentation says policy is configured but real tag-run attestation evidence remains unverified locally.
- EP-010 remains partial; staging, recovery, soak, review, and human sign-off are not relabeled as passed.
- The changed-file set is contained in section 6, with any formatter/generated change justified in the Decision Log.
- No production deployment, tag, push, registry publication, or production database operation occurs.

## 11. Idempotence and Recovery

The state checker and policy checker are read-only. The nightly wrapper uses a temporary output file and removes it on exit; rerunning it does not mutate repository state. Workflow edits are deterministic and can be reapplied without changing the release contract. The release job's digest and attestation are created only by an authorized GitHub tag run, not by local validation. If interrupted, inspect Progress, rerun the first unchecked milestone, and preserve unrelated worktree changes.

## 12. Progress

- [x] M1 - EP-021 active, state checker/index, and release evidence boundary recorded (`bash scripts/check-execplan-state.sh` -> `execplan state: ok`; 2026-08-11).
- [x] M2 - Fail-fast nightly conformance discovery wrapper implemented and workflow wired (`bash scripts/test-nightly-conformance.sh` -> `nightly conformance: ok`; `c9_soak_10k` passed; 2026-08-11).
- [x] M3 - Signed release provenance/SBOM workflow policy and static checker implemented (`bash scripts/check-release-policy.sh` -> `release policy: ok`; `sh -n scripts/check-release-policy.sh` passed; 2026-08-11).
- [x] M4 - Mandatory local gates and documentation updated (`bash scripts/preflight.sh` -> `preflight: ok`; 2026-08-11).
- [x] M5 - Full local acceptance, diff review, and outcomes recorded (`bash scripts/test-nightly-conformance.sh` -> `nightly conformance: ok`; `bash scripts/check-release-policy.sh` -> `release policy: ok`; `bash scripts/check-execplan-state.sh` -> `execplan state: ok`; `bash scripts/preflight.sh` -> `preflight: ok`; `sh -n` and `git diff --check` passed; full `bash scripts/verify.sh` exited 0 after 209.9 seconds with the required gate markers; 2026-08-11).

## 13. Surprises & Discoveries

- 2026-08-11: The first wrapper run correctly discovered `c9_soak_10k` but exposed an existing Wasm fuel trap during the 10,000-record `probe`; the wrapper did not mask it.
- 2026-08-11: A `200_000_000` test-only grant still exhausted fuel during `BTreeMap` insertion. Source inspection confirmed that `probe` rebuilds both a 10,000-entry map and change log; the 250-record pagination comparison passed.
- 2026-08-11: A bounded `2_000_000_000` test-only grant passed the named soak while retaining the existing `fuel_remaining() > 0` assertion. Production BridgeHost fuel policy was not changed.
- 2026-08-11: The repository is private and user-owned, so the release attestation explicitly disables the action's organization-only storage-record path and removes `artifact-metadata: write`; private-repository hosted attestation still requires the GitHub capability/plan to be available at tag-run time.

## 14. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-11 | Use `actions/attest@v4` rather than the legacy wrapper | The official action documentation identifies the current action as the preferred implementation and accepts an image name plus Buildx digest. |
| 2026-08-11 | Keep BuildKit `provenance: mode=max` and `sbom: true` explicit | Build metadata and SBOM generation remain visible in the image build contract; the signed attestation is an additional release boundary, not a substitute. |
| 2026-08-11 | Fail closed when attestation permissions/service support are unavailable | An unsigned image must not be presented as a reviewed release artifact. Local policy cannot fabricate hosted attestation evidence. |
| 2026-08-11 | Require named ignored-test discovery in nightly validation | A zero-test `cargo test --ignored` invocation can exit successfully while proving no soak behavior. |
| 2026-08-11 | M1 state validation passed | `bash scripts/check-execplan-state.sh` returned `execplan state: ok`; EP-021 is the only ACTIVE plan. |
| 2026-08-11 | Use a separate explicit soak grant of `2_000_000_000` fuel | The named 10k fixture performs deterministic BTreeMap/change-log construction inside the sandbox; the ordinary `20_000_000` grant is appropriate for small conformance cases but traps before the soak can validate pagination. |
| 2026-08-11 | M2 validation passed | The exact wrapper returned `nightly conformance: ok`; the named `c9_soak_10k` test reported `1 passed`, and no failure was suppressed. |
| 2026-08-11 | M3 validation passed | `bash scripts/check-release-policy.sh` returned `release policy: ok`; the release workflow contract is statically validated without contacting GitHub. |
| 2026-08-11 | M4 validation passed | `bash scripts/preflight.sh` returned `preflight: ok` with the expected local `.env` note; the policy and nightly scripts are now mandatory artifacts/gates. |
| 2026-08-11 | Disable organization-only artifact storage records | The current repository is a private personal repository. Registry attestation remains requested, while `create-storage-record: false` and removal of `artifact-metadata: write` keep the workflow within the action's documented permission boundary. |
| 2026-08-11 | M5 validation passed | Focused gates, shell syntax, whitespace, and the full verifier passed; no tag, push, registry publication, attestation, deployment, or production database operation occurred. |

## 15. Outcomes & Retrospective

EP-021 is complete for the code-owned release-policy and nightly-discovery seam. The repository now fails closed when required hosted attestation support is unavailable, but no tag-run attestation or registry verification was executed. EP-010 remains partial until an authorized release/tag run, staging drills, recovery evidence, and human sign-off are completed.
