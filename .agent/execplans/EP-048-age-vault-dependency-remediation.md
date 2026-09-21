# EP-048 Age Vault Dependency Remediation

Plan status: COMPLETE

## 1. Purpose / Big Picture

Upgrade Hydra's pinned age vault library from `0.11.1` to `0.12.1` so the
resolved localization macro edge can use `proc-macro-error3`, removing the
remaining unmaintained `proc-macro-error2` warning without changing Hydra's
validated encrypted-vault behavior or cryptographic boundary.

## 2. Scope

- Activate EP-048 as the only active plan.
- Add SPEC-028 and state/index evidence.
- Update only the age dependency and the lockfile edges Cargo proves needed.
- Run vault, BridgeHost, Kernel, security, dependency, and full verification
  gates against disposable local services.
- Update the ADR and readiness/security documentation truthfully.

## 3. Non-goals

- No vault artifact format migration or plaintext/re-encryption workflow.
- No custom cryptography, second secret store, or API authority surface.
- No broad workspace dependency refresh.
- No production deployment, registry publication, push, tag, or production DB.
- No claim of closing EP-010 staging, recovery-drill, or human evidence.

## 4. Context and Orientation

EP-047 removed Wasmtime's avoidable `fxhash` profiling edge. The only
remaining audit warning is `proc-macro-error2 2.0.1`, pulled by the pinned age
0.11.1 release through `i18n-embed-fl 0.9.4`. Current crate metadata shows
age 0.12.1 depends on the 0.10 localization line, whose latest 0.10.1
release uses `proc-macro-error3`. The upgrade is still security-sensitive:
age is Hydra's cryptographic vault boundary, so API compatibility, artifact
round-trip behavior, key rotation, backup/restore, and all policy gates must
be proved before closeout.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/state/execplan-index.md`
- `Cargo.toml`
- `Cargo.lock`
- `crates/bridge-host/Cargo.toml`
- `crates/bridge-host/src/vault.rs`
- `crates/bridge-host/src/lib.rs`
- `crates/vault-cli/src/main.rs`
- `crates/vault-cli/tests/recovery.rs`
- `scripts/check-wasmtime-features.sh`
- `scripts/security-check.sh`
- `scripts/dependency-audit.sh`
- `SECURITY.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`
- `.agent/specs/SPEC-028-age-vault-dependency.md`

## 6. Files to Change

- `Cargo.toml`
- `Cargo.lock`
- `scripts/check-age-vault-dependency.sh`
- `scripts/dependency-audit.sh`
- `COMMANDS.md`
- `SECURITY.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`
- `.agent/specs/SPEC-028-age-vault-dependency.md`
- `.agent/execplans/EP-048-age-vault-dependency-remediation.md`
- `.agent/state/execplan-index.md`

## 7. Interfaces and Contracts

- `age = { version = "=0.12.1", default-features = false }` remains an
  explicit workspace dependency.
- The resolved graph MUST contain no `proc-macro-error2` package and MUST
  retain the age 0.12.1 cryptographic package family.
- `scripts/check-age-vault-dependency.sh` verifies the manifest, locked graph,
  and absence of the old macro package; dependency audit runs it as a
  mandatory precondition.
- `EncryptedVault`, `VaultSecretSource`, `hydra-vault`, and all environment
  contracts remain source-compatible unless the compiler proves a minimal
  adaptation is required.

## 8. Milestones

### M1 - Activate the contract and state

Add SPEC-028, EP-048, the index row/transition, and state-checker coverage.
Run `bash scripts/preflight.sh` and `bash scripts/check-execplan-state.sh`.
Expected: `preflight: ok` and `execplan state: ok`.

### M2 - Upgrade and inspect the locked graph

Update the exact age version and run
`cargo update -p age --precise 0.12.1` followed by
`cargo tree --locked --offline -i proc-macro-error2` and
`cargo tree --locked --offline -i age`.
Expected: no `proc-macro-error2` package, age 0.12.1 remains the only direct
age version, and no unrelated direct dependency is introduced. Recovery:
inspect the first graph/API mismatch and revert only the age declaration if
the upgrade cannot preserve the vault contract.

### M3 - Make the dependency boundary mandatory

Add the graph assertion and wire it into `scripts/dependency-audit.sh`. Run
`bash scripts/check-age-vault-dependency.sh`, `bash scripts/security-check.sh`,
and `bash scripts/dependency-audit.sh`.
Expected: `age vault dependency policy: ok`, `security check: ok`, and
`dependency audit: ok`, with no old macro warning. Recovery: do not add an
advisory ignore; keep EP-048 active until the graph is corrected.

### M4 - Exercise the cryptographic and runtime boundaries

Run `cargo test -p bridge-host vault --locked --offline -- --nocapture`,
`cargo test -p hydra-vault --locked --offline -- --nocapture`, and
`cargo check -p hydra-kernel --tests --locked --offline`.
Expected: vault round-trip, wrong-key/tamper, rotation, backup/restore, CLI,
and Kernel compilation pass. Recovery: inspect any API or artifact mismatch;
do not alter the encrypted format to fit the dependency.

### M5 - Full acceptance and truthful closeout

Run the resource-safe full `bash scripts/verify.sh` against disposable
loopback Postgres and JetStream, then run the state checker and diff check.
Expected: `verify: ok`, `execplan state: ok`, and clean diff validation. No
production action is permitted.

## 9. Concrete Steps

1. Confirm EP-047 is complete and no active plan exists.
2. Add SPEC-028 and activate EP-048 in the state index/checker.
3. Update only the age pin and resolve the lockfile offline/with approved
   registry access.
4. Add the mandatory age graph check and run dependency/security gates.
5. Run focused vault/Kernel tests and correct only compatibility issues.
6. Update commands, security, readiness, and ADR documentation with exact
   evidence and remaining operator-owned gaps.
7. Run the full verifier and close EP-048 only after every acceptance signal
   passes.

## 10. Validation and Acceptance

EP-048 is accepted only when:

- the state checker finds one valid EP-048 plan;
- the manifest and locked graph use age 0.12.1;
- `proc-macro-error2` is absent and no unrelated direct dependency is added;
- encrypted vault and CLI tests pass without format changes;
- security, dependency, preflight, bridge, Kernel, and full gates pass;
- documentation accurately records the removed warning and remaining EP-010
  recovery/readiness gaps; and
- no production operation occurs.

## 11. Idempotence and Recovery

The version pin and graph check are idempotent. Re-running Cargo resolution
must preserve the exact age pin and remove only obsolete transitive edges. If
the upgrade changes artifact compatibility or fails a focused test, keep the
prior dependency declaration, document the blocker, and leave EP-048 ACTIVE.

## 12. Progress

- [x] M1 - Contract and active-plan state (`preflight: ok`; `execplan state: ok`, 2026-08-12)
- [x] M2 - Upgrade and locked graph (`cargo update -p age --precise 0.12.1`; `cargo fetch --locked`; `cargo tree --locked --offline -i age` resolved `age v0.12.1`; the inverse query for `proc-macro-error2` returned the expected package-not-found absence; no unrelated direct dependency was added, 2026-08-12)
- [x] M3 - Mandatory dependency gate (`age vault dependency policy: ok`; `security check: ok`; `dependency audit: ok`; no RustSec or unmaintained-package warning remained, 2026-08-12)
- [x] M4 - Vault and runtime regression tests (`bridge-host` vault: 6 passed; `hydra-vault`: 2 passed; `cargo check -p hydra-kernel --tests --locked --offline`: exit 0, 2026-08-12)
- [x] M5 - Full acceptance and closeout (resource-safe `bash scripts/verify.sh`: exit 0 in 1107.1s with `verify: ok`; `execplan state: ok`; `git diff --check` passed, 2026-08-12)

## 13. Surprises & Discoveries

- The latest age 0.12.1 metadata is available and still uses an empty default
  feature set, preserving Hydra's minimal age surface.
- age 0.12.1's i18n edge resolves through i18n-embed-fl 0.10.1, which uses
  proc-macro-error3 rather than proc-macro-error2.
- The upgrade must be validated as a cryptographic boundary change even
  though Hydra's source API is expected to remain stable.
- The inverse `cargo tree -i proc-macro-error2` query exits with Cargo's
  expected package-not-found status after remediation; the mandatory graph
  policy instead scans the complete locked workspace graph and passes.
- The first Kernel test-target check reached the shell's 124-second timeout
  during compilation; the identical command completed successfully with the
  documented extended timeout and no source change.

## 14. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-12 | Select age 0.12.1 for bounded advisory remediation | It is the current compatible release and its current localization macro edge removes the unmaintained proc-macro-error2 package. |
| 2026-08-12 | Preserve the existing vault artifact and API contract | Dependency remediation must not create plaintext exposure or a format migration. |
| 2026-08-12 | Activate EP-048 only after the state validator passed | Dependency edits must remain behind the repository's one-active-plan and preflight controls. |

## 15. Outcomes & Retrospective

EP-048 is complete. The pinned age vault dependency now resolves to 0.12.1,
the retired proc-macro-error2 graph edge is absent, and the encrypted-vault
artifact/API boundary remains covered by focused BridgeHost, hydra-vault,
Kernel, security, dependency, and full-repository gates. No production action
occurred. EP-010 owner custody, off-box recovery, staged drills, and human
evidence remain separate and open.
