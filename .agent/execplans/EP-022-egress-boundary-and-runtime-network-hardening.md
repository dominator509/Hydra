# EP-022 Egress Boundary and Runtime Network Hardening

Plan status: COMPLETE

## 1. Purpose / Big Picture

Close the remaining code-owned outbound-network boundary gap identified in `ARCHITECTURE.md` and `PRODUCTION_READINESS.md`. Hydra's Compose topology provides Tinyproxy as the only application-side external network path, but production Rust HTTP clients currently rely on ambient environment proxy behavior and the OIDC JWKS client constructs a direct client without an explicit proxy contract. This plan makes the proxy URL an explicit Hydra configuration value, requires it in staging and production, threads it through LLM providers and OIDC JWKS retrieval, and exposes safe proxy-aware constructors for the existing Fabric and BridgeHost egress seams.

The change preserves Hydra's six-layer law: no new cross-layer dependency, no SQL, no adapter ABI change, no LLM/Governor change, and no direct Nexus or vendor authority path. Local development and deterministic tests may continue to omit the setting; staging and production fail closed before startup when the required proxy is missing or invalid.

## 2. Scope

- Add `HYDRA_EGRESS_PROXY_URL` to the typed Kernel configuration and deployment examples.
- Require a valid absolute HTTP(S) proxy URL when `HYDRA_ENV=staging` or `HYDRA_ENV=prod`.
- Construct explicit proxy-aware `reqwest` clients for configured LLM providers and OIDC JWKS retrieval.
- Add proxy-aware constructors to Fabric's egress abstraction and BridgeHost's `ReqwestEgressClient` without changing existing test-friendly constructors.
- Preserve proxy URL secrecy: configuration errors and logs must not include credentials or full secret-bearing URLs.
- Add focused tests for configuration fail-closed behavior, invalid proxy rejection, OIDC client construction, and provider client construction.
- Add a static repository policy checker and wire it into preflight and the full verifier.
- Reconcile architecture, security, environment, deployment, package, readiness, audit, command, and decision documentation.

## 3. Non-goals

- No CRM/CDM, Governor, ActionEnvelope, event, migration, or SQL changes.
- No new network dependency, Node/npm toolchain, proxy implementation, destination allowlist redesign, or production deployment.
- No automatic proxy discovery in staging/production beyond the explicit Hydra variable.
- No claim that external DNS/TLS, Tinyproxy ACLs, IdP reachability, or staging network policy has been exercised.
- No removal of existing `new()` constructors used by deterministic tests or standalone development.
- No routing of database, NATS, ingress, or local health traffic through the external proxy.

## 4. Context and Orientation

`docker/compose.yaml` places the Kernel on `proxy-internal` and gives the egress proxy the only `egress-external` network. It currently sets generic `HTTP_PROXY` and `HTTPS_PROXY`, but application behavior is not validated by Hydra configuration. `crates/llm-router/src/lib.rs` uses `reqwest::Client::new()` for all model providers, `crates/fabric/src/auth/oidc.rs` builds a direct JWKS client, and the public egress seams in `crates/fabric/src/egress.rs` and `crates/bridge-host/src/host.rs` do not expose an explicit proxy constructor. The intended policy is already recorded in `ARCHITECTURE.md`: external integration egress must pass through the proxy boundary and adapters retain their grant boundary.

The safe compatibility rule is: dev/test may use the current ambient-client constructors; the Kernel's staging/prod path must pass the validated explicit proxy URL to every configured external HTTP client. A malformed proxy must be a configuration error, not a runtime fallback.

## 5. Files to Read First

`AGENTS.md`; `COMMANDS.md`; `.agent/PLANS.md`; `.agent/EXECUTION_RULES.md`; `.agent/state/execplan-index.md`; `ARCHITECTURE.md`; `SECURITY.md`; `ENVIRONMENT.md`; `DEPLOYMENT.md`; `PRODUCTION_READINESS.md`; `NEXUS_INTEGRATION_AUDIT.md`; `DECISIONS.md`; `docker/compose.yaml`; `docker/egress-proxy.conf`; `.env.example`; `docker/nexus.env.example`; `crates/kernel/src/config.rs`; `crates/kernel/src/main.rs`; `crates/kernel/src/runtime_services.rs`; `crates/llm-router/src/lib.rs`; `crates/llm-router/src/providers/anthropic.rs`; `crates/llm-router/src/providers/deepseek.rs`; `crates/llm-router/src/providers/nexus.rs`; `crates/llm-router/src/providers/openai_compat.rs`; `crates/fabric/src/auth/oidc.rs`; `crates/fabric/src/egress.rs`; `crates/bridge-host/src/host.rs`; `scripts/preflight.sh`; `scripts/verify.sh`.

