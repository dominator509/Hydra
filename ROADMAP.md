# ROADMAP.md — Strategic Sequence Only

> Do not implement directly from this file. Implementation must happen through an ExecPlan.

| Phase | Purpose | Depends on | Exit criteria | Specs | ExecPlans |
|---|---|---|---|---|---|
| 0 Discovery & foundation | Confirm greenfield, scaffold workspace, gates, CI | — | verify.sh green on skeleton | SPEC-000 | EP-000, EP-001 |
| 1 Core domain | CDM kinds, Governor, envelope machine (pure) | 0 | unit suite green; governor property tests | SPEC-001 | EP-002 |
| 2 Data & persistence | Postgres schema, store, outbox, event_log, TK ledger tables | 1 | integration suite green vs dockerized PG | SPEC-002 | EP-003 |
| 3 Service layer | fabric REST v1 + MCP server, llm-router, TOKENKILLER, bridge-host+conformance | 2 | contract tests green; cache-hit-audit ≥0.97 on replay | SPEC-003, SPEC-006 | EP-004 |
| 4 Interface | shell (Askama+htmx): pipelines, records, approval queue, bridge tabs, agent console | 3 | e2e suite green | SPEC-004 | EP-005 |
| 5 Auth & security | OAuth2 both roles, sessions, roles, vault, grants, PII gate proof | 4 | security-check green; authz tests | SPEC-005 | EP-006 |
| 6 Testing hardening | conformance soak, failure-mode, regression, flaky policy | 5 | verify green 3× consecutively | — | EP-007 |
| 7 Observability & ops | tracing JSON, metrics, /healthz, alerts, runbooks, TK dashboards | 6 | smoke + obs acceptance | SPEC-007 | EP-008 |
| 8 Deploy & release | Dockerfiles, compose, Caddy, CI/CD, staging, rollback path | 7 | staging deploy + smoke green | — | EP-009 |
| 9 Production readiness | drills (restore, rollback, nuke, cache), reviews, launch gate | 8 | PRODUCTION_READINESS all green | SPEC-008 | EP-010 |

Production readiness milestone = Phase 9 exit = `scripts/production-readiness-check.sh` → `production readiness: ok`.
