# EP-015 Nexus E2E, Deployment, and Gates

Plan status: COMPLETE

## 1. Purpose / Big Picture
Prove the complete local Hydra/Nexus seam with a deterministic fake Nexus harness, remove every false-green validation path, validate Docker/Compose packaging for standalone and Nexus-connected modes, and report production readiness honestly without deploying production.

## 2. Scope
Fake issuer/JWKS and Nexus service/agent/human principals; binding bootstrap; MCP/REST clients; durable event consumer; full governed stage-change/idempotency scenario and cross-business denial; real required E2E discovery; unmasked integration/nightly/CI gates; Docker/Compose network/port/egress corrections; Nexus-enabled profile; package contract; readiness documentation and exact acceptance commands.

## 3. Non-goals
No real Nexus repository/cloud IdP, production deployment, push/tag/release, production database operation, whole Nexus stack in this repo, GraphQL, A2A/model/skills work, or claim that EP-010 staging drills/soak/reviews/human sign-off passed.

## 4. Context and Orientation
EP-012 through EP-014 establish auth/binding/capabilities, governed execution, and durable events. Current `test-e2e.sh` succeeds with no tests, integration masks bridge failures, nightly marks required checks optional, and Compose gives the egress proxy only an internal network while publishing kernel and NATS ports. EP-015 must prove local interoperability and packaging, not production readiness.

## 5. Files to Read First
`.agent/specs/SPEC-010-nexus-interoperability.md`; `TESTING.md`; `ENVIRONMENT.md`; `DEPLOYMENT.md`; `PRODUCTION_READINESS.md`; `OPERATIONS.md`; `COMMANDS.md`; `scripts/{test-e2e.sh,test-integration.sh,verify.sh,security-check.sh,dependency-audit.sh,smoke-test.sh}`; `.github/workflows/{ci.yml,nightly.yml,release.yml}`; `docker/{compose.yaml,Dockerfile,Caddyfile,egress-proxy.conf}`; EP-012 through EP-014 tests/runtime; `.agent/state/execplan-index.md`.

## 6. Files to Change
`COMMANDS.md`; `TESTING.md`; `ENVIRONMENT.md`; `DEPLOYMENT.md`; `PRODUCTION_READINESS.md`; `NEXUS_INTEGRATION.md`; `NEXUS_PACKAGE_CONTRACT.md` (new); `README.md`; `scripts/test-e2e.sh`; `scripts/test-integration.sh`; `scripts/verify.sh`; `scripts/preflight.sh`; `scripts/security-check.sh` only if fixture-safe scanning needs a stricter evidence-backed rule; `.github/workflows/ci.yml`; `.github/workflows/nightly.yml`; `.github/workflows/release.yml` only to prevent unintended deployment or add validated prerequisites; `docker/compose.yaml`; `docker/Dockerfile`; `docker/Caddyfile`; `docker/egress-proxy.conf`; `docker/nexus.env.example` (new); `crates/kernel/tests/e2e_nexus.rs` (new); `crates/kernel/tests/support/mod.rs` (new or updated); `crates/kernel/tests/support/fake_nexus.rs` (new); `crates/kernel/tests/support/fake_mcp_client.rs` (new); `crates/kernel/tests/support/fake_event_consumer.rs` (new); `crates/kernel/tests/fixtures/nexus/README.md` (new); any exact test-only lint files discovered by required all-target clippy with a Decision Log entry; this plan; `.agent/state/execplan-index.md`; `.agent/execplans/EP-010-production-readiness.md` reality status only.

M1 justified additions: `crates/kernel/Cargo.toml`, `crates/kernel/tests/support/fake_nexus_consumer.rs`, and `DECISIONS.md`. The E2E test crate needs direct declarations for already-locked HTTP/JWT crates, and the existing consumer result gains additive event identity/provenance fields so the round trip can assert the exact canonical event without duplicating its validator. ADR-0026 records the dependency boundary.

M4 justified addition: `.dockerignore`. Without it, Docker sends local Rust build outputs, Git metadata, ignored environment files, and private agent-tool state into the image build context. The exclusion is packaging/security hygiene and does not change runtime behavior.

