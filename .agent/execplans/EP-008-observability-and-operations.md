# EP-008 Observability & Operations

## 1. Purpose / Big Picture
Make HYDRA operable: structured JSON logging with redaction, the full Prometheus metric set (including the TOKENKILLER dashboard series), health endpoints hardened, alert rules as code, dashboards committed, backup/restore scripts, and runbooks verified — SPEC-007 acceptance.

## 2. Scope
tracing subscriber config + redaction layer in kernel, metrics registry + instrumentation across crates via `metrics()` hooks, docker/alerts.yaml, docker/dashboards/{golden,tokenkiller}.json, scripts/db-backup.sh + scripts/db-restore.sh, cron compose service (purge, prune, rollup, backup), OPERATIONS.md appendix (jq cookbook), smoke-test.sh metric assertions.

## 3. Non-goals
No external APM/OTLP exporter (post-v1); no Grafana provisioning automation beyond committed JSON; no log shipping (stdout only; operator attaches collector); no new product features.

## 4. Context and Orientation
OBSERVABILITY.md is the normative metric/alert list; SPEC-007 binds acceptance. Instrumentation points already exist as span boundaries from earlier plans — this plan adds the metrics layer and the redaction layer, then proves them via smoke assertions.

## 5. Files to Read First
OBSERVABILITY.md, SPEC-007, OPERATIONS.md, crates/kernel/src/main.rs (subscriber init), crates/tokenkiller/src/ledger.rs (ratio source), scripts/smoke-test.sh.

## 6. Files to Change
crates/kernel/src/{telemetry.rs,main.rs}, crates/{fabric,governor-adjacent instrumentation in kernel,store,bridge-host,llm-router,tokenkiller,agents}/src/** (metric hooks only — no behavior change), docker/alerts.yaml, docker/dashboards/golden.json, docker/dashboards/tokenkiller.json, docker/compose.yaml (cron service + prometheus optional profile), scripts/db-backup.sh (new), scripts/db-restore.sh (new), scripts/smoke-test.sh (metric assertions), OPERATIONS.md (jq appendix + drill-evidence table), OBSERVABILITY.md (only if a metric name must change — Decision Log).

## 7. Interfaces and Contracts
telemetry::init(env) -> sets JSON subscriber with allowlist redaction layer (field names: password, secret, token, api_key, prompt, tail dropped; prefix_sha/tail_sha allowed). Metric names EXACTLY as OBSERVABILITY.md — smoke greps these strings; renames are breaking. /readyz JSON: {pg:bool,nats:bool,vault:bool,adapters:bool}. db-backup.sh: pg_dump -Fc to /backups/hydra-<utc>.dump + prints `backup: ok`; db-restore.sh <file>: pg_restore into fresh DB + prints `restore: ok`.

## 8. Milestones
M1 Telemetry init + redaction layer. Goal: JSON logs with required fields; secrets provably dropped. Read: telemetry docs in OBSERVABILITY.md. Change: kernel/src/telemetry.rs + main.rs. Exact edits: tracing-subscriber json fmt, EnvFilter from RUST_LOG, custom Layer filtering fields by allowlist. Validation: `cargo test -p hydra-kernel redaction_` → `m1: ok`. Expected: `m1: ok` (test logs a secret-shaped field, asserts absent in captured output). Recovery: capture via tracing-subscriber::fmt::TestWriter; if layer ordering eats fields, put redaction BEFORE fmt layer.
M2 Metrics registry + instrumentation. Goal: every OBSERVABILITY series emitted. Change: kernel metrics registry (metrics + metrics-exporter-prometheus crates — Decision Log dependency entry), hooks in fabric middleware, executor, store outbox relay, bridge sync loop, router, TK session/ledger. Validation: `cargo test -p hydra-kernel metrics_surface` (boots app, scrapes /metrics, asserts 6 series names incl. tk_cache_hit_ratio) → `m2: ok`. Recovery: missing series ⇒ the hook site is the gap; grep OBSERVABILITY name back to owning crate.
M3 Alerts + dashboards as code. Change: docker/alerts.yaml (exact rules from OBSERVABILITY.md incl. ratio <0.97 60m warn / <0.90 15m page, nuke spike, cost cap), two dashboard JSONs (panels: hit-ratio vs 0.97 line, hit/miss area, nuke aborts, spend vs cap; golden: rate/errors/latency). Validation: `python3 -c "import yaml,json;yaml.safe_load(open('docker/alerts.yaml'));json.load(open('docker/dashboards/golden.json'));json.load(open('docker/dashboards/tokenkiller.json'))" && echo m3: ok`. Expected: `m3: ok`. Recovery: yaml/json parse errors name line numbers.
M4 Backup/restore + cron jobs. Change: the two scripts + compose cron service (supercronic or alpine cron image) entries: purge 03:00, events_prune 03:20, ledger rollup hourly, backup 02:00. Validation: `bash scripts/db-backup.sh && bash scripts/db-restore.sh $(ls -t /backups/*.dump | head -1)` → `backup: ok` then `restore: ok` (against dev compose PG; restore into hydra_restore_check DB, not the live one). Expected: both ok lines. Recovery: pg_restore role errors → --no-owner flag; NEVER point restore at the live DB name (STOP-adjacent; script hardcodes hydra_restore_check).
M5 Smoke + runbook closure. Change: smoke-test.sh adds /metrics assertions + readyz JSON shape; OPERATIONS.md jq appendix + empty drill-evidence table (EP-010 fills). Validation: `bash scripts/smoke-test.sh` → `smoke test: ok`; `bash scripts/verify.sh` → `verify: ok`. Recovery: metric absent at smoke ⇒ M2 hook missed a startup path — check the series requires traffic and smoke generates one request first.

## 9. Concrete Steps
Milestones in order; commit per milestone `EP-008: M<n> <slug>`; docker compose up -d postgres nats before M4.

## 10. Validation and Acceptance
verify.sh green; smoke asserts 6 metric series; alerts/dashboards parse; backup+restore ok lines; redaction test present; diff ⊆ §6; SPEC-007 acceptance rows all demonstrable.

## 11. Idempotence and Recovery
Backup script timestamped (safe rerun); restore targets scratch DB; metric registration idempotent (describe once in registry init). Resume = rerun failing milestone validation.

## 12. Progress
- [x] M1 - [x] M2 - [x] M3 - [x] M4 - [x] M5

## 13. Surprises & Discoveries
## 14. Decision Log
(metrics/metrics-exporter-prometheus dependency addition goes here + DECISIONS.md)
## 15. Outcomes & Retrospective
