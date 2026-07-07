# TESTING.md — Strategy

## Pyramid
Unit (L1 pure logic, TK canon/prefix/nukeguard) → Integration (store+PG, router+wiremock, bridge-host+fixture adapter) → Contract/Conformance (REST OpenAPI checks; adapter conformance property suite) → E2E (compose up, curl/htmx flows) → Smoke (scripts/smoke-test.sh).

## Rules
- Unit: no network, no fs, no sleep; proptest for Governor level math and canon idempotence; each test <100ms.
- Integration: dockerized Postgres (`docker compose up -d postgres`); sqlx test pools with per-test schema; wiremock fakes all HTTP providers incl. a **DeepSeek fake** that emits `prompt_cache_hit_tokens`/`prompt_cache_miss_tokens` computed by real longest-prefix matching over prior requests — this is how cache discipline is testable offline.
- Conformance (bridge): `cargo test -p bridge-host --test conformance` runs the property suite (crud round-trip, cursor stability, pagination exhaustiveness, 429 honoring, idempotent upsert, unicode, etag conflict, 10k soak marked `#[ignore]` for nightly).
- E2E: `scripts/test-e2e.sh` boots compose, seeds tenant, drives: login → create deal → agent suggests follow-up (L1) → raise cell to L3 → approve batch → bridge tab lists SuiteCRM-fixture records → TK ledger shows ratio ≥0.97 on the replay corpus.
- Contract: OpenAPI schema in fabric is asserted against handlers (utoipa) in integration tests.
- Regression: every fixed bug gets a test named `regress_<issue>`.
- Performance: criterion bench for governor.evaluate (<5ms p99 asserted loosely in test), prefix assembly bench.
- Security tests: authz matrix table-tests; PII-gate test proving pii=true request to non-private provider errors; NukeGuard abort test with 1MB dump fixture.
- Accessibility: e2e asserts landmarks/labels present, all POST forms function with htmx disabled (plain HTTP fallback).

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
