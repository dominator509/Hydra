# EP-029 Kernel Lifecycle Supervision and Bounded Readiness

Plan status: COMPLETE

## 1. Purpose / Big Picture

Hydra's application boundary must behave predictably under container
termination, background-task failure, and dependency stalls. The current
Kernel listens for Ctrl-C but not the SIGTERM used by Docker and most
orchestrators, waits on relay and executor tasks only after HTTP serving ends,
and does not expose executor-worker liveness through readiness. Readiness
dependency calls also have no bounded operation timeout.

This plan adds a small, portable lifecycle contract. The Kernel will accept
the operating-system termination signal, propagate one shutdown state to all
long-lived tasks, supervise relay and executor termination, bound shutdown
waiting, and fail readiness when a required runtime worker is no longer alive
or a dependency check exceeds its configured deadline. Existing health and
business routes remain compatible.

## 2. Scope

- Add portable Ctrl-C/SIGTERM shutdown coordination.
- Add supervised relay and executor task joining with bounded shutdown.
- Add executor-worker health state and expose it in structured readiness.
- Bound Postgres, NATS, event-status, and recovery dependency checks.
- Bound initial Postgres/NATS connection attempts.
- Add validated environment settings for shutdown and dependency deadlines.
- Add focused unit, runtime, and smoke coverage.
- Update operational, deployment, environment, and command documentation.

## 3. Non-goals

- No database schema or migration changes.
- No change to CRM, Governor, MCP, REST, event, or public entity contracts.
- No automatic replay of ambiguous in-flight executions.
- No new scheduler, retention policy, metrics authentication, or dashboard.
- No production deployment, tag, push, registry publication, or real
  production database operation.
- No new dependency; use the existing Tokio signal, sync, and time features.
- No claim that EP-010 production-readiness evidence has been completed.

## 4. Context and Orientation

`crates/kernel/src/main.rs::run` currently creates relay and executor tasks,
serves Axum, waits only on `tokio::signal::ctrl_c`, and joins workers after
the server future completes. `crates/kernel/src/relay.rs` already owns a
relay health object, while `ExecutorWorker` has no equivalent durable health
signal. `readiness_report` checks Postgres, NATS, event infrastructure, bridge
lifecycle, and stale executions, but each check can wait on an unbounded
database or NATS operation. `Config::validate` is the existing fail-closed
environment boundary.

The implementation must preserve the existing `/healthz` body, the legacy
`/readyz` success/failure status contract, and the non-secret JSON shape of
`/readyz/details` while adding bounded, named checks. A task ending during
normal shutdown is not an incident; a task ending before shutdown requests
must trigger coordinated process shutdown and leave a diagnosable error.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/state/execplan-index.md`
- `crates/kernel/src/main.rs`
- `crates/kernel/src/config.rs`
- `crates/kernel/src/runtime_services.rs`
- `crates/kernel/src/relay.rs`
- `crates/kernel/src/event_status.rs`
- `crates/kernel/src/event_stream.rs`
- `crates/kernel/tests/runtime_wiring.rs`
- `crates/kernel/tests/smoke_healthz.rs`
- `.env.example`
- `docker/compose.yaml`
- `ENVIRONMENT.md`
- `DEPLOYMENT.md`
- `OPERATIONS.md`
- `PRODUCTION_READINESS.md`

## 6. Files to Change

- `.agent/execplans/EP-029-kernel-lifecycle-supervision.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `crates/kernel/src/lib.rs`
- `crates/kernel/src/main.rs`
- `crates/kernel/src/config.rs`
- `crates/kernel/src/runtime_services.rs`
- `crates/kernel/src/supervisor.rs`
- `crates/kernel/tests/runtime_wiring.rs`
- `crates/kernel/tests/smoke_healthz.rs`
- `.env.example`
- `docker/compose.yaml`
- `COMMANDS.md`
- `ENVIRONMENT.md`
- `DEPLOYMENT.md`
- `OPERATIONS.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`

## 7. Interfaces and Contracts

- `HYDRA_SHUTDOWN_TIMEOUT_SECONDS` is a positive integer, defaults to `30`,
  and bounds background-task shutdown joins. Invalid or zero values fail
  configuration validation.
- `HYDRA_DEPENDENCY_TIMEOUT_SECONDS` is a positive integer, defaults to `5`,
  and bounds startup and each individual readiness dependency operation.
  Invalid or zero values fail configuration validation.
- `GET /healthz` continues to return `200` and `ok` when the process is alive.
- `GET /readyz` continues to return `200`/`ok` only when all required checks
  pass and `503` with the existing short failure body otherwise.
