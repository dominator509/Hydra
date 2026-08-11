# ExecPlan Status Index

This file is the authoritative current-state ledger for Hydra ExecPlans. Historical plan checkboxes and Decision Logs remain evidence of what was claimed or observed when each plan ran; they do not override this index's current verification status.

## State rules

- At most one plan may be `ACTIVE`. During EP-011 through EP-015 execution exactly one is active; after EP-015 completes, zero active plans is the truthful terminal state while EP-016 remains deferred.
- `QUEUED` plans may be read for sequencing but may not be implemented.
- `DEFERRED` plans may not be implemented until their stated prerequisite is met.
- `HISTORICAL` preserves prior execution without asserting that every claim still holds.
- `COMPLETE` requires the plan's current acceptance commands and Definition of Done to pass without masked failures.
- Changing the active row requires completing the current plan or recording an AGENTS.md section 4 STOP condition in that plan.

## Current program state

| Plan | Program state | Historical status | Current verified status | Evidence | Remaining work | Superseding plan |
|---|---|---|---|---|---|---|
| EP-000 | HISTORICAL | All discovery milestones checked | Repository/control documents exist; the repo is now an implemented brownfield workspace | `Cargo.toml`, `crates/`, `.agent/execplans/EP-000-repository-discovery.md` | No discovery work; retain as history | None |
| EP-001 | HISTORICAL | All foundation milestones checked | Workspace/scripts remain; EP-015 replaced the false-green E2E stage with two required named scenarios | `scripts/test-e2e.sh` -> 2 passed and `e2e tests: ok` on 2026-08-11 | Foundation has no remaining Nexus-specific work | EP-015 |
| EP-002 | HISTORICAL | All core-domain milestones checked | CDM and deterministic Governor remain intact; EP-013 added backward-compatible invocation context and verified transition provenance | `crates/governor/src/envelope.rs`; EP-013 Store/Governor tests | Canonical event envelope remains | EP-014 |
| EP-003 | HISTORICAL | All persistence milestones checked | Bindings, idempotency, approvals, receipts, tenant-safe transitions, canonical events, trace carriers, leases, and parking are additive and verified | Migrations `0008` through `0013`; EP-013/014 integration evidence | Production retention/backup/restore evidence remains EP-010 | EP-013, EP-014 |
| EP-004 | HISTORICAL | M1-M7 checked | Authenticated MCP, real Executor/provider/BridgeHost/TOKENKILLER wiring, acknowledged relay, and event readiness are locally verified; bridge lifecycle remains unavailable | EP-012 through EP-014 focused/full gates | Persisted bridge lifecycle and production secrets remain unavailable | EP-012 through EP-014 |
| EP-005 | HISTORICAL | M1-M4 checked; M5 open | Shell exists and EP-015 adds real system E2E, but browser accessibility/no-JS evidence remains absent | `scripts/test-e2e.sh` -> 2 required Nexus system scenarios | Accessibility and degradation evidence remains EP-010/UI work | EP-015 |
| EP-006 | HISTORICAL | M1-M5 checked | Asymmetric resource-server auth, binding-derived tenancy, scopes/rate limits, immutable four-eyes approval, and executor assertions pass local E2E | EP-012/013 auth tests; EP-015 fake Nexus round trip | Real IdP/TLS/staging review and persisted vault remain | EP-012, EP-013, EP-015 |
| EP-007 | HISTORICAL | M1-M5 checked | Agent capability truth, unmasked integration failures, required E2E discovery, and gating nightly checks are corrected | Integration/failure/E2E markers and workflow policy scan on 2026-08-11 | Nightly must still run successfully in GitHub | EP-013, EP-015 |
| EP-008 | HISTORICAL | M1-M5 checked | Canonical event readiness/tracing and isolated Compose topology are verified locally; live dashboards/alerts, schedulers, restore, and operational drills remain absent | EP-014 event tests; EP-015 Compose/image evidence | EP-010 staging observability/recovery evidence | EP-014, EP-015 |
| EP-009 | HISTORICAL | M1-M5 checked | Non-root image builds and Compose now isolates ingress/data/events/proxy/egress with only Caddy host-published | `docker build` exit 0; both Compose modes exit 0; normalized network inspection on 2026-08-11 | Staging deploy/rollback, real TLS, SBOM/provenance | EP-015 |
| EP-010 | HISTORICAL | M1 complete; M2-M4 deferred; M5 partial | Correctly remains partial; Nexus auth/execution/events/E2E packaging are locally green, but operations/security/readiness claims were reconciled to current gaps | `PRODUCTION_READINESS.md`; EP-015 independent local gates | D1-D5, soak, real TLS/IdP, vault/retention/recovery, reviews, benchmarks, accessibility, human sign-off | Not superseded; EP-015 does not satisfy production readiness |
| EP-011 | COMPLETE | New | Acceptance passed; repository history, boundary contract, dependency baseline, and plan state are reconciled | `bash scripts/verify.sh` -> `verify: ok`; `cargo audit` -> zero vulnerabilities; `bash scripts/check-execplan-state.sh` -> `execplan state: ok` | None | None |
| EP-012 | COMPLETE | New | Asymmetric OIDC resource-server auth, Hydra-owned bindings, centralized capability authorization/rate limiting, MCP 2025-11-25, and read-only Nexus REST facade passed focused and full gates | `cargo test -p fabric mcp_contract -- --nocapture` -> 4 passed; `bash scripts/verify.sh` -> exit 0 and `verify: ok` terminal path on 2026-08-11 | None | None |
| EP-013 | COMPLETE | New | Durable provenance/idempotency, typed stage execution, mutable persisted policy, supervised runtime wiring, immutable human approval, execution receipts, and truthful agent/bridge capability states are verified | Focused suites passed; `bash scripts/test-integration.sh` -> both success markers without suppression; `bash scripts/verify.sh` -> `verify: ok` on 2026-08-11 | None | None |
| EP-014 | COMPLETE | New | Typed v1 events, transactional audit/outbox, acknowledged JetStream relay, W3C carrier propagation, fail-closed event readiness, and durable fake Nexus consumption are verified | Focused M1-M5 suites passed; real JetStream replay/restart test passed; SQLx/dependency/security gates passed; `bash scripts/verify.sh` exited zero through its `verify: ok` terminal path in 973.5 seconds on 2026-08-11 | None | None |
| EP-015 | COMPLETE | New | Fake Nexus E2E, truthful gates, isolated standalone/Nexus packaging, and the full local acceptance matrix passed | Independent success markers; `docker build` exit 0 in 827.1 seconds; both Compose modes exit 0; final `verify.sh` exit 0 in 308.9 seconds through terminal `verify: ok` on 2026-08-11 | None | None |
| EP-016 | DEFERRED | New | Not executed; implementation forbidden until EP-011 through EP-015 pass | `.agent/execplans/EP-016-nexus-model-a2a-and-skills.md` | Optional model gateway, A2A, and skills interoperability | None |

