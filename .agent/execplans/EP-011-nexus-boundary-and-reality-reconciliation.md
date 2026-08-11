# EP-011 Nexus Boundary and Reality Reconciliation

Plan status: COMPLETE

## 1. Purpose / Big Picture
Make Hydra's plans, specifications, architecture, and verified implementation agree before changing the external trust boundary. Establish Hydra as the independently deployable CRM/revenue bounded context beneath Nexus, preserve historical evidence, and leave one machine-verifiable active-plan state.

## 2. Scope
Audit current runtime/schema/routes/auth/events/tests/deployment; create SPEC-010, ADR-0019, integration guidance, and the ExecPlan status index; create EP-012 through EP-016 with only EP-011 active; append current reality sections to affected historical plans; restore the pre-existing clippy baseline without behavior changes; repair any critical dependency vulnerability exposed by the mandatory full gate; validate repository state.

## 3. Non-goals
No authentication, MCP, persistence, execution, event, or deployment behavior changes beyond behavior-neutral validation repair. No new crate; the only dependency change permitted is an evidence-backed security patch-line correction required for a green baseline. No historical checkbox or Decision Log deletion/rewrite. No GraphQL implementation. No push, merge, tag, staging/production deploy, or real production database operation.

## 4. Context and Orientation
The 2026-08-10 baseline at `f38689a` confirmed all 18 user-supplied findings and additional tenant/provenance gaps. `bash scripts/preflight.sh` passed. `bash scripts/verify.sh` failed in lint on an unused test import and denied test-only unwraps, so no downstream gate from that run is counted as passed. `.agent/state/execplan-index.md` becomes current status authority while old plan text remains historical evidence.

## 5. Files to Read First
`AGENTS.md`; `COMMANDS.md`; `.agent/PLANS.md`; `.agent/EXECUTION_RULES.md`; `README.md`; `REPO_BRIEF.md`; `PROJECT_BRIEF.md`; `ARCHITECTURE.md`; `DECISIONS.md`; `SECURITY.md`; `TESTING.md`; `ENVIRONMENT.md`; `DEPLOYMENT.md`; `PRODUCTION_READINESS.md`; `ROADMAP.md`; every EP-000 through EP-010; every accepted SPEC; all source/tests under the crates, migrations, docker, scripts, and workflows named by the master directive.

## 6. Files to Change
`Cargo.toml`; `Cargo.lock`; `deny.toml`; `ARCHITECTURE.md`; `COMMANDS.md`; `DECISIONS.md`; `ROADMAP.md`; `README.md`; `REPO_BRIEF.md`; `NEXUS_INTEGRATION_AUDIT.md` (new); `NEXUS_INTEGRATION.md` (new); `.agent/EXECUTION_RULES.md`; `.agent/specs/SPEC-010-nexus-interoperability.md` (new); `.agent/state/execplan-index.md` (new); `.agent/execplans/EP-011-nexus-boundary-and-reality-reconciliation.md` (new); `.agent/execplans/EP-012-nexus-auth-and-mcp-control-plane.md` (new); `.agent/execplans/EP-013-governed-execution-and-provenance.md` (new); `.agent/execplans/EP-014-nexus-event-bridge-and-tracing.md` (new); `.agent/execplans/EP-015-nexus-e2e-deployment-and-gates.md` (new); `.agent/execplans/EP-016-nexus-model-a2a-and-skills.md` (new); `.agent/execplans/EP-001-foundation.md`; `.agent/execplans/EP-004-api-or-service-layer.md`; `.agent/execplans/EP-005-user-interface-or-client.md`; `.agent/execplans/EP-006-auth-security-and-permissions.md`; `.agent/execplans/EP-007-testing-hardening.md`; `.agent/execplans/EP-008-observability-and-operations.md`; `.agent/execplans/EP-009-deployment-and-release.md`; `.agent/execplans/EP-010-production-readiness.md`; `scripts/check-execplan-state.sh` (new); `scripts/preflight.sh`; `crates/agents/src/comms.rs`; `crates/agents/src/data_steward.rs`; `crates/agents/tests/bridge_engineer_loop.rs`; `crates/bridge-host/src/host.rs`; `crates/bridge-host/tests/store_kv.rs` (new); `crates/fabric/src/wiring.rs`; `crates/fabric/tests/integration_contracts.rs`; `crates/kernel/src/metrics.rs`; `crates/kernel/src/telemetry.rs`; `crates/kernel/tests/smoke_healthz.rs`; `docker/compose.yaml`.

