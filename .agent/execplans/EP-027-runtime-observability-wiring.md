# EP-027 Runtime Observability Wiring

Plan status: COMPLETE

## 1. Purpose / Big Picture

Make Hydra's existing Prometheus metrics registry real in the running Kernel.
The repository currently renders documented metric families, but production
mutation helpers are test-only and no request middleware records counters or
latency. Add a bounded, non-PII Kernel middleware that records request totals
and duration while preserving the existing `/metrics` contract and keeping
live dashboards, alert delivery, and staging observability drills explicitly
owned by EP-010.

## 2. Scope

- Enable the existing counter and histogram mutation helpers in production.
- Add Kernel request middleware for bounded route, method, status-class, and
  duration metrics.
- Avoid user-controlled path cardinality, query strings, tenant IDs, tokens,
  customer data, or raw headers in metric labels.
- Install the middleware in the real Kernel router path.
- Add deterministic unit and middleware tests proving recording and bounded
  route labels.
- Reconcile architecture, operations, readiness, audit, commands, ADR, and
  plan-state documentation without claiming a live monitoring deployment.

## 3. Non-goals

- No new metrics, Prometheus, OpenTelemetry, dashboard, or alert dependency.
- No external metrics exporter or network endpoint beyond the existing local
  `/metrics` route.
- No high-cardinality labels or customer/tenant identifiers.
- No change to authentication, Governor, Store, event, or rate-limit policy.
- No staging deployment, production database, release, push, merge, or tag.

## 4. Context and Orientation

`crates/kernel/src/metrics.rs` already owns the process-local Prometheus text
registry and pre-registers `hydra_requests_total` and
`hydra_request_duration_seconds`, but `inc_counter` and `observe_histogram`
are compiled only for tests. `crates/kernel/src/main.rs` exposes `/metrics`
but does not install a request metrics middleware. The correct layer is Kernel
L6: it can observe final HTTP responses without making Fabric depend on
operations code, while the route label must be reduced to a fixed allowlist.

## 5. Files to Read First

`AGENTS.md`; `COMMANDS.md`; `.agent/PLANS.md`; `.agent/EXECUTION_RULES.md`;
`.agent/state/execplan-index.md`; `ARCHITECTURE.md`; `OPERATIONS.md`;
`PRODUCTION_READINESS.md`; `NEXUS_INTEGRATION_AUDIT.md`; `DECISIONS.md`;
`crates/kernel/src/metrics.rs`; `crates/kernel/src/main.rs`;
`crates/kernel/Cargo.toml`; current Kernel metrics and smoke tests.

## 6. Files to Change