## 6. Files to Change

- `.agent/execplans/EP-022-egress-boundary-and-runtime-network-hardening.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `scripts/check-egress-policy.sh`
- `scripts/preflight.sh`
- `scripts/verify.sh`
- `crates/kernel/src/config.rs`
- `crates/kernel/src/main.rs`
- `crates/kernel/src/runtime_services.rs`
- `crates/llm-router/src/lib.rs`
- `crates/llm-router/src/providers/anthropic.rs`
- `crates/llm-router/src/providers/deepseek.rs`
- `crates/llm-router/src/providers/nexus.rs`
- `crates/llm-router/src/providers/openai_compat.rs`
- `crates/fabric/src/auth/oidc.rs`
- `crates/fabric/src/egress.rs`
- `crates/bridge-host/src/host.rs`
- `crates/fabric/tests/nexus_auth.rs`
- `crates/kernel/tests/support/fake_nexus.rs`
- `.env.example`
- `docker/compose.yaml`
- `docker/nexus.env.example`
- `COMMANDS.md`
- `ENVIRONMENT.md`
- `ARCHITECTURE.md`
- `SECURITY.md`
- `DEPLOYMENT.md`
- `PRODUCTION_READINESS.md`
- `NEXUS_INTEGRATION_AUDIT.md`
- `DECISIONS.md`

## 7. Interfaces and Contracts

- `HYDRA_EGRESS_PROXY_URL` is optional in `dev` and required in `staging`/`prod`.
- The value must parse as an absolute `http://` or `https://` URI; invalid values fail configuration without echoing credentials or the full URL.
- `JsonHttpClient::new_with_proxy`, `DirectProxy::new_with_proxy`, and `ReqwestEgressClient::new_with_proxy` return construction errors for malformed proxy URLs. Existing `new()` constructors remain available for tests/dev compatibility.
- Kernel provider construction uses the validated proxy URL for DeepSeek, Anthropic, OpenAI-compatible, and optional Nexus model gateway providers.
- `OidcAuthenticator` uses the same explicit proxy for remote JWKS retrieval. Pinned-key mode has no external fetch but accepts the same config shape.
- No configured staging/prod external provider or JWKS client may silently fall back to an unproxied client.
- `bash scripts/check-egress-policy.sh` prints `egress policy: ok` only when config, Compose, client builders, and required staging/prod validation are present.
- Existing tests and standalone dev mode remain runnable without a proxy service.

## 8. Milestones

### M1 - Activate EP-022 and record the boundary

Add EP-022 to the authoritative index, extend the plan-state checker, record the EP-021-to-EP-022 transition, and document the explicit proxy/fail-closed boundary in the plan.

Validation: `bash scripts/check-execplan-state.sh`

Expected result: `execplan state: ok` with EP-022 as the only `ACTIVE` row.

Recovery: repair the index, plan status, or checker loop with `apply_patch`; do not begin Rust edits while the checker reports a state mismatch.

### M2 - Add typed proxy configuration and fail-closed validation

Add `HYDRA_EGRESS_PROXY_URL` to `Config`, parse and validate it, require it for staging/prod, and pass it from Kernel startup to the runtime and OIDC configuration. Update examples and environment documentation.

Validation: `cargo test -p hydra-kernel --bin hydra-kernel config::tests:: -- --nocapture`

Expected result: configuration tests pass, including dev omission, staging/prod omission failure, valid URL acceptance, and malformed URL rejection without secret echo.

Recovery: use the named proxy test filter on the Kernel binary and inspect the exact validation error; keep development defaults compatible and never weaken production fail-closed behavior.

### M3 - Thread explicit proxy clients through outbound paths

Add proxy-aware client constructors in `llm-router`, Fabric OIDC/egress, and BridgeHost; use the proxy-aware constructors from the real Kernel runtime provider/authentication paths. Keep `new()` only as a compatibility constructor for test/dev paths.

Validation: `cargo test -p llm-router --lib -- --nocapture` and `cargo test -p fabric --lib -- --nocapture`

Expected result: provider/client construction tests pass and invalid proxy URLs fail before any request is attempted.

Recovery: run the narrow crate test, then inspect each `reqwest::Client` production construction site; do not patch only one provider.

