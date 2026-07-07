# OBSERVABILITY.md

## Logging
tracing JSON to stdout. Required fields: ts, level, target, msg, tenant?, envelope_id?, adapter_id?, route?, span ids. Redaction layer: allowlist fields only; secrets/prompt bodies NEVER logged (prompt hashes only: `prefix_sha`, `tail_sha`).

## Metrics (Prometheus, /metrics)
- hydra_http_requests_total{route,code} / latency histogram
- governor_decisions_total{decision} ; governor_eval_seconds
- envelopes_total{state} ; approvals_pending gauge
- bridge_sync_records_total{adapter,op} ; bridge_errors_total{adapter,variant} ; adapter_fuel_used
- llm_calls_total{provider,route,outcome} ; llm_cost_cents_total{provider}
- **tk_prompt_tokens_total{route,kind="hit|miss"}** ; **tk_cache_hit_ratio{route}** (1h rolling) ; **tk_nuke_aborts_total{route}** ; tk_output_bytes histogram ; tk_segment_version{seg}
- events_appended_total ; outbox_lag_seconds

## Traces
Span per request/envelope/sync-cycle; agent loop spans carry route + prefix_sha so cache forensics can correlate ratio drops to exact segment changes.

## Health/uptime
/healthz, /readyz as in OPERATIONS; external uptime ping on / every 60s (staging+prod).

## Dashboards
1. Golden signals (http, errors, latency). 2. Autonomy (envelope funnel by level, approvals age). 3. Bridges (sync throughput, parked adapters). 4. **TOKENKILLER**: hit-ratio per route vs 0.97 line, hit/miss token area, nuke aborts, spend vs constitution cap.

## Alerts
- readyz failing 3m → page
- tk_cache_hit_ratio{route=~"deepseek.*"} < 0.97 for 60m → warn ; < 0.90 for 15m → page
- tk_nuke_aborts_total increase > 5/15m → warn
- llm_cost_cents_total month-to-date > 0.9×cap → warn ; ≥ cap → router auto-degrades to local provider + page
- envelope Failed rate >2% 15m → warn ; bridge parked >30m → warn

## SLIs/SLOs
Availability 99.5% monthly (readyz); shell p95 TTFB <150ms; governor eval p99 <5ms; deepseek routes hit-ratio ≥0.97 (1h).

## Debugging production issues
`jq` filters cookbook in this file's appendix; cache forensics: `scripts/cache-hit-audit.sh --since 1h --route <r>` prints per-call hit/miss with prefix_sha transitions — a ratio cliff at a sha change identifies the offending segment edit.

## Acceptance
EP-008 done when every metric above is scraped, both dashboards JSON committed under docker/dashboards/, alert rules committed, and smoke asserts /metrics contains tk_cache_hit_ratio.