The mandatory workspace formatter also normalized these behavior-neutral files in the same run: `crates/agents/src/bridge_engineer.rs`; `crates/fabric/src/auth/jwt.rs`; `crates/fabric/src/auth/password.rs`; `crates/fabric/src/auth/session.rs`; `crates/fabric/src/lib.rs`; `crates/fabric/src/rate.rs`; `crates/fabric/src/rest/autonomy.rs`; `crates/fabric/src/rest/bridges.rs`; `crates/fabric/src/rest/oauth.rs`; `crates/fabric/src/services.rs`; `crates/fabric/tests/authz_endpoints.rs`; `crates/fabric/tests/four_eyes.rs`; `crates/fabric/tests/token_scopes.rs`; `crates/kernel/src/main.rs`; `crates/shell/src/routes/approvals.rs`; `crates/shell/src/routes/login.rs`; `crates/shell/src/routes/mod.rs`; `crates/store/src/lib.rs`.

## 7. Interfaces and Contracts
The status index is authoritative current state and MUST contain exactly one `ACTIVE` plan. Historical plan appendices describe present verification without changing old progress/logs. SPEC-010 is normative. `scripts/check-execplan-state.sh` MUST fail on missing required artifacts, wrong section order/count, multiple active plans, or non-deferred EP-016 and print `execplan state: ok` only on success. Clippy repair MUST change test diagnostics only, not production behavior.

## 8. Milestones
M1 Baseline and audit. Goal: capture current branch, implementation, known findings, and command evidence. Files: audit and index. Edits: evidence-backed topology, services, routes, tools, auth, execution, events, test gates, deployment, risk map, and status rows. Validation: `bash scripts/preflight.sh`. Expected: `preflight: ok`. Recovery: if preflight fails, use its named missing tool/file and the smallest documented recovery; do not infer later gate status.

M2 Boundary contract. Goal: make ownership and protocol requirements normative. Files: architecture, ADR, integration docs, SPEC-010, roadmap. Edits: bounded contexts, dual gate, binding, no-direct-access, GraphQL truth, full security/version/failure/deployment/test contract. Validation: `bash scripts/check-execplan-state.sh` after M3 creates the checker. Expected: `execplan state: ok`. Recovery: missing contract heading/artifact is fixed in the owning document, not waived.

M3 Plan-state mechanism and plan queue. Goal: exactly one active plan and complete queued plans. Files: execution rules, index, COMMANDS, checker, EP-011 through EP-016. Edits: all 15 required sections in order, exact files/commands/recovery, status markers, EP-016 deferred. Validation: `bash scripts/check-execplan-state.sh`. Expected: `execplan state: ok`. Recovery: checker names the missing/duplicate state; repair the index/plan atomically.

M4 Historical reality appendices and discoverability. Goal: preserve old history while recording current truth. Files: affected historical plans, README, REPO_BRIEF. Edits: append `Post-Implementation Reality Check (2026-08-10)` sections only; add links to audit/spec/index/integration docs. Validation: `bash scripts/preflight.sh`. Expected: `preflight: ok`. Recovery: if an old section changed rather than appended, restore its exact prior text from `git diff` and reapply only the appendix.

M5 Baseline validation repair and final review. Goal: behavior-neutral removal of current denied test lints, correction of invalid Compose profile syntax, and remediation of any critical advisory exposed by the full dependency gate, then full validation. Files: the validation-repair files listed in section 6 plus this plan/index. Validation: `cargo test -p bridge-host --test conformance`, `cargo audit`, `cargo deny check`, and `bash scripts/verify.sh`. Expected: conformance passes; audit and deny are green; `verify: ok`. Recovery: first failure gets smallest targeted fix; second same-root failure uses the relevant single-crate/test command; third records the hypothesis and simpler path per AGENTS section 7. Do not ignore an advisory, mask, or delete a test.