### M4 - Add static policy gate and documentation

Add `scripts/check-egress-policy.sh`, wire it into preflight and `verify.sh`, update command/env/deployment/security/architecture/readiness/audit documentation, and record ADR-0032. The static check must prove that the explicit variable, staging/prod validation, Compose wiring, OIDC proxy construction, and provider proxy construction are present.

Validation: `bash scripts/check-egress-policy.sh`, then `bash scripts/preflight.sh`

Expected result: `egress policy: ok` and `preflight: ok`.

Recovery: fix the owning contract or script assertion; do not make the checker warning-only or permit an unproxied production fallback.

### M5 - Full local acceptance and truthful reconciliation

Run the focused gates, shell syntax checks, diff review, security/dependency checks as part of the full verifier against the existing isolated services. Update EP-022 Progress, Decision Log, Outcomes, and the index only after all validations pass.

Validation: `bash scripts/verify.sh`

Expected result: terminal `verify: ok`; no production deployment, tag, push, external provider call, or production database operation.

Recovery: follow AGENTS.md section 7 and reuse the isolated Postgres/NATS path. If a required external-network validation is unavailable, record it as unexecuted rather than simulating it.

## 9. Concrete Steps

1. Activate EP-022 and validate the index before implementation.
2. Add the typed proxy setting and unit coverage in Kernel config.
3. Thread the validated setting into runtime providers and OIDC construction.
4. Add explicit proxy builders to shared outbound seams and preserve dev/test constructors.
5. Add the static policy checker and mandatory gate wiring.
6. Update all environment, deployment, architecture, security, readiness, audit, command, and decision records.
7. Run milestones in order, then the full verifier and changed-file review.

## 10. Validation and Acceptance

- EP-022 is the only active plan during implementation and its state/index agree.
- Staging/prod configuration rejects a missing or malformed proxy before runtime startup.
- Dev/test configuration remains compatible when the proxy is omitted.
- Kernel provider construction and OIDC JWKS retrieval use the explicit validated proxy when configured.
- Fabric and BridgeHost proxy-aware constructors reject malformed values and do not log secrets.
- `bash scripts/check-egress-policy.sh` prints `egress policy: ok`.
- `bash scripts/preflight.sh` prints `preflight: ok`.
- `bash scripts/verify.sh` exits 0 through `verify: ok` with no masked required failures.
- Documentation distinguishes local client-construction evidence from real staging proxy/TLS/ACL evidence.
- EP-010 remains partial; no staging or production evidence is fabricated.

## 11. Idempotence and Recovery

The plan-state and static policy checks are read-only. Configuration and client changes are additive and deterministic. Re-running tests does not contact an external provider when fixtures are used. If interrupted, inspect Progress, rerun the first unchecked milestone, and preserve unrelated worktree changes. No migration or production data operation is introduced.

## 12. Progress

- [x] M1 - EP-022 active, state checker/index, and boundary recorded.
- [x] M2 - Typed proxy configuration and fail-closed validation implemented.
- [x] M3 - Explicit proxy clients threaded through outbound paths.
- [x] M4 - Static policy gate and documentation updated.
- [x] M5 - Full local acceptance, diff review, and outcomes recorded (`bash scripts/verify.sh` -> terminal `verify: ok` with explicit isolated Postgres/NATS endpoints on 2026-08-11).

## 13. Surprises & Discoveries

- 2026-08-11: The broad Cargo filter `cargo test -p hydra-kernel config -- --nocapture` also selected the `bridge_lifecycle` integration test, which requires an isolated database. It is not counted as a focused M2 result; the final validation targets the Kernel binary's `config::tests::` module without selecting database-backed integration tests.
- 2026-08-11: The first narrowed command `cargo test -p hydra-kernel --lib config -- --nocapture` exited zero but selected zero tests because the config tests live in the binary target and the test names do not contain `config`. The second `--lib proxy` command also selected zero library tests. Neither is counted as evidence; the M2 selector now targets `--bin hydra-kernel config::tests::`, which matches all four proxy configuration tests without selecting database-backed integration tests.
- 2026-08-11: The first full verifier attempt reached Clippy and rejected the new Fabric test module because it preceded the production `impl Proxy` under `clippy::items_after_test_module`. Moving the test module to the file end fixed the issue; the focused Fabric suite then passed 75 tests.
- 2026-08-11: The first isolated integration retry used `nats://127.0.0.1:4222` and exposed two JetStream connection refusals; the configured listener is IPv6 loopback. Re-running the complete integration suite with `NATS_URL=nats://[::1]:4222` passed all six event-bridge tests and both required integration success markers. The failed attempt is not counted as green evidence.
- 2026-08-11: The final verifier used `HYDRA_TEST_DATABASE_URL` and `DATABASE_URL` at the isolated Postgres listener `127.0.0.1:55433` plus `NATS_URL=nats://[::1]:4222`. It exited zero after 465.9 seconds and emitted `preflight: ok`, `egress policy: ok`, `integration tests: ok`, `failure suites: ok`, `e2e tests: ok`, `security check: ok`, `dependency audit: ok`, and terminal `verify: ok`.