## Baseline evidence: 2026-08-10

- Branch/worktree: `main`, clean at inspection start, `HEAD == origin/main == f38689a`.
- `bash scripts/preflight.sh`: passed with `preflight: ok`; `.env` absence was a local setup note.
- `bash scripts/verify.sh`: failed in `scripts/lint.sh` before later gates. Exact failures were one unused `EnvelopeDraft` import and denied `unwrap_used` findings in `crates/agents/tests/bridge_engineer_loop.rs`, `crates/agents/src/comms.rs`, `crates/agents/src/data_steward.rs`, and `crates/fabric/src/wiring.rs`. Vendor SQLx emitted warning-only `unexpected_cfgs` diagnostics.
- No E2E, integration, security, dependency, build, smoke, or replay result from that failed `verify.sh` run is recorded as passed.
- EP-011 repaired the named lint/test baseline, Compose profile syntax, and the vulnerable Wasmtime 38 dependency line. Bridge conformance passed on pinned Wasmtime 36.0.13 LTS; `cargo audit` reported no vulnerabilities; `cargo deny check` passed all four policy groups.
- The first repaired full run exceeded a 20-minute cold-build timeout while entering `cargo deny`; focused dependency, smoke, and TOKENKILLER tail diagnostics all passed. The unchanged warm `bash scripts/verify.sh` then completed in 487 seconds with exit zero and `verify: ok`.
- That baseline green result did not erase false gates. EP-013 later made integration failure suites fail-fast, and EP-015 later required two named E2E scenarios and made required CI/nightly checks gating.

## Transition log

| Date | From | To | Reason |
|---|---|---|---|
| 2026-08-10 | Historical EP-000 through EP-010 state | EP-011 ACTIVE; EP-012 through EP-015 QUEUED; EP-016 DEFERRED | Explicit Hydra/Nexus master directive and verified plan-vs-code divergence |
| 2026-08-10 | EP-011 COMPLETE | EP-012 ACTIVE | Reconciliation artifacts and dependency baseline passed full `verify.sh`; authenticated Nexus control-plane work may begin |
| 2026-08-11 | EP-012 COMPLETE | EP-013 ACTIVE | MCP/auth/resource-server focused tests, SQLx metadata, dependency/security gates, and the unchanged full verifier passed; governed execution may begin |
| 2026-08-11 | EP-013 COMPLETE | EP-014 ACTIVE | Governed execution, durable approval/receipt provenance, truthful runtime capabilities, checked SQL metadata, unmasked integration tests, and full `verify.sh` passed |
| 2026-08-11 | EP-014 COMPLETE | EP-015 ACTIVE | Canonical events, transactional outbox, acknowledged JetStream relay, W3C tracing, fail-closed readiness, durable fake Nexus replay/restart, checked metadata, policy gates, and full `verify.sh` passed |
| 2026-08-11 | EP-015 ACTIVE | EP-015 COMPLETE; no ACTIVE plan; EP-016 DEFERRED | Fake Nexus round trip/isolation, truthful local/CI gates, non-root image, isolated Compose modes, readiness reconciliation, independent acceptance commands, and final full verifier passed; production deployment and deferred model/A2A/skills work remain unauthorized |