- `GET /readyz/details` retains its JSON envelope and adds bounded checks for
  `executor_worker` and `shutdown`; it never returns credentials, URLs with
  secrets, customer data, or stack traces.
- A relay or executor task that exits before coordinated shutdown causes the
  supervisor to request shutdown and returns a non-success process result.
- SIGTERM and Ctrl-C initiate the same idempotent shutdown path. Windows
  builds continue to use Ctrl-C where Unix signal APIs are unavailable.
- Existing tenant isolation, append-only audit, outbox authority, and
  fail-closed behavior remain unchanged.

## 8. Milestones

### M1 - Activate and baseline the lifecycle contract

Goal: make EP-029 the only active plan and establish a clean executable
baseline. Read the state ledger and required source files; add this plan,
its index row, and checker coverage. Run `bash scripts/check-execplan-state.sh`
and `bash scripts/preflight.sh`.

Expected output: `execplan state: ok` and `preflight: ok`.

Recovery: if the checker reports duplicate or missing state, repair only the
index/checker/plan status before Rust edits. Do not alter historical plans.

### M2 - Implement bounded lifecycle and worker supervision

Goal: add configuration parsing, portable shutdown signaling, executor health,
supervised task joins, and bounded connection/readiness operations. Change
only the files listed in section 6. Run `cargo fmt --all` and focused Kernel
tests.

Expected output: focused configuration, supervisor, runtime-wiring, and
readiness tests pass; `cargo test -p hydra-kernel --offline` exits 0.

Recovery: isolate the failing test or `cargo check -p hydra-kernel --tests
--offline`; preserve the existing route contract and revert only the smallest
incorrect lifecycle change with `apply_patch`.

### M3 - Validate container and operational contracts

Goal: prove the new settings are represented consistently in templates and
docs, and that smoke/readiness behavior remains compatible. Run shell syntax,
the smoke test against isolated local dependencies, and the state checker.

Expected output: `smoke test: ok` and `execplan state: ok`; documentation and
Compose checks contain both new settings with the documented defaults.

Recovery: compare `.env.example`, `docker/compose.yaml`, `ENVIRONMENT.md`,
and `COMMANDS.md`; remove any undocumented setting rather than silently
accepting it.

### M4 - Full acceptance and truthful completion

Goal: run the mandatory repository gates, review the changed-file set against
section 6, record exact evidence, and mark EP-029 COMPLETE with no active plan.

Validation commands:

```text
bash scripts/preflight.sh
bash scripts/test-unit.sh
bash scripts/test-integration.sh
bash scripts/test-e2e.sh
bash scripts/security-check.sh
bash scripts/dependency-audit.sh
bash scripts/verify.sh
bash scripts/check-execplan-state.sh
git diff --check
```

Expected output: every required success marker, including `verify: ok`,
`execplan state: ok`, and exit code 0 from `git diff --check`.

Recovery: follow AGENTS.md section 7. A timeout is investigated with a
narrower command before retrying; no failed gate is reclassified as passed.

## 9. Concrete Steps

1. Activate EP-029 in the authoritative index and extend the state checker.
2. Add a small Kernel supervisor module for signal waiting, task outcome
   classification, bounded joins, and idempotent shutdown state.
3. Add executor-worker health shared state and expose it through runtime
   readiness without leaking internal errors.
4. Add validated shutdown/dependency timeout configuration and apply it to
   startup connection attempts and readiness operations.
5. Preserve the existing legacy readiness response while adding structured
   worker/shutdown checks.
6. Add tests for invalid configuration, normal shutdown, unexpected task
   exit, bounded readiness failure, and worker health transitions.
7. Update environment/Compose/operator documentation and record the design
   decision in `DECISIONS.md`.
8. Run all milestone and final gates, then reconcile EP-010 as still partial.

## 10. Validation and Acceptance

- The state checker accepts exactly one active plan during implementation and
  the completed EP-029 state afterward.
- `Config::validate` rejects zero/invalid lifecycle timeout values and applies
  documented defaults.
- SIGTERM handling compiles on Unix and Ctrl-C handling remains available on
  non-Unix targets.
- A failed relay or executor task requests coordinated shutdown and is not
  silently detached.
- Shutdown waits are bounded and timeout paths abort only the owned
  background task handles after recording a redacted diagnostic.
- Executor worker health is false before startup, true while running, and
  false after exit; readiness reports the state without secrets.
- Readiness operations cannot hang beyond the configured per-check deadline.
- Existing health/readiness success and failure response compatibility tests
  pass.