## 14. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-11 | Make `HYDRA_EGRESS_PROXY_URL` explicit and mandatory in staging/prod | Ambient `HTTP_PROXY`/`HTTPS_PROXY` environment behavior is not a sufficient application contract for a security boundary; production must fail closed when the proxy is absent or malformed. |
| 2026-08-11 | Preserve existing unconfigured constructors for dev/test only | Deterministic local fixtures and standalone development do not require an external proxy, while the real Kernel staging/prod path receives the explicit validated setting. |
| 2026-08-11 | Harden OIDC JWKS retrieval as part of the same seam | JWKS fetches are external egress and must not bypass the same network boundary used by model providers. |
| 2026-08-11 | Use the corrected binary-target configuration-module command as the M2 validation | The broader Cargo filter selected a database-backed integration test, while library-target filters selected zero tests because configuration tests are compiled with `main.rs`; the binary module filter targets all four tests without suppression. |
| 2026-08-11 | Do not count a zero-test Cargo success | A green command with `0 passed` is insufficient evidence; the M2 filter now matches the four named proxy tests directly. |
| 2026-08-11 | Keep proxy construction errors static and secret-free | Proxy URLs can contain credentials; no invalid value or reqwest error is returned to logs or callers. |
| 2026-08-11 | Validate every explicit outbound seam with focused unit suites | `llm-router`, Fabric OIDC/egress, and BridgeHost each have deterministic constructor coverage; no external provider or IdP call is required. |
| 2026-08-11 | Make egress policy a required repository gate | A static checker proves the typed setting, fail-closed validation, Compose wiring, all four provider constructors, OIDC proxy construction, and mandatory preflight/verifier wiring remain present together. |
| 2026-08-11 | Keep new test modules after production items | Hydra denies `clippy::items_after_test_module`; test placement must preserve the crate-wide lint contract. |
| 2026-08-11 | Count only explicitly isolated database and JetStream endpoints as final acceptance evidence | The integration wrapper enforces loopback Postgres, while this worktree's JetStream listener is IPv6 loopback; explicit endpoints prevent accidental reliance on another local service and make the successful verifier reproducible. |

## 15. Outcomes & Retrospective

Complete. EP-022 now makes outbound HTTP routing an explicit typed contract: staging and production fail closed without a valid `HYDRA_EGRESS_PROXY_URL`, configured model providers and OIDC JWKS retrieval use the explicit proxy, and Fabric/BridgeHost expose secret-safe proxy-aware constructors. The required static policy gate is wired into preflight and the full verifier.

M1 through M5 passed. Focused Kernel configuration (4 tests), llm-router (8), Fabric (75), and BridgeHost (10) suites passed; `bash scripts/check-egress-policy.sh` emitted `egress policy: ok`; `bash scripts/preflight.sh` emitted `preflight: ok`; the explicit isolated integration run emitted `integration tests: ok` and `failure suites: ok`; and the full verifier exited zero after 465.9 seconds through terminal `verify: ok`. No external provider, staging network, real IdP, production database, tag, push, or deployment was used.

EP-010 remains partial. Real staging proxy reachability, DNS/TLS, Tinyproxy ACL behavior, IdP/provider connectivity, recovery drills, soak, reviews, and human sign-off still require operator-controlled evidence.

## Post-EP-044 Reality Check (2026-08-12)

EP-044 corrected a runtime wiring gap that was not covered by EP-022's static
constructor checks: `build_bridge_lifecycle` had still injected
`DenyEgressClient` into every configured lifecycle. The real Kernel path now
constructs `ReqwestEgressClient::new_with_proxy` from the validated typed
configuration and fails closed without registering lifecycle handlers when
construction fails. EP-022's external proxy, ACL, DNS/TLS, and staging
readiness boundary remains unchanged and unexecuted.
