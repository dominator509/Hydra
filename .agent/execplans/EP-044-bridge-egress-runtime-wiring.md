# EP-044 Bridge Egress Runtime Wiring

Plan status: COMPLETE

## 1. Purpose / Big Picture

Connect the already validated explicit Hydra egress proxy configuration to the
real configured Wasmtime bridge lifecycle. EP-022 added
`ReqwestEgressClient::new_with_proxy`, but `crates/kernel/src/runtime_services.rs`
still constructs `DenyEgressClient` unconditionally. That makes a configured
adapter appear available while all granted HTTP calls fail closed. This plan
corrects that runtime defect without changing the WIT ABI, grants, Store
authority, or external network topology.

## 2. Scope

- Construct `bridge_host::ReqwestEgressClient` from the typed proxy setting in
  the Kernel lifecycle builder.
- Preserve direct development/test behavior when no proxy is configured.
- Fail closed if client construction cannot complete.
- Add a focused regression test and strengthen the static egress policy gate.
- Reconcile the EP-022 reality record and operational documentation.

## 3. Non-goals

- No new adapter ABI, WIT change, adapter code generation, or bridge provider.
- No new dependency, migration, SQL, destination allowlist redesign, or
  ambient proxy discovery.
- No change to BridgeHost grants, fuel limits, named secret access, tenant KV,
  lifecycle state, synchronization semantics, or Governor behavior.
- No staging or production deployment, external provider call, tag, push, or
  production database operation.
- No claim that staging proxy ACLs, DNS/TLS, or live adapter connectivity were
  exercised.

## 4. Context and Orientation

`EP-022` defined `HYDRA_EGRESS_PROXY_URL` and the proxy-aware
`bridge_host::ReqwestEgressClient`. The host path already forwards the injected
`EgressClient` through `BridgeLifecycle::host_state` to the WIT `http` host
function. The remaining mismatch is the Kernel construction site:
`build_bridge_lifecycle` passes `DenyEgressClient` even when an adapter root,
secret source, and proxy configuration are present.

The smallest safe correction is to construct `ReqwestEgressClient` at that
site with the already validated `Option<&str>`. Invalid construction is
reported as unavailable and therefore prevents lifecycle handlers from being
registered. The deny client remains available to explicit test helpers and
standalone paths that do not configure adapters.

## 5. Files to Read First

`AGENTS.md`; `COMMANDS.md`; `.agent/PLANS.md`;
`.agent/EXECUTION_RULES.md`; `.agent/state/execplan-index.md`;
`ARCHITECTURE.md`; `SECURITY.md`; `DEPLOYMENT.md`;
`PRODUCTION_READINESS.md`; `DECISIONS.md`;
`.agent/specs/SPEC-024-bridge-egress-runtime.md`;
`.agent/execplans/EP-022-egress-boundary-and-runtime-network-hardening.md`;
`crates/kernel/src/runtime_services.rs`; `crates/kernel/tests/runtime_wiring.rs`;
`crates/bridge-host/src/host.rs`; `crates/bridge-host/src/lifecycle.rs`;
`scripts/check-egress-policy.sh`; `scripts/preflight.sh`; `scripts/verify.sh`.

## 6. Files to Change

