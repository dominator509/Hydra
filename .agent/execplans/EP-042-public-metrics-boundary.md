# EP-042 - Public Metrics Boundary

Plan status: COMPLETE

## 1. Purpose / Big Picture

Close the verified ingress exposure of Kernel's process-local `/metrics`
endpoint. Caddy currently reverse-proxies every path, while Prometheus already
has an internal direct scrape path. This plan denies `/metrics*` at Caddy,
preserves internal scraping, and adds executable policy and regression tests.

## 2. Scope

Implement SPEC-022 in the reference Caddyfile, add a dependency-free static
ingress policy check and fixture tests, wire them into preflight/full
verification, and reconcile deployment/readiness documentation.

## 3. Non-goals

- No metrics payload redesign, authentication system, dashboard, alert
  receiver, or external observability service.
- No public port changes beyond denying the metrics path.
- No runtime API, database schema, migration, tenant, event, or Governor
  change.
- No staging deployment, live monitoring, human observability review, or
  production action.
- No Caddy image upgrade, dependency, provider, or network redesign.
- No push, tag, merge, release, or production database operation.

## 4. Context and Orientation

EP-027/031/040 added bounded Kernel metrics and an optional internal
Prometheus/Alertmanager profile. `docker/compose.yaml` places Prometheus on
`ingress-internal` and `observability-internal`, publishes no observability
ports, and keeps Kernel off host ports. `docker/Caddyfile` currently has a
catch-all `reverse_proxy kernel:8080` under the public `:443` site, so
`/metrics` is reachable through the public listener. The smallest correction
is an explicit Caddy path denial before the catch-all handle.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/state/execplan-index.md`
- `.agent/specs/SPEC-020-scheduler-concurrency-and-observability.md`
- `.agent/specs/SPEC-022-public-metrics-boundary.md`
- `docker/Caddyfile`
- `docker/compose.yaml`
- `docker/prometheus.yml`
- `scripts/check-observability.sh`
- `scripts/verify.sh`
- `scripts/preflight.sh`
- `DEPLOYMENT.md`
- `OPERATIONS.md`
- `PRODUCTION_READINESS.md`
- `TESTING.md`
- `DECISIONS.md`

## 6. Files to Change

- `.agent/specs/SPEC-022-public-metrics-boundary.md`
- `.agent/execplans/EP-042-public-metrics-boundary.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `docker/Caddyfile`
- `scripts/check-ingress-policy.sh`
- `scripts/test-ingress-policy.sh`
- `scripts/preflight.sh`
- `scripts/verify.sh`
- `COMMANDS.md`
- `TESTING.md`
- `DEPLOYMENT.md`
- `OPERATIONS.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`

## 7. Interfaces and Contracts

- `docker/Caddyfile` defines named matcher `@private_metrics` for
  `/metrics*`, a denial `handle @private_metrics`, and a separate catch-all
  `handle` containing `reverse_proxy kernel:8080`.
- `scripts/check-ingress-policy.sh` reads `HYDRA_CADDYFILE` when supplied
  for fixture testing, otherwise `docker/Caddyfile`, and emits
  `ingress policy: ok` only when the required blocks are present.
- `scripts/test-ingress-policy.sh` uses temporary copies and proves the
  accepted file plus missing matcher, missing denial, and missing catch-all
  proxy failures.
- The existing Prometheus target remains `kernel:8080`; no public route is
  used for scraping.

## 8. Milestones

### M1 - Contract and active-plan state

Add SPEC-022, activate EP-042, extend the state checker/index, and record the
verified public exposure. Validate with `bash scripts/preflight.sh` and
`bash scripts/check-execplan-state.sh`; expect `preflight: ok` and
`execplan state: ok`. Recovery: correct plan/index state before implementation.

### M2 - Caddy boundary and policy tests

Add the named metrics denial, static policy checker, and negative fixture
tests. Validate with `sh scripts/test-ingress-policy.sh` and
`sh scripts/check-ingress-policy.sh`; expect `ingress policy: ok` from each
successful check. Recovery: inspect the exact Caddy block and preserve the
internal Prometheus target.

### M3 - Verification and documentation wiring