- All required repository gates pass without masking, and `bash
  scripts/verify.sh` prints `verify: ok`.
- `git diff --name-only` is contained in section 6, and no production action
  occurred.

## 11. Idempotence and Recovery

Configuration parsing is pure and repeatable. Sending the shutdown signal
multiple times is harmless because the shared watch state is boolean and the
supervisor treats a repeated shutdown request as the same transition. A
normal rerun after interruption starts at the first unchecked milestone;
focused tests determine whether the prior code change is already present.
No migration or destructive command is part of this plan. If a background
task cannot stop within the configured deadline, the supervisor aborts only
that in-process handle, returns a failure, and leaves operator recovery to
the existing EP-010 procedures.

## 12. Progress

- [x] M1 - Activate and baseline the lifecycle contract (`execplan state: ok`; `preflight: ok`)
- [x] M2 - Implement bounded lifecycle and worker supervision (`cargo check -p hydra-kernel --tests --offline`; library 6/6; binary 29/29)
- [x] M3 - Validate container and operational contracts (`sh -n`; Compose config exit 0; isolated service-backed `smoke test: ok`; `execplan state: ok`)
- [x] M4 - Full acceptance and truthful completion (`bash scripts/verify.sh` exit 0 through terminal `verify: ok`; `git diff --check` exit 0; no active plan)

## 13. Surprises & Discoveries

- Initial discovery: Kernel shutdown currently waits only on `ctrl_c`; the
  container termination signal and background-task health are not integrated
  into the readiness contract.
- `bash scripts/smoke-test.sh` reached the new Kernel and failed closed after
  `HYDRA_DEPENDENCY_TIMEOUT_SECONDS=5` because the Docker-backed local NATS
  service was unavailable. The local PostgreSQL listener on 5432 was also a
  non-responsive Docker backend port; an isolated local Postgres cluster was
  found on `127.0.0.1:55433`, but no NATS listener was present. Compose startup
  with `docker/nexus.env.example` timed out after the Docker backend failed to
  become responsive. This is external test-service evidence, not a code gate.
- Recovery used the isolated loopback PostgreSQL cluster on
  `127.0.0.1:55433` and an existing WSL NATS JetStream listener on
  `localhost:4222`; no Docker service or production database was used. With
  those explicit test endpoints, smoke, integration, and E2E gates passed.
- The first workspace verifier attempt exceeded its 20-minute cold-build
  window while the Docker-backed dependency endpoint was unavailable. After
  service recovery, the authoritative verifier completed in 301.1 seconds
  with exit 0; its terminal path is `verify: ok`.

## 14. Decision Log

| Date | Context | Decision | Why |
|---|---|---|---|
| 2026-08-11 | EP-028 completed and no active plan remained | Select lifecycle supervision as EP-029 | It is code-owned, high-impact for container correctness, and can be validated locally without production credentials or data |
| 2026-08-11 | Tokio already provides signal, sync, and time features | Add no dependency | The smallest reversible implementation preserves the Rust-first dependency boundary |
| 2026-08-11 | Coordinated shutdown can race a worker's final poll | Check the shared watch value before classifying a completed handle as unexpected, then join only the still-owned survivor | Avoids polling a completed `JoinHandle` and keeps normal drain distinct from task failure |
| 2026-08-11 | Service-backed smoke could not start | Keep the new fail-closed timeout behavior and leave M3/M4 open until Postgres and NATS are responsive; do not weaken smoke or make NATS optional | Required health/readiness validation must exercise the real dependency contract |
| 2026-08-11 | Docker-backed dependency ports remained unavailable | Re-run the required gates against the isolated loopback Postgres and WSL NATS JetStream services with explicit test URLs | This preserves real dependency validation without weakening the gate or touching production |

## 15. Outcomes & Retrospective

M1-M4 are complete. The focused Kernel library and binary suites passed
(6/6 and 29/29 respectively), runtime wiring compiled with worker-health
coverage, shell syntax and Compose validation passed, and the recovered
service-backed smoke, integration, and E2E gates passed. The final
`bash scripts/verify.sh` run used explicit isolated PostgreSQL and NATS
endpoints, exited 0 in 301.1 seconds, and reached the script's `verify: ok`
terminal path; `bash scripts/check-execplan-state.sh` and `git diff --check`
also passed. The changed files are contained in this plan's section 6 after
accounting for the pre-existing dirty worktree. EP-010 remains partial for
staging termination/drain and recovery drills, live observability, soak,
security/privacy/accessibility/performance reviews, release evidence, and
human sign-off. No production deployment, push, tag, or production database
operation occurred.