## 9. Concrete Steps
Execute M1 through M5 in order. Immediately record each observed command result in Progress/Surprises/Decision Log. Before completion, compare `git diff --name-only` to section 6 and justify any additional file. Change the index to EP-011 `COMPLETE` and EP-012 `ACTIVE` only after M5 passes.

## 10. Validation and Acceptance
The audit cites actual files/symbols; all EP-000 through EP-010 rows are truthful; affected historical records have additive reality appendices; SPEC-010 is self-contained; architecture clearly separates ownership and GraphQL status; exactly one plan is active; EP-016 is deferred; `bash scripts/check-execplan-state.sh` prints `execplan state: ok`; `bash scripts/verify.sh` prints `verify: ok`; no runtime feature or production action occurred.

## 11. Idempotence and Recovery
Docs and status rows are deterministic replacements or append-once dated sections. The checker is read-only and rerunnable. Lint edits replace test `.unwrap()` with descriptive `.expect()` and remove one unused import, so reruns are no-ops. Never rewrite historical commits or delete evidence. If interrupted, inspect index Progress and `git diff --name-only`, then resume the first unchecked milestone.

## 12. Progress
- [x] M1 - Required reads, preflight, implementation audit, and baseline failure evidence captured
- [x] M2 - Bounded-context docs, SPEC-010, ADR-0019, and roadmap strategy created
- [x] M3 - ExecPlan queue and state checker complete (`execplan state: ok`)
- [x] M4 - Historical reality appendices and discoverability complete (`preflight: ok`; `git diff --check` clean)
- [x] M5 - Baseline lint/security repaired; bridge conformance, audit, deny, and full verification green

## 13. Surprises & Discoveries
- 2026-08-10: The worktree was clean and synchronized at inspection start despite earlier reports that Hydra was not pushing. This directive forbids push, so no remote write was attempted.
- 2026-08-10: `verify.sh` was already red before edits due to denied lints in tests; vendor SQLx warnings were noisy but not the failing root.
- 2026-08-10: Several security tests explicitly preserve development behavior, including unauthenticated entity creation and tenant-mismatch authorization.
- 2026-08-10: Documented local dependency startup could not parse `docker/compose.yaml` because the migration service used unsupported singular `profile` instead of `profiles`; no container was started by that failed command.
- 2026-08-10: The integration contract expected legacy actor `dev-admin`, while the active `AuthCtx` path consistently maps the documented dev token to `user:admin`; the runtime audit record was correct and the assertion was stale.
- 2026-08-10: The same integration contract attempted envelope approval without authentication even though the active route requires `Approver`; adding the documented dev-admin bearer token allowed the test to exercise the intended authorization path.
- 2026-08-10: The kernel smoke harness discarded child stderr, reducing every startup failure to an opaque exit code and preventing bounded diagnosis.
- 2026-08-10: The documented Compose services were not usable for host-driven integration: the retained Postgres volume no longer matched the documented password, and the internal-only NATS network made its published host port unreachable. Ephemeral test-only containers on `localhost:55432` and `localhost:54222` provided isolated validation without altering the retained volume.
- 2026-08-10: The first full dependency audit found 18 Wasmtime 38.0.4 vulnerabilities, including two critical sandbox-escape advisories. Pinning the current 36.0.13 LTS removed every vulnerability and preserved all eight active bridge conformance tests.
- 2026-08-10: The machine's older `cargo-deny` 0.17.0 could not parse current CVSS v4 advisory data. Updating the tool to the repository-pinned 0.19.9 restored the intended gate without changing workspace dependencies.
- 2026-08-10: Wasmtime 36.0.13 retains transitive unmaintained `fxhash` through profiling, while `webpki-roots` uses the Mozilla root-data `CDLA-Permissive-2.0` license. Neither is an exploitable advisory; both require explicit, narrow policy classification rather than hidden ignores.
- 2026-08-10: The first post-repair `verify.sh` attempt reached `cargo deny check` but exceeded its 20-minute outer timeout after a cold dependency rebuild. Focused `cargo deny`, smoke, and TOKENKILLER checks all passed; the unchanged warm full run then completed in 487 seconds.
- 2026-08-10: Current rustfmt normalized several pre-existing Rust files while formatting the targeted lint repairs. Their shared write timestamp and representative diffs confirm import ordering and line wrapping only; they are listed explicitly in section 6 rather than omitted from scope.