Add ingress policy tests/checks to preflight and full verification, and update
deployment/operations/testing/readiness/command/decision documents. Validate
with preflight, state, focused ingress tests, observability policy, and
`git diff --check`.

### M4 - Full acceptance

Run the focused policy checks and `bash scripts/verify.sh` against the
documented disposable loopback services; expect `verify: ok`. No production
deployment or live observability claim is authorized.

## 9. Concrete Steps

1. Confirm EP-041 is complete and no active plan exists.
2. Add the accepted SPEC-022, active EP-042, index row, and state-checker
   entry.
3. Add the explicit Caddy denial and policy/fixture tests.
4. Wire policy checks into preflight/full verification.
5. Reconcile docs and append ADR-0052 without claiming staging evidence.
6. Run milestone validations in order and record exact outputs below.

## 10. Validation and Acceptance

- `bash scripts/preflight.sh` -> `preflight: ok`.
- `bash scripts/check-execplan-state.sh` -> `execplan state: ok`.
- `sh scripts/test-ingress-policy.sh` -> `ingress policy tests: ok`.
- `sh scripts/check-ingress-policy.sh` -> `ingress policy: ok`.
- `bash scripts/check-observability.sh` -> `observability policy: ok`.
- Fixture mutations missing the matcher, denial, or catch-all proxy fail.
- The Compose policy still has no host-published Kernel/Prometheus/
  Alertmanager/NATS ports.
- `bash scripts/verify.sh` -> `verify: ok`.
- No staging or production action occurs.

## 11. Idempotence and Recovery

The plan changes no database state and is safe to rerun. Fixture tests create
only temporary copies under the system temp directory and remove them through
a trap. If Caddy validation is unavailable, the static policy still provides
the repository gate; do not weaken the deny rule or claim live Caddy startup
evidence. If full verification is interrupted, rerun it against disposable
loopback services.

## 12. Progress

- [x] M1 - Contract and active-plan state (`preflight: ok`; `execplan state: ok`)
- [x] M2 - Caddy boundary and policy tests (`ingress policy tests: ok`; `ingress policy: ok`)
- [x] M3 - Verification and documentation wiring (`observability policy: ok`; preflight/state/diff checks passed; Nexus Compose config exited 0)
- [x] M4 - Full acceptance (current-state `bash scripts/verify.sh` exited 0 in 335.5s through `verify: ok`; static ingress/observability/Compose gates passed)

## 13. Surprises & Discoveries

The Docker Compose normalized configuration and static policy pass, but
`docker compose config` does not validate Caddy directive semantics. The
repository therefore keeps a structural policy gate and does not claim live
Caddy startup behavior without an explicit Caddy runtime validation. The
documented Caddy validation command timed out after 304 seconds, and the
narrow `docker image inspect caddy:2.8-alpine` diagnostic also timed out; the
Docker daemon/image path is unavailable in this environment.

## 14. Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-08-12 | Deny `/metrics*` at Caddy with a dedicated handle | The public catch-all proxy currently forwards the process-local metrics surface, while internal Prometheus already has a direct path. |
| 2026-08-12 | Keep direct Kernel metrics and internal Prometheus scraping unchanged | This corrects ingress exposure without removing existing local observability behavior. |
| 2026-08-12 | Use a static policy checker plus negative fixtures | The boundary is configuration-owned; fixture tests catch accidental removal without adding dependencies or requiring live deployment. |
| 2026-08-12 | Treat Caddy runtime validation as an explicit environment-bounded residual | The Docker compose config and static policy passed, but both the Caddy validation command and a narrow image-inspect diagnostic timed out before returning; no live Caddy behavior is claimed. |

## 15. Outcomes & Retrospective

Completed 2026-08-12. Caddy now denies `/metrics*` with `404` before the
catch-all Kernel proxy, and internal Prometheus continues to scrape
`kernel:8080/metrics` over `ingress-internal`. Static policy tests prove the
matcher, denial, and proxy structure, including negative fixtures for each
missing component. Preflight, ExecPlan state, observability policy, Nexus
Compose normalization, diff checks, and the current-state full verifier all
passed; `verify.sh` exited 0 in 335.5s through `verify: ok`. Docker-based Caddy
runtime validation timed out before producing a result, so no live Caddy
startup or staging observability claim is made. EP-010 remains partial and no
production action occurred.
