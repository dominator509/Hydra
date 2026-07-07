# OPERATIONS.md — Runbook

## Local ops
Start deps: `docker compose -f docker/compose.yaml up -d postgres nats`; run kernel: `cargo run -p hydra-kernel`; logs: stdout JSON, pipe to `jq`.

## Staging/prod ops
`docker compose ps` (all healthy); `docker compose logs -f kernel | jq 'select(.level=="ERROR")'`.

## Health checks
GET /healthz (process live) ; /readyz (PG ping, NATS ping, vault loaded, adapters loaded) ; /metrics (Prometheus).

## Common failure modes
| Symptom | Likely cause | Fix |
|---|---|---|
| readyz 503 "nats" | nats container down | `docker compose restart nats`; kernel reconnects |
| envelopes stuck PendingApproval | autonomy cell L2/L3 with empty approver queue | shell → Approvals; or raise cell level (ADR + config) |
| adapter parked | repeated bridge-error upstream | `hydra bridge status <id>`; check egress proxy logs; resume: `hydra bridge resume <id>` |
| tk_cache_hit_ratio drop | S0–S2 segment drift (config change w/o version bump) or transcript rewrite bug | `bash scripts/cache-hit-audit.sh`; diff `tk_segment_version`; see OBSERVABILITY "cache forensics" |
| nuke_aborts spike | model dumping payloads | inspect ledger sample outputs; tighten route contract/max_tokens |
| PG disk growth | event_log unpruned | verify retention job `events_prune` ran (cron container) |

## Backup / restore
Nightly `scripts/db-backup.sh` → pg_dump + WAL to /backups (off-box rsync). Restore drill (quarterly + EP-010): fresh volume → `scripts/db-restore.sh <dump>` → smoke green.

## Scheduled jobs
purge soft-deleted >30d (daily 03:00), events_prune to 180d (daily), token-ledger rollup hourly, backup nightly — all as compose `cron` service entries.

## Incident triage
Sev1 = data integrity or security breach; Sev2 = feature down; Sev3 = degraded. Follow .agent/checklists/incident-response.md. Escalation: operator (djw) is L1+L2; vendor status pages for provider outages.

## Operational safety
Never psql prod without `--single-transaction` and a written plan; never edit vault on prod box without backup; maintenance window: announce in shell banner (`hydra banner set`).
