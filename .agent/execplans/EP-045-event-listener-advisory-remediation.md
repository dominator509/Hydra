# EP-045 Event Listener Advisory Remediation

Plan status: COMPLETE

## 1. Purpose / Big Picture

Remove the known transitive `event-listener 5.4.1` unsoundness from Hydra's
dependency graph. RustSec advisory `RUSTSEC-2026-0221` identifies a safe-code
thread-safety issue in `StackSlot`; the patched floor is `5.4.2`. The affected
crate is pulled by the repository's vendored SQLx 0.8.6 stack, so the smallest
safe remediation is a compatible lockfile update rather than a runtime or
SQL abstraction redesign.

## 2. Scope

- Resolve `event-listener` to the patched compatible release.
- Confirm no vulnerable duplicate remains in the complete target graph.
- Run cargo audit/deny, security/dependency, preflight, and full verifier gates.
- Update the production-readiness residual and record the decision.

## 3. Non-goals

- No application code, SQL queries, migrations, schema, or runtime behavior.
- No direct dependency added solely to force a version.
- No SQLx replacement, vendor refresh, Tokio/Wasmtime change, or broad lockfile
  churn beyond resolver-required compatible entries.
- No advisory ignore, production deployment, tag, push, or database operation.

## 4. Context and Orientation

`cargo tree -i event-listener --target all --offline` shows `event-listener
5.4.1` only through `vendor/sqlx` -> `sqlx-core` and `sqlx-postgres`. The
current security gate reports it as the allowed unsound warning
`RUSTSEC-2026-0221`; the two unrelated unmaintained transitive warnings remain
separately documented. RustSec identifies `>=5.4.2` as patched. Cargo's
existing semver requirements should permit a lockfile-only update.

## 5. Files to Read First

`AGENTS.md`; `COMMANDS.md`; `.agent/PLANS.md`;
`.agent/EXECUTION_RULES.md`; `.agent/state/execplan-index.md`;
`Cargo.toml`; `Cargo.lock`; `vendor/sqlx/Cargo.toml`;
`vendor/sqlx-core/Cargo.toml`; `deny.toml`; `scripts/security-check.sh`;
`scripts/dependency-audit.sh`; `PRODUCTION_READINESS.md`; `DECISIONS.md`;
`.agent/specs/SPEC-025-event-listener-advisory-remediation.md`.

## 6. Files to Change

- `.agent/specs/SPEC-025-event-listener-advisory-remediation.md`
- `.agent/execplans/EP-045-event-listener-advisory-remediation.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `Cargo.lock`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`

## 7. Interfaces and Contracts

- The workspace dependency manifest remains unchanged unless Cargo proves a
  resolver constraint requires a minimal compatible edit.
- The lockfile must contain one patched `event-listener` version at or above
  `5.4.2`.
- No `RUSTSEC-2026-0221` ignore may be added to `deny.toml`, scripts, or CI.
- The existing vendored SQLx public API and checked-query snapshots remain
  unchanged.
- A green audit/deny command must be counted only when it scans the current
  lockfile and reports its required marker.

## 8. Milestones

### M1 - Activate plan and validate state

Add EP-045/SPEC-025, make EP-045 the only active row, extend the checker, and
record the EP-044 to EP-045 transition.

Validation: `bash scripts/check-execplan-state.sh`

Expected result: `execplan state: ok` with EP-045 as the only `ACTIVE` row.

Recovery: repair the index, status, or checker before changing dependencies.

### M2 - Resolve the patched transitive crate

Run `cargo update -p event-listener --precise 5.4.2` (or a newer compatible
patched release if Cargo requires it), inspect the lockfile diff, and reject
unrelated dependency churn. Confirm the reverse dependency remains the
vendored SQLx path.

Validation: `cargo tree -i event-listener --target all --offline`

Expected result: one patched `event-listener` release at or above `5.4.2`,
with the existing SQLx reverse-dependency path.

Recovery: if the version is unavailable locally, use the documented network
Cargo update path; if the resolver requires broad incompatible churn, stop
the dependency approach and record the exact blocker rather than forcing a
direct dependency.

### M3 - Run security and dependency gates

Run audit, deny, security, and dependency checks against the updated lockfile.
The old unsound warning must disappear; unrelated allowed warnings must remain
explicit rather than being suppressed.

Validation: `cargo audit`, `cargo deny check`,
`bash scripts/security-check.sh`, and `bash scripts/dependency-audit.sh`

Expected result: no vulnerabilities, no `RUSTSEC-2026-0221` warning, and each
repository marker is green.

Recovery: inspect the exact advisory or policy failure and make only the
smallest compatible lockfile/policy correction; never add an ignore to hide a
new vulnerability.

### M4 - Reconcile documentation and local gates

Remove only the remediated event-listener item from the production-readiness
supply-chain residual, preserve the two unrelated unmaintained warnings, add
ADR-0056, and run preflight/state/diff checks.

Validation: `bash scripts/preflight.sh`,
`bash scripts/check-execplan-state.sh`, and `git diff --check`

