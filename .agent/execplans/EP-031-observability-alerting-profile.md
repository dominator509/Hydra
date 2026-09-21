# EP-031 Observability Alerting Profile

Plan status: COMPLETE

## 1. Purpose / Big Picture

Hydra already exposes bounded process metrics and commits Prometheus alert
rules, but the reference Compose topology does not run a scraper or an alert
manager. The D3 runbook therefore names an alerting path that is not
executable from the repository. This plan adds an optional, internal-only
observability profile that wires the existing Kernel metrics and rules to
Prometheus and Alertmanager without claiming that an operator notification
receiver, dashboard service, or staging drill exists.

## 2. Scope

- Add pinned Prometheus and Alertmanager configuration files.
- Add an optional `observability` Compose profile for the two services.
- Keep observability services off all host-published ports and away from the
  data, events, and egress networks.
- Add a static configuration contract check and make it a required local gate.
- Update the deployment contract, D3 runbook, readiness evidence, and
  decision history.

## 3. Non-goals

- No Grafana, log shipping, OTLP exporter, or new application metrics.
- No default external webhook, email, paging integration, or notification
  secret. The checked-in Alertmanager receiver is intentionally empty and an
  operator-owned overlay is required for real delivery.
- No staging or production deployment, database operation, or claim that D3
  or EP-010 is complete.
- No new Rust, Node/npm, or application dependency.
- No changes to NukeGuard thresholds or the existing Prometheus metric names.

## 4. Context and Orientation

`crates/kernel/src/metrics.rs` renders the process-local `/metrics` surface.
`docker/alerts.yaml` contains the existing TOKENKILLER, cost, and envelope
rules. `docker/compose.yaml` currently provides Kernel, Caddy, Postgres, NATS,
and Tinyproxy only. The observability services must reach only the Kernel
metrics endpoint and Alertmanager must not become a new authority for CRM
state or tenant identity.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/state/execplan-index.md`
- `docker/compose.yaml`
- `docker/alerts.yaml`
- `crates/kernel/src/metrics.rs`
- `scripts/verify.sh`
- `scripts/preflight.sh`
- `OPERATIONS.md`
- `DEPLOYMENT.md`
- `PRODUCTION_READINESS.md`
- `.agent/execplans/EP-008-observability-and-operations.md`

## 6. Files to Change

- `.agent/execplans/EP-031-observability-alerting-profile.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `scripts/check-observability.sh`
- `scripts/preflight.sh`
- `scripts/verify.sh`
- `docker/compose.yaml`
- `docker/prometheus.yml`
- `docker/alertmanager.yml`
- `docker/alerts.yaml`
- `COMMANDS.md`
- `DEPLOYMENT.md`
- `OPERATIONS.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`
- `.agent/execplans/EP-008-observability-and-operations.md`

## 7. Interfaces and Contracts

- The profile is enabled explicitly with `--profile observability`.
- Prometheus scrapes `http://kernel:8080/metrics` over the existing internal
  ingress network and loads `docker/alerts.yaml` without rewriting rules.
- Prometheus sends firing alerts to `alertmanager:9093` over a dedicated
  internal observability network.
- Alertmanager groups by alert name and severity and has a receiver-neutral
  `hydra-operator` receiver with no default outbound destination.
- No observability service publishes a host port. Operators may use an
  authenticated network path or an explicit, separately reviewed Compose
  override for administration and notification delivery.
- The static gate validates YAML structure, required service/network/profile
  boundaries, rule names, scrape target, and the absence of host ports.
- Existing metrics and alert rule names remain backward compatible.

## 8. Milestones

### M1 - Activate and baseline the observability contract

Activate EP-031 as the only active plan and add its state/index/checker
coverage. Run `bash scripts/check-execplan-state.sh` and
`bash scripts/preflight.sh`.

Expected output: `execplan state: ok` and `preflight: ok`.

Recovery: repair only the state ledger or plan status; do not change runtime
files until the single-active-plan invariant passes.

### M2 - Add the internal observability profile

Add Prometheus and Alertmanager configuration, Compose services, internal
networking, pinned images, and the static `check-observability.sh` contract.
Wire the check into preflight and the full verifier. Run the focused check and
the Compose config command from `COMMANDS.md` with the example environment.

Expected output: `observability policy: ok` and Compose exits 0.

Recovery: first validate the individual YAML files and then the normalized
Compose model; preserve the default standalone profile if a profile-only
service fails.

### M3 - Reconcile operations and readiness evidence