M5 justified additions: `SECURITY.md`, `OPERATIONS.md`, `scripts/production-readiness-check.sh`, `scripts/check-execplan-state.sh`, and `scripts/db-restore.sh`. Final truth reconciliation found stale security/operations claims, skip-capable readiness gates, an unnecessary restore-error suppression, and an index validator unable to represent the truthful terminal state where EP-015 is complete and EP-016 remains deferred.

## 7. Interfaces and Contracts
The harness runs locally with deterministic asymmetric test keys and no live Nexus. The primary `e2e_nexus_round_trip` performs auth -> capability discovery -> context -> search -> idempotent stage proposal -> Governor queue/execute -> distinct human approval when queued -> typed execution/verification -> audit/outbox -> JetStream event -> durable fake consumer correlation -> retry deduplication. `e2e_cross_business_blocked` proves no cross-business access. Scripts fail if required tests are absent. Production profile exposes only Caddy ingress by default; Postgres/NATS/kernel stay internal; egress proxy spans internal backend plus narrow external network.

## 8. Milestones
M1 Fake Nexus harness and identity clients. Implement local issuer/JWKS, principals, binding bootstrap, MCP/REST clients, and durable event consumer. Validation: `cargo test -p hydra-kernel fake_nexus_harness -- --nocapture`. Expected: all harness component tests pass without network cloud dependencies. Recovery: isolate issuer, binding, transport, and consumer fixtures; no fallback identity.

M2 Full round trip and isolation E2E. Implement both required `e2e_` scenarios with ephemeral tenant/business data and actual local Postgres/JetStream. Validation: `bash scripts/test-e2e.sh`. Expected: `e2e tests: ok` only after both named tests pass. Recovery: narrow to the first failed numbered stage; retain durable audit/outbox evidence and do not bypass queue/approval.

M3 Truthful local and CI gates. Remove `|| true`/required `continue-on-error`, require E2E presence/count, separate truly informational nightly work, add explicit auth/event/fake-Nexus/Docker/Compose checks to CI, and resolve all discovered failures. Validation: `bash scripts/test-integration.sh` then `bash scripts/test-e2e.sh`. Expected: `integration tests: ok`, `failure suites: ok`, `e2e tests: ok`. Recovery: a flaky/failing required suite remains failing until fixed under TESTING policy; never reclassify without evidence.

M4 Deployment topology and package contract. Correct internal networks/ports/egress, support standalone and Nexus-connected config, document image/env/volumes/migration/health/bootstrap/trust/backup/rollback/upgrades. Validation: `docker build -f docker/Dockerfile -t hydra/kernel:local .` then `docker compose -f docker/compose.yaml config`. Expected: image build exit 0 and Compose config exit 0 with no public NATS/default direct-kernel production exposure. Recovery: inspect normalized Compose; keep kernel off the external-egress network and give only proxy dual attachment.

M5 Acceptance matrix and readiness reconciliation. Run every required command independently, update index/EP-010/readiness with exact evidence, and confirm no deployment. Validation: `bash scripts/preflight.sh`; `bash scripts/test-unit.sh`; `bash scripts/test-integration.sh`; `bash scripts/test-e2e.sh`; `bash scripts/security-check.sh`; `bash scripts/dependency-audit.sh`; `bash scripts/verify.sh`. Expected, in order: `preflight: ok`, `unit tests: ok`, `integration tests: ok`, `failure suites: ok`, `e2e tests: ok`, `security check: ok`, `dependency audit: ok`, `verify: ok`. Recovery: stop the sequence at first failure, fix its root under bounded retry, rerun that command, then restart the full matrix.

## 9. Concrete Steps
Execute M1-M5 in order. Use only test databases/streams and unique fixtures. Verify no production endpoint/credential is configured. Inspect `git diff --name-only`, required gate masking patterns, normalized Compose, and plan index. Mark EP-015 complete only after every command and both Docker validations pass. Keep EP-016 deferred.

## 10. Validation and Acceptance
All EP-015 acceptance criteria in the master directive pass with exact signals; fake Nexus round trip and cross-business block are executable; no required test is absent/masked; Docker image and Compose validate; standalone/Nexus-connected modes are documented; package contract is complete; current status index is truthful; EP-010 remains partial; no push/tag/release/deploy/production DB operation occurred; EP-016 remains deferred.