Expected result: `preflight: ok`, `execplan state: ok`, and no diff errors.

Recovery: preserve the old residual if the current audit still reports it;
do not document a remediation before the scan proves it.

### M5 - Full local acceptance and close the plan

Run the unchanged full verifier against the existing isolated loopback
services. Record exact output, changed-file review, remaining warnings, and
the fact that no production operation occurred.

Validation: `bash scripts/verify.sh`

Expected result: terminal `verify: ok` and no advisory regression.

Recovery: follow AGENTS.md section 7 and keep EP-045 active until all required
gates pass.

## 9. Concrete Steps

1. Activate EP-045 and validate state.
2. Update the lockfile to patched `event-listener`.
3. Inspect the dependency tree and run audit/deny/security/dependency gates.
4. Reconcile readiness and decision documentation.
5. Run preflight, diff, and the full verifier, then close the state ledger.

## 10. Validation and Acceptance

- EP-045 is the only active plan during implementation.
- `event-listener` is patched at or above `5.4.2` with no vulnerable duplicate.
- Audit and deny pass without an advisory ignore for the unsoundness.
- The SQLx vendor boundary and checked metadata remain intact.
- Preflight, diff, state, and full verifier pass with required markers.
- The readiness ledger removes only the remediated residual and retains
  unrelated supply-chain and operator-owned gaps.
- No production action occurred.

## 11. Idempotence and Recovery

Cargo update is deterministic once the lockfile and registry cache are fixed;
re-running it at the same precise version is idempotent. Before any update,
capture the lockfile diff and inspect it. If a resolver failure occurs, retain
EP-045 ACTIVE, record the exact error, and do not hand-edit checksums or add a
direct dependency. All audits and verifiers are read-only apart from normal
build artifacts.

## 12. Progress

- [x] M1 - EP-045 activated and state validation passed.
- [x] M2 - Patched event-listener lockfile resolution (`cargo tree -i event-listener --target all --offline` -> `event-listener v5.4.2` through vendored SQLx 0.8.6, 2026-08-12).
- [x] M3 - Audit, deny, security, and dependency gates (`cargo audit` -> no vulnerabilities and only the two documented unmaintained warnings; `cargo deny check` -> advisories/bans/licenses/sources ok; repository security and dependency markers passed, 2026-08-12).
- [x] M4 - Documentation and local gate reconciliation (`preflight: ok`, `execplan state: ok`, and `git diff --check` passed, 2026-08-12).
- [x] M5 - Full local acceptance and outcomes (`bash scripts/verify.sh` exited 0 in 1,278.6 seconds with the terminal `verify: ok` path, using `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0` and disposable loopback services, 2026-08-12).

## 13. Surprises & Discoveries

- 2026-08-12: `cargo update -p event-listener --precise 5.4.2` updated the
  lockfile from 5.4.1 to 5.4.2 and removed its `concurrent-queue` dependency;
  the workspace manifests were not changed. The complete offline reverse tree
  then resolved only `event-listener v5.4.2` through vendored SQLx 0.8.6.
- 2026-08-12: the first offline reverse-tree attempt failed because 5.4.2
  was not cached. `cargo fetch --locked` completed, after which the same
  offline graph check passed. This was a local cache recovery, not a resolver
  or application failure.
- 2026-08-12: invoking the gate as `rtk bash scripts/security-check.sh`
  hid the installed `cargo-audit` from PATH. The exact scripts passed through
  the documented Git Bash launcher; no gate was weakened and no warning was
  suppressed.
- 2026-08-12: the first full verifier reached typecheck and then failed at
  the Windows linker with `LNK1201` while writing `target/debug/deps/hydra_kernel.pdb`
  after the host reached 99% disk usage. A narrow reduced-debug kernel link
  passed, generated Cargo debug artifacts were removed with `cargo clean
  --profile dev`, and the unchanged verifier then passed with incremental
  compilation disabled and debug info reduced. The source, Git metadata, and
  tracked files were not cleaned or reverted.

## 14. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-12 | Use a precise compatible lockfile update first | `event-listener` is transitive through the vendored SQLx stack and the patched release is semver-compatible; this is smaller and safer than adding a forcing dependency or replacing SQLx. |
| 2026-08-12 | Do not ignore RUSTSEC-2026-0221 | The advisory is an unsound thread-safety issue; production hardening must remove it from the graph rather than classify it away. |
| 2026-08-12 | Preserve the existing SQLx boundary and resolve only the lockfile entry | `event-listener 5.4.2` is compatible with the vendored SQLx 0.8.6 graph, so no application or checked-query change is justified. |

## 15. Outcomes & Retrospective

EP-045 is complete. The resolved dependency graph is free of
`RUSTSEC-2026-0221`, while the two unrelated unmaintained warnings remain
visible and documented. The vendored SQLx boundary, checked metadata, and
runtime contracts were unchanged. Preflight, state/diff, security,
dependency, and the unchanged full verifier passed using disposable loopback
services. EP-010 remains partial because this dependency remediation does not
provide staging, recovery, operational, or human production evidence. No
production deployment, push, tag, or production database operation occurred.
