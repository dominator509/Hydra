# TESTING.md — Strategy

## Pyramid
Unit (L1 pure logic, TK canon/prefix/nukeguard) → Integration (Store+Postgres, router+wiremock, bridge-host+fixture adapter, JetStream) → Contract/Conformance (MCP/auth/event snapshots and adapter conformance) → E2E (fake Nexus over real local HTTP/Postgres/JetStream) → Smoke (`scripts/smoke-test.sh`).

## Rules
- Bridge lifecycle tests must prove equal adapter IDs remain isolated across tenants. The focused Store contract is `cargo test -p store --test adapter_kv --offline -- --nocapture`; the BridgeHost contract is `cargo test -p bridge-host --test store_kv --test lifecycle --offline -- --nocapture`. These tests use disposable PostgreSQL schemas and do not validate production readiness.
- Unit: no network, no fs, no sleep; proptest for Governor level math and canon idempotence; each test <100ms.
- Integration: dockerized Postgres (`docker compose up -d postgres`) targeted through `HYDRA_TEST_DATABASE_URL` (loopback-only; it falls back to `DATABASE_URL` for compatibility); sqlx test pools with per-test schema; wiremock fakes all HTTP providers incl. a **DeepSeek fake** that emits `prompt_cache_hit_tokens`/`prompt_cache_miss_tokens` computed by real longest-prefix matching over prior requests — this is how cache discipline is testable offline.
- Conformance (bridge): `cargo test -p bridge-host --test conformance` runs the property suite (crud round-trip, cursor stability, pagination exhaustiveness, 429 honoring, idempotent upsert, unicode, etag conflict, 10k soak marked `#[ignore]` for nightly).
- Governed bridge lifecycle and synthesis: `cargo test -p bridge-host lifecycle -- --nocapture`, `cargo test -p store bridge_adapter_registry -- --nocapture`, `cargo test -p agents bridge_engineer --offline -- --nocapture`, and the loopback Kernel/Fabric `bridge_lifecycle` and `a2a_workflows` suites prove component-root trust, digest/grant/fuel checks, bounded TOKENKILLER mapping proposals, durable tenant-scoped state, typed execution, and proposal-only Fabric behavior. These tests do not claim staging deployment, generated code, synchronization, conformance, canary, or promotion.
- Governed bridge synchronization: `cargo test -p store --test bridge_sync --offline -- --nocapture`, `cargo test -p bridge-host --test lifecycle --offline -- --nocapture`, `cargo test -p hydra-kernel --test bridge_lifecycle --offline -- --nocapture`, and `cargo test -p fabric --test bridge_lifecycle --offline -- --nocapture` prove transactional cursor/run/conflict behavior, tenant-scoped WIT page invocation, bounded full-relist fallback, typed runtime registration, path-bound authenticated proposals, and idempotency. These tests do not claim scheduler, staging, or production evidence.
- Governed bridge scheduling: `cargo test -p store --test bridge_schedules --offline -- --nocapture`, `cargo test -p fabric --test scheduled_proposals --offline -- --nocapture`, `cargo test -p hydra-kernel --lib bridge_scheduler --offline -- --nocapture`, and `cargo test -p hydra-kernel --test bridge_scheduler --offline -- --nocapture` prove tenant-scoped lease claims, stale-lease recovery, two-worker exclusivity, interrupted-proposal replay, governed scheduler provenance/idempotency, real claim-to-envelope polling, and deterministic slot identity. `cargo test -p hydra-kernel --lib metrics --offline -- --nocapture` proves bounded scheduler outcome labels. These tests are disposable loopback evidence only and do not claim EP-010 staging or production readiness.
- E2E: `scripts/test-e2e.sh` fails if either required scenario is absent, then runs `e2e_nexus_round_trip` and `e2e_cross_business_blocked` serially against isolated Postgres schemas and a JetStream-enabled local NATS. The round trip uses a loopback issuer/JWKS plus service, agent, and distinct human tokens; it proves capability/context reads, MCP search/proposal, governed approval/execution, audit/outbox/event delivery, and retry idempotency. It does not claim browser, shell accessibility, or staging evidence.
- Smoke: `scripts/smoke-test.sh` runs the real Kernel health/readiness child against a unique `store::TestDb` schema that owns its migrations and cleanup. The child receives a URL-scoped `search_path`; the root database does not need pre-applied migrations, and the test does not weaken the Store-backed limiter or readiness checks.
- Contract: Fabric snapshots MCP tool/input/output schemas and tests asymmetric auth, Origin, binding, scope, structured output, and compatibility behavior. CDM/Kernel tests validate canonical event schemas, acknowledged relay, replay, and durable consumer behavior.
- A2A/skills: `cargo test -p fabric --test a2a_workflows -- --nocapture` covers authenticated A2A 1.0 task idempotency, tenant scoping, cancellation/resume, bounded bridge-synthesis proposal discovery, and unavailable workflow truth. `cargo test -p agents skill_trust -- --nocapture` covers upstream metadata, Ed25519 manifest/hash verification, key rotation/revocation, semantic version conflicts, sandbox policy, and least-authority rejection. Kernel runtime-wiring tests prove standalone skill disablement, partial configuration failure, and Experimental synthesis availability only with a provider chain.
- Bridge conformance: `cargo test -p bridge-host --lib conformance_rejects --offline -- --nocapture`, `cargo test -p bridge-host --test lifecycle conformance --offline -- --nocapture`, `cargo test -p hydra-kernel --test bridge_lifecycle --offline -- --nocapture`, and `cargo test -p fabric --test a2a_workflows conformance --offline -- --nocapture` prove bounded page validation, digest/grant enforcement, runtime availability, tenant isolation, no CRM mutation, authenticated A2A discovery, durable idempotency, and redacted failure artifacts. These tests do not claim adapter activation, synchronization, staging, or EP-010 production readiness.
- Regression: every fixed bug gets a test named `regress_<issue>`.
- Performance: `bash scripts/test-performance.sh` requires the release-only Governor p99 test, named `c9_soak_10k` bridge conformance soak, and TOKENKILLER cache-hit audit. This is local executable evidence only; shell/API latency, staging soak, and human performance review remain EP-010 gates.
- Security tests: authz matrix table-tests; PII-gate test proving pii=true request to non-private provider errors; NukeGuard abort test with 1MB dump fixture.
- Accessibility: `bash scripts/test-shell-accessibility.sh` proves checked-in Shell landmarks, native disclosures, and native POST fallbacks. Browser keyboard, screen-reader, contrast, and no-htmx review remain staging-owned EP-010 evidence; the Nexus E2E gate does not satisfy them.
- Deployment safety: `bash scripts/test-deployment-safety.sh` checks digest-pinned staging/promotion, strict SSH host trust, health/readiness gates, immutable-only promotion, and a distinct safe dry-run marker without contacting a registry, host, or staging URL.
- Release immutability: `bash scripts/check-release-policy.sh` and the deployment safety gate reject a mutable Hydra `latest` tag in the tag-triggered release workflow while leaving local `HYDRA_TAG` Compose behavior unchanged.
- NATS transport: `cargo test -p hydra-kernel --test nats_transport --offline -- --nocapture` proves secure auth/TLS defaults, credentials-file and mTLS pairing, URL secret rejection, redacted errors, and plain dev compatibility; `bash scripts/check-nats-policy.sh` validates the checked-in Compose/docs gate without contacting a broker.
- Production-readiness evidence: `bash scripts/test-readiness-evidence.sh` uses temporary ledgers to prove exact `PASS` rows, named owners, fresh ISO dates, required launch checks, human sign-off presence, and fail-closed rejection of pending, stale, malformed, duplicate, future, and placeholder evidence. It does not create staging evidence.
- Public ingress: `bash scripts/test-ingress-policy.sh` exercises temporary Caddyfile fixtures and proves the `/metrics*` denial, dedicated denial handle, and catch-all Kernel proxy cannot be removed without failing. `bash scripts/check-ingress-policy.sh` validates the checked-in Caddy topology; it does not replace a staged Caddy startup or external penetration test.
- Ingress-aware smoke: `bash scripts/test-smoke-boundary.sh` uses a fake curl boundary to prove `HYDRA_SMOKE_URL` is used only for public health/readiness, optional metrics checks use `HYDRA_SMOKE_INTERNAL_METRICS_URL`, and equal URLs fail closed. The direct in-repository smoke still checks Kernel metrics internally.

## Test data / fixtures / mocking
Fixtures in `crates/*/tests/fixtures/`; factory fns in `crates/store/src/testkit.rs`. Mock only at trust boundaries (HTTP, clock via injected `Clock` trait). Never mock the Governor.

## Validation matrix (per feature)
| Change type | Required |
|---|---|
| L1 domain | unit + proptest |
| store/schema | migration + integration |
| fabric endpoint | contract + integration |
| agent behavior | unit (prompt assembly via TK) + integration (fake provider) |
| adapter | conformance suite |
| shell view | e2e path |
| TK segments | replay cache-hit-audit ≥0.97 |

## Flaky policy
A test failing intermittently 2× in CI gets `#[ignore]` + issue + owner within the same day; never retry-loop in CI config to hide it.

## Definition of test done
Named per behavior, asserts observable output (not internals), runs in the matrix row's suite, green in `scripts/verify.sh`.