## 11. Idempotence and Recovery
Harness fixtures use unique IDs and test-only schemas/streams; retry proves idempotency. Compose config/build are read/build-only. Gate scripts are deterministic and rerunnable. CI changes do not trigger a release locally. If interrupted, consult Progress and test artifacts; never clean a non-test database/stream or run deploy scripts.

## 12. Progress
- [x] M1 - Fake Nexus harness components (`cargo test -p hydra-kernel fake_nexus_harness -- --nocapture --test-threads=1`: 1 passed, 2026-08-11)
- [x] M2 - Round trip and cross-business E2E (`bash scripts/test-e2e.sh`: 2 passed and `e2e tests: ok`, 2026-08-11)
- [x] M3 - Truthful local/CI gates (`bash scripts/test-integration.sh`: `integration tests: ok` and `failure suites: ok` in 263.8 seconds; `bash scripts/test-e2e.sh`: 2 passed and `e2e tests: ok`; workflow YAML parse and zero required-mask scan passed, 2026-08-11)
- [x] M4 - Deployment topology and package contract (`docker build -f docker/Dockerfile -t hydra/kernel:local .`: exit 0 in 827.1 seconds with `adapters: ok`; standalone and Nexus Compose config: exit 0; image/network/Caddy inspections passed, 2026-08-11)
- [x] M5 - Full acceptance matrix and readiness reconciliation (`preflight: ok`; `unit tests: ok`; `integration tests: ok`; `failure suites: ok`; two E2E tests and `e2e tests: ok`; `security check: ok`; `dependency audit: ok`; final `verify.sh` exit 0 in 308.9 seconds through terminal `verify: ok`, 2026-08-11)

## 13. Surprises & Discoveries
- 2026-08-11: Activated only after EP-014's five focused milestones, checked metadata/dependency/security gates, durable real-JetStream consumer test, and full verifier passed.
- 2026-08-11: EP-015 preflight passed with `preflight: ok`; the absent `.env` remained the documented local note, while test service URLs are injected explicitly.
- 2026-08-11: The real harness can reuse Fabric's deterministic Ed25519 key material while serving only a public JWK from loopback. Four distinct signed tokens share one cached JWKS fetch, and persisted business bindings remain the only source of Hydra tenant authority.
- 2026-08-11: The first runtime E2E reached and acknowledged the canonical executed event before the relay's subsequent Store update made `published_at` observable. This is the documented at-least-once crash boundary, not a missing acknowledgement; the assertion now polls the authoritative outbox receipt with a bounded timeout.
- 2026-08-11: The first `test-e2e.sh` attempt found a test-only JSON `Value` versus `Uuid` comparison that did not compile; comparing the canonical string representation fixed the narrow assertion. The next runtime attempt proved cross-business denial but exposed the expected publish-ack/Store-receipt observation race described above. The third unchanged gate passed both tests.
- 2026-08-11: Required-mask scanning after M3 found no `|| true`, `continue-on-error`, or absent-E2E warning path in the local integration/E2E scripts or GitHub workflows. The scan's `rg` exit code 1 represented the expected empty match set; all three edited workflow documents parsed as YAML.
- 2026-08-11: The prior Compose defaulted `HYDRA_ENV` to the runtime-invalid value `development`, exposed kernel/NATS ports, shared the data and event trust zone, placed kernel on an external-capable network, and left Tinyproxy without external egress. The migration service also used singular `profile` and redundantly supplied the image entrypoint as an argument.
- 2026-08-11: The prior Dockerfile invoked `scripts/build-adapters.sh` without installing its pinned WASI target/tool and copied nonexistent `/app/static`; shell static content is actually embedded with `include_str!`. The corrected clean-context build compiled the Kernel in 8m31s, built the adapter in 28s, and exported the local image successfully.
- 2026-08-11: Caddy validation was behaviorally green but initially reported noncanonical indentation; applying its non-mutating format diff removed that diagnostic. Its remaining HTTP-server warning is expected because port 80 is an explicit redirect-only site.
- 2026-08-11: The first final `verify.sh` attempt stopped correctly in all-target Clippy because the fake issuer helper accepted eight positional arguments. Replacing them with a typed `TokenRequest` removed both the lint violation and claim-ordering risk; no runtime code or token content changed.