- `.agent/specs/SPEC-024-bridge-egress-runtime.md`
- `.agent/execplans/EP-044-bridge-egress-runtime-wiring.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `scripts/check-egress-policy.sh`
- `crates/kernel/src/runtime_services.rs`
- `crates/kernel/tests/runtime_wiring.rs`
- `.agent/execplans/EP-022-egress-boundary-and-runtime-network-hardening.md`
- `ARCHITECTURE.md`
- `SECURITY.md`
- `DEPLOYMENT.md`
- `PRODUCTION_READINESS.md`
- `NEXUS_INTEGRATION_AUDIT.md`
- `DECISIONS.md`

## 7. Interfaces and Contracts

- `build_bridge_lifecycle` uses
  `bridge_host::ReqwestEgressClient::new_with_proxy(config.egress_proxy_url.as_deref())`.
- A successful client construction injects the client into
  `BridgeLifecycle::new`; lifecycle handlers are then registered normally.
- A construction error returns `RuntimeAvailability::Unavailable` and no
  lifecycle runtime or handlers.
- `None` remains a supported development/test value and uses the existing
  direct compatibility constructor.
- `DenyEgressClient` may remain in `BridgeLifecycle::disabled_for_tests` and
  other explicit test-only helpers, but not in the real Kernel lifecycle path.
- The static checker must detect the real runtime constructor call and reject
  a regression to an unconditional deny client.

## 8. Milestones

### M1 - Activate plan and validate state

Add EP-044 and SPEC-024, make EP-044 the only active index row, extend the
state checker to validate the new plan, and record the EP-043 to EP-044
transition.

Validation: `bash scripts/check-execplan-state.sh`

Expected result: `execplan state: ok` with EP-044 as the only `ACTIVE` row.

Recovery: repair the index, plan status, or checker loop before Rust edits.

### M2 - Wire the proxy-aware bridge client

Replace the unconditional deny client in `build_bridge_lifecycle` with the
existing proxy-aware client. Map construction failure to unavailable runtime
state without exposing proxy values. Preserve the disabled test helper.

Validation: `cargo check -p hydra-kernel --tests --offline`

Expected result: zero compilation errors.

Recovery: run the narrowed Kernel check and inspect the exact trait-object or
error mapping issue; do not bypass the `EgressClient` abstraction.

### M3 - Add executable runtime regression coverage

Add focused coverage for the configured bridge lifecycle builder and malformed
proxy construction. The test must assert that configured lifecycle construction
is available through the real Kernel path and that invalid proxy configuration
does not register a usable lifecycle. Keep any network behavior local and
deterministic; do not call an external destination.

Validation: `cargo test -p hydra-kernel --test bridge_lifecycle -- --nocapture`
and `cargo test -p hydra-kernel --test runtime_wiring -- --nocapture`

Expected result: both suites pass, including the new proxy-wiring assertions.

Recovery: isolate the single new test, then use the existing loopback fixture
path. Never replace a zero-test or unavailable-service result with a pass.

### M4 - Strengthen the policy gate and reconcile documentation

Require the static egress checker to see the real Kernel constructor call and
the absence of the unconditional deny client in that production path. Update
the EP-022 reality record, architecture/security/deployment/readiness/audit
documentation, and ADR-0055.

Validation: `bash scripts/check-egress-policy.sh` and
`bash scripts/preflight.sh`

Expected result: `egress policy: ok` and `preflight: ok`.

Recovery: fix the owning contract or checker assertion; do not make the gate
warning-only.

### M5 - Full local acceptance and close the plan

Run security, dependency, focused, state, diff, and full repository gates
using the existing isolated loopback services. Update Progress, Decision Log,
Outcomes, and the state index only after the complete verifier passes.

Validation: `bash scripts/verify.sh`

Expected result: terminal `verify: ok`; no production action occurred.

Recovery: follow AGENTS.md section 7 and preserve this plan ACTIVE until the
required gates are genuinely green.

## 9. Concrete Steps

1. Activate EP-044 and run the state checker.
2. Patch the real Kernel lifecycle builder to construct the explicit egress
   client and fail closed on construction error.
3. Add focused runtime coverage using existing isolated test infrastructure.
4. Extend the egress policy gate and update all affected documentation.
5. Run preflight, focused suites, security/dependency checks, full verifier,
   diff review, and state reconciliation.

## 10. Validation and Acceptance

- EP-044 is the only active plan during implementation.
- The configured Kernel bridge lifecycle injects
  `ReqwestEgressClient::new_with_proxy`.
- The real path does not unconditionally inject `DenyEgressClient`.
- Invalid construction fails closed and does not register lifecycle handlers.
- Development/test compatibility remains intact when no proxy is configured.
- Focused bridge/runtime tests, egress policy, preflight, state, security,
  dependency, and full `verify.sh` all pass with their required markers.
- `git diff --check` passes and changed files stay within this section.
- EP-010 remains partial; no staging or production evidence is fabricated.

## 11. Idempotence and Recovery

The change is additive and has no migration or persistent data operation.
Re-running the state checker, policy checker, focused tests, and full verifier
is safe. If interrupted, inspect the plan Progress and index, rerun the first
unchecked milestone, and preserve unrelated worktree changes. If Docker or an
external network is unavailable, use the documented isolated local services
and record the external validation as unexecuted.

## 12. Progress

- [x] M1 - EP-044 activated and state validated (`execplan state: ok`, 2026-08-12).
- [x] M2 - Proxy-aware bridge client wired into the real Kernel lifecycle (`cargo check -p hydra-kernel --tests --offline`, zero errors, 2026-08-12).
- [x] M3 - Runtime regression coverage passes (bridge lifecycle `2 passed`; runtime wiring `7 passed` against loopback Postgres, 2026-08-12).
- [x] M4 - Egress policy gate and documentation reconciled (`egress policy: ok`, `preflight: ok`, 2026-08-12).
- [x] M5 - Full local acceptance and outcomes recorded (`bash scripts/verify.sh` exited 0 in 664.3s; verifier contract ends with `verify: ok`, 2026-08-12).

## 13. Surprises & Discoveries

2026-08-12: The first malformed-proxy fixture used `not-a-proxy-uri`, which
`reqwest` accepted as a relative proxy value. The focused test initially
failed because the fixture did not exercise construction failure. Replaced it
with the existing malformed authority form `http://user:secret@[invalid`,
which the BridgeHost constructor already proves is rejected without echoing
credentials.

## 14. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-12 | Use the existing `ReqwestEgressClient` rather than adding a new adapter HTTP abstraction | EP-022 already defined and tested the proxy-aware BridgeHost seam; the defect is only the Kernel construction site. |
| 2026-08-12 | Keep `DenyEgressClient` for explicit disabled/test helpers | Standalone and deterministic tests must retain a safe no-network default, while configured lifecycle must not advertise a permanently denied egress path. |
| 2026-08-12 | Treat the initial missing `DATABASE_URL` run as unexecuted, then use `127.0.0.1:55433` | The Kernel bridge tests require Store migrations; the repository's disposable loopback Postgres listener provided the required authority without using a production database. |
| 2026-08-12 | Close EP-044 after the full verifier, not only focused suites | The runtime change crosses Kernel/BridgeHost construction and policy gates; the repository-wide verifier is required to catch lint, format, integration, E2E, security, dependency, smoke, and cache regressions. |

## 15. Outcomes & Retrospective

Completed 2026-08-12. The configured Kernel bridge lifecycle now injects the
existing proxy-aware `ReqwestEgressClient` using the validated explicit egress
setting. Invalid construction returns an unavailable runtime and registers no
lifecycle handlers; the deny-only client remains limited to explicit disabled
or test helpers. Compile, focused bridge/runtime suites, policy, preflight,
state, diff, and full repository verification passed. EP-010 remains partial:
staging proxy ACL/DNS/TLS, upstream connectivity, recovery, soak, reviews,
and human sign-off were not performed. No production deployment, tag, push,
or production database operation occurred.