- `.agent/execplans/EP-027-runtime-observability-wiring.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `crates/kernel/src/metrics.rs`
- `crates/kernel/src/main.rs`
- `COMMANDS.md`
- `ARCHITECTURE.md`
- `OPERATIONS.md`
- `PRODUCTION_READINESS.md`
- `NEXUS_INTEGRATION_AUDIT.md`
- `DECISIONS.md`

## 7. Interfaces and Contracts

- `hydra_requests_total` remains a counter labeled only by bounded `method`,
  `route`, and `status_class` values.
- `hydra_request_duration_seconds` remains a histogram labeled only by bounded
  `method` and `route` values.
- Unknown or user-shaped paths map to `/other`; query strings are never read.
- Middleware records after the downstream response is available and never
  changes response status, body, headers, or error behavior.
- The existing `/metrics` text format and names remain backward compatible.

## 8. Milestones

### M1 - Activate and verify the metric gap

Validate the current test-only helper and missing production middleware, then
run state/preflight.

Validation: `bash scripts/check-execplan-state.sh` and
`bash scripts/preflight.sh`.

Expected: `execplan state: ok` and `preflight: ok`.

Recovery: repair plan state before Rust edits; preserve historical evidence.

### M2 - Runtime-safe registry and middleware

Remove test-only compilation from the existing mutation helpers, add bounded
route classification and response recording, and install the middleware in
the Kernel router.

Validation: `cargo test -p hydra-kernel --bin hydra-kernel metrics --offline -- --nocapture`
and `cargo check -p hydra-kernel --tests --offline`.

Expected: metrics unit/middleware tests pass and the Kernel test target
compiles.

Recovery: narrow to route classification or registry recording; never add
request-derived unbounded labels.

### M3 - Full local verification

Run lint, format, focused tests, security/dependency policy, and the complete
isolated verifier.

Validation: `bash scripts/lint.sh` followed by explicit isolated
`bash scripts/verify.sh`.

Expected: `lint: ok` and verifier exit 0 through `verify: ok`.

Recovery: fix the smallest code-owned failure; do not mask a test or reclassify
missing monitoring as passed.

### M4 - Documentation and truthful reconciliation

Document runtime collection, bounded labels, local-only registry limits, and
remaining live dashboard/alert/staging evidence. Add ADR-0037 and append the
current audit/readiness entries.

Validation: `bash scripts/check-execplan-state.sh` and
`bash scripts/preflight.sh`.

Expected: `execplan state: ok` and `preflight: ok`.

Recovery: preserve the existing metric names and EP-010 partial status.

## 9. Concrete Steps

1. Activate EP-027 and validate state/preflight.
2. Implement bounded runtime metrics recording in Kernel.
3. Run focused and full local gates.
4. Update operations, architecture, readiness, audit, command, ADR, and plan
   status artifacts.
5. Re-run state/preflight and leave EP-010 partial.

## 10. Validation and Acceptance

- Production Kernel request processing records `hydra_requests_total` and
  `hydra_request_duration_seconds`.
- Metric labels are bounded and contain no query, tenant, identity, token, or
  customer data.
- Existing `/metrics` output remains valid and includes the documented metric
  families.
- Middleware does not alter downstream responses.
- Focused tests, lint, state/preflight, security/dependency gates, and full
  isolated verification pass.
- Live dashboards, alert delivery, scrape authentication, and staging
  observability drills remain EP-010 gaps.

## 11. Idempotence and Recovery

The registry is process-local diagnostic state and resets on Kernel restart;
metrics are not a source of truth. Reinstalling the middleware is harmless
only once in the router construction. If recording fails due to a poisoned
internal mutex, the request response still passes through unchanged and the
failure is not exposed as business authority.

## 12. Progress

- [x] M1 - Observability gap verified and EP-027 activated.
- [x] M2 - Runtime metrics middleware complete.
- [x] M3 - Full local verification complete.
- [x] M4 - Documentation and truthful reconciliation complete.

## 13. Surprises & Discoveries

The first focused command used the library target and therefore reported zero
tests because `metrics.rs` is compiled by the `hydra-kernel` binary target.
The command was corrected to the documented binary target and passed all 8
metrics tests. No source or test was removed to obtain a green result.

## 14. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-11 | Keep metrics in Kernel L6 | Kernel can observe final responses without violating the six-layer law or coupling Fabric to operations. |
| 2026-08-11 | Use an allowlisted route taxonomy | Raw paths can create unbounded cardinality and may contain identifiers; unknown paths map to `/other`. |
| 2026-08-11 | Keep the registry process-local | Prometheus text is an operational diagnostic surface; durable business state and cross-replica authority remain in Store/event infrastructure. |

## 15. Outcomes & Retrospective

Runtime request metrics are now wired through Kernel L6 with bounded labels;
the focused suite passed 8 tests. `cargo check -p hydra-kernel --tests
--offline`, preflight, lint, security/dependency gates, and the isolated full
verifier passed with their required success markers. Documentation now states
that the registry is process-local diagnostic state only. EP-010 remains
partial until live dashboards, alerts, scrape/authentication review, staging
outage/observability drills, and human sign-off exist.