## 14. Decision Log
| Date | Decision | Rationale |
|---|---|---|
| 2026-08-10 | Use `.agent/state/execplan-index.md` as current status authority | Preserves historical plan evidence while enforcing one active plan |
| 2026-08-10 | Keep GraphQL deferred and outside Nexus v1 | No implementation exists and Nexus does not require it |
| 2026-08-10 | Repair only behavior-neutral pre-existing test lints in EP-011 | A truthful green baseline is required before trust-boundary changes |
| 2026-08-10 | Make Nexus translation Fabric-local and generic below Fabric | Prevents Nexus-specific concepts from contaminating L1/L2 |
| 2026-08-10 | Append dated reality checks instead of changing historical progress | Preserves prior evidence while making current status explicit |
| 2026-08-10 | Make unwired metric mutation hooks test-only until EP-013/EP-014 | Second lint diagnostic proved no production caller; avoids pretending instrumentation exists while preserving metric rendering/tests |
| 2026-08-10 | Move Postgres-backed StoreKvStore assertion from unit to integration target | Unit verification must not require `DATABASE_URL`; durable coverage remains mandatory in `test-integration.sh` |
| 2026-08-10 | Correct the migration service key from `profile` to `profiles` in EP-011 | This is the smallest syntax repair needed to run the documented local validation; network and deployment topology changes remain scoped to EP-015 |
| 2026-08-10 | Update the integration actor assertion to `user:admin` | The active principal extractor and authorization tests establish that value; changing runtime audit identity to match a dead helper would be a behavioral regression |
| 2026-08-10 | Authenticate the integration contract's approval request | Preserves the existing authorization boundary and removes a stale unauthenticated-success assumption from the test |
| 2026-08-10 | Capture kernel child stderr in `smoke_healthz` | Makes the required integration gate machine-diagnostic without changing kernel behavior or masking failure |
| 2026-08-10 | Pin Wasmtime/WASI to 36.0.13 LTS | Smallest supported LTS release that resolves every observed Wasmtime RustSec vulnerability while preserving the existing Component Model ABI |
| 2026-08-10 | Deny direct unmaintained dependencies and classify transitive maintenance warnings | Keeps workspace ownership strict without pretending Hydra can replace an LTS engine's private profiling dependency |
| 2026-08-10 | Permit `CDLA-Permissive-2.0` only for `webpki-roots@1.0.8` | Accepts Mozilla's curated root-certificate data license without broadening Hydra's global license allowlist |
| 2026-08-10 | Retain workspace rustfmt normalization required by the format gate | The changes are behavior-neutral, machine-generated, and fully enumerated; reverting them would restore a known failing baseline |
| 2026-08-10 | Treat the 20-minute full-run timeout as failed evidence and diagnose tail stages separately | Preserves bounded retry discipline; the unchanged warm rerun supplied the required repository-level success result |

## 15. Outcomes & Retrospective
Completed 2026-08-10. All required documentation, SPEC-010, ADR-0019, status-index, historical reality appendices, and queued plans exist. `bash scripts/preflight.sh` printed `preflight: ok`; `bash scripts/check-execplan-state.sh` printed `execplan state: ok`; bridge conformance passed 8 active tests with the nightly soak explicitly ignored; `cargo audit` exited zero with no vulnerabilities; `cargo deny check` printed `advisories ok, bans ok, licenses ok, sources ok`; `bash scripts/dependency-audit.sh` printed `dependency audit: ok`; smoke printed `smoke test: ok`; TOKENKILLER replay printed `cache-hit audit: ok (ratio=0.9717)`; and the unchanged full `bash scripts/verify.sh` completed in 487 seconds with exit zero and `verify: ok`.

Acceptance is met for EP-011. The current implementation gaps in authenticated MCP, tenant binding, governed execution, JetStream acknowledgement, and runtime wiring remain deliberately assigned to EP-012 through EP-014. False-green E2E discovery, masked integration failures, nightly non-gating checks, and deployment topology remain explicitly open for EP-015. Vendor SQLx `unexpected_cfgs` and duplicate dependency versions remain warning-only visibility. EP-010 remains partial and no production deployment, real production database operation, push, merge, or tag occurred. The authoritative index transitioned from EP-011 to EP-012 only after this evidence passed.