Update commands, deployment topology, D3 steps, EP-008 reality status, and
production-readiness evidence. Run shell syntax, security, and focused
operational checks.

Expected output: `observability policy: ok`, `security check: ok`, and
`operational tools: ok`.

Recovery: keep live notification and staging evidence marked open; do not
convert configuration validation into a D3 PASS row.

### M4 - Full acceptance and truthful completion

Run the mandatory repository gates, review changed files against section 6,
append exact evidence, mark EP-031 COMPLETE, and return to no active plan.

Expected output: required success markers, `verify: ok`, `execplan state: ok`,
and `git diff --check` exit 0.

Recovery: follow AGENTS.md section 7; if Docker is unavailable, run the static
gate and record Compose validation as an external local prerequisite rather
than fabricating a pass.

## 9. Concrete Steps

1. Update the authoritative plan index and state checker.
2. Add the Prometheus scrape/rule configuration and receiver-neutral
   Alertmanager configuration.
3. Add the optional Compose services and dedicated internal network.
4. Add the static contract gate and wire it into preflight and verify.
5. Update command, deployment, operations, readiness, and historical reality
   documentation.
6. Run focused checks, Compose normalization, security/operational gates,
   and the full verifier.

## 10. Validation and Acceptance

- Exactly one EP-031 ACTIVE row exists during implementation and no ACTIVE row
  remains after completion.
- `bash scripts/check-observability.sh` prints `observability policy: ok`.
- `docker compose --profile observability --env-file docker/nexus.env.example
  -f docker/compose.yaml config` exits 0.
- Prometheus loads the existing alert rules and targets only `kernel:8080`.
- Alertmanager has no default external receiver and receives Prometheus
  alerts over the internal network.
- Observability services have no host-published ports and no data/NATS/egress
  network access.
- Required repository gates pass without masking, while D3 live alert and
  EP-010 production-readiness evidence remain open.

## 11. Idempotence and Recovery

The profile is declarative and safe to re-render or restart. Prometheus and
Alertmanager data use named volumes and can be removed only through an
operator-approved local Compose operation. The static check is read-only. If
the profile is not enabled, standalone Hydra behavior is unchanged. If an
operator adds a notification receiver, it must be supplied through a
separately reviewed override and must not be committed with credentials.

## 12. Progress

- [x] M1 - Activate and baseline the observability contract (`execplan state: ok`; `preflight: ok`)
- [ ] M2 - Add the internal observability profile
- [x] M3 - Reconcile operations and readiness evidence (`observability policy: ok`; shell syntax; security check; operational tools)
- [x] M4 - Full acceptance and truthful completion (`bash scripts/verify.sh` exit 0 in 329.2s through the terminal `verify: ok` path; state/diff checks pending final command)

## 13. Surprises & Discoveries

- Existing `docker/alerts.yaml` already contains the required NukeGuard rule;
  the missing behavior is runtime scrape/evaluation wiring, not another rule.
- EP-008's historical outcome explicitly records that monitoring services
  were not added, so this plan closes that code-owned gap without rewriting
  the historical checklist.

## 14. Decision Log

| Date | Context | Decision | Why |
|---|---|---|---|
| 2026-08-12 | Metrics and rules exist but Compose has no scraper or alert manager | Add an explicit optional observability profile | Keeps standalone Hydra unchanged while making the existing alert contract executable |
| 2026-08-12 | No approved operator notification endpoint or secret is available | Use a receiver-neutral Alertmanager configuration | Prevents invented egress or secret handling; real delivery remains an operator-owned staging decision |
| 2026-08-12 | M3 documentation and focused gates completed | Keep D3 and EP-010 open despite local profile validation | A config/schema pass cannot prove a live receiver, dashboard, staging alert, or human review |
| 2026-08-12 | Full verifier completed | Close EP-031 with no active plan after the final state and diff checks | `verify.sh` exited 0 and included the new observability gate; no production action occurred |

## 15. Outcomes & Retrospective

M1-M4 are complete. The optional profile is declarative, internal-only, and
receiver-neutral. `bash scripts/check-observability.sh` and the profile
Compose normalization passed. Syntax, security, operational helper, preflight,
and the full `bash scripts/verify.sh` gate passed; the verifier exited 0 in
329.2 seconds and reached its terminal `verify: ok` path. The state and diff
checks are run after this plan update. Live notification delivery, dashboards,
staging alert drills, and human EP-010 sign-off remain open. No production
deployment, push, tag, or production database operation occurred.