## 14. Decision Log
| Date | Decision | Rationale |
|---|---|---|
| 2026-08-10 | Queue behind EP-014 and prohibit deployment | E2E needs the completed auth, execution, and event contracts; user expressly forbids production actions |
| 2026-08-11 | Activate after EP-014 completion | The authenticated command path and acknowledged event contract are now verified inputs to the final E2E and packaging gates |
| 2026-08-11 | Reuse workspace-pinned HTTP/JWT crates as Kernel dev-dependencies | Real loopback REST/MCP/JWKS tests need direct test-crate imports; ADR-0026 keeps them out of the production Kernel dependency surface |
| 2026-08-11 | Mirror Kernel production assembly from public components instead of exporting `main` internals | The harness exercises the same Store, persisted Governor, RuntimeServices, Fabric AppState, Executor, relay, and JetStream types while keeping binary boot helpers private |
| 2026-08-11 | Make `scripts/test-e2e.sh` require both named Nexus scenarios | A missing or renamed E2E target now fails before Cargo runs, so zero-test discovery can never produce the success marker |
| 2026-08-11 | Make nightly verify, ignored conformance soak, and cache audit one gating job | These checks are required evidence; a summary may report failure but may not convert it into success or continue past it as if green |
| 2026-08-11 | Start NATS with explicit `-js` in CI, nightly, and release verification | GitHub service syntax did not prove JetStream was enabled; the loopback `/jsz` readiness probe now establishes the exact broker mode required by event and E2E tests |
| 2026-08-11 | Split Compose into dedicated ingress, data, events, proxy, and external edge networks | Only Caddy may publish ingress; a Nexus consumer may reach the attachable event network without obtaining a Postgres path; only Tinyproxy receives application-side external egress |
| 2026-08-11 | Add `.dockerignore` as an M4 security exception to Expected Changed Files | Build context must exclude ignored secrets, local tool state, Git metadata, and multi-gigabyte Rust outputs |
| 2026-08-11 | Permit zero ACTIVE rows only after EP-015 is COMPLETE while EP-016 stays DEFERRED | The single-active invariant applies during executable work; inventing a new active plan or activating forbidden EP-016 would make the terminal ledger false |
| 2026-08-11 | Replace stale production-security claims with verified controls plus explicit gaps | Local Nexus evidence cannot substantiate nonexistent vault/cron/restore/session-hardening or staging claims |
| 2026-08-11 | Complete EP-015 without activating EP-016 | Every EP-015 local acceptance gate passed; the user explicitly required EP-016 to remain deferred and production readiness remains an independent EP-010 program |

## 15. Outcomes & Retrospective
Complete. M1 passed the loopback issuer/JWKS, cached asymmetric validation, Store-backed two-business bindings, distinct principal types, production-like runtime assembly, REST reads, and MCP `2025-11-25` initialization. M2's fail-closed script passed both named E2E scenarios: the full governed stage-change round trip with idempotent retry and the non-leaking cross-business denial. M3 removed false-green local/CI paths. M4 built the 48.3 MB non-root image with its WASI adapter and split Compose into ingress, data, events, proxy, and edge trust zones; only Caddy publishes ports, and standalone/Nexus-enabled configs both validate.

M5 independently passed preflight, unit, integration plus required failure suites, E2E, security, dependency/license policy, image/Compose checks, and the full verifier. The first full verifier correctly exposed one test-helper Clippy violation; a typed request object fixed it, full lint passed, and the restarted verifier exited zero in 308.9 seconds through its unconditional `verify: ok` terminal path. EP-010 remains partial because local fake-Nexus evidence does not supply staging drills, soak, real TLS/IdP, recovery, reviews, benchmarks, or human sign-off. Residual risks are the policy-allowed `fxhash` and `event-listener` advisories, process-local rate limits, missing production vault/bridge-secret wiring, owner-operated binding bootstrap, local-CA Caddyfile, absent retention scheduler/restore drills, and absent SBOM/signed provenance policy. No live Nexus, cloud identity service, production database/NATS, push, merge, tag, release, or deployment was used.
