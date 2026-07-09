# 24-Hour Staging Soak (EP-010 M4)

## Purpose
Verify sustained production readiness: cache-hit ratio >= 0.97, zero error spikes, and stable resource usage over a 24-hour period with synthetic tenant agents running.

## Preconditions
- [ ] Staging deployed at a vN tag
- [ ] Synthetic tenant(s) configured with active agent workflows
- [ ] Metrics endpoint accessible (/metrics)
- [ ] Prometheus or equivalent scraping configured (or manual sampling)
- [ ] Alerts configured for: tk_cache_hit_ratio drop, error rate spike, memory growth

## Metric Collection Instructions

### Cache-hit ratio (primary metric)
Sample `/metrics` every hour and extract `tk_cache_hit_ratio`:
```bash
curl -s http://<staging>/metrics | grep "^tk_cache_hit_ratio" | awk '{print $2}'
```
Record each sample in the hourly table below.

### Error rate
Check `/readyz` and aggregate error counters from `/metrics`:
```bash
curl -s http://<staging>/readyz                      # must return 200
curl -s http://<staging>/metrics | grep -E "^http_requests_total.*status=\"[45]" || echo "no 4xx/5xx"
```

### Resource usage
```bash
docker stats --no-stream --format "table {{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}"
```
Sample at hours 0, 6, 12, 18, 24.

### Memory growth check
Compare RSS at hour 0 vs hour 24:
```bash
curl -s http://<staging>/metrics | grep "^process_resident_memory_bytes"
```
Expected: no sustained growth (plateau within 20% of start).

## Expected Thresholds

| Metric | Threshold | Critical if |
|--------|-----------|-------------|
| tk_cache_hit_ratio | >= 0.97 | < 0.97 for any 2 consecutive hourly samples |
| Error rate (5xx) | 0 | Any 5xx response observed |
| Error rate (4xx) | < 1% of requests | > 1% sustained over 1 hour |
| /readyz | 200 | Non-200 at any sample |
| Memory growth | < 20% over 24h | > 20% growth without plateau |
| HTTP p95 latency | < 150ms | > 200ms at any sample |

## Soak Run Log

### Run 1
- **Start**: TBD
- **End**: TBD
- **Staging tag**: TBD
- **Synthetic tenant ID**: TBD
- **Operator**: TBD

#### Hourly cache-hit ratio samples
| Hour | tk_cache_hit_ratio | Error count | /readyz | Notes |
|------|-------------------|-------------|---------|-------|
| 0 | | | | |
| 1 | | | | |
| 2 | | | | |
| 3 | | | | |
| 4 | | | | |
| 5 | | | | |
| 6 | | | | |
| 7 | | | | |
| 8 | | | | |
| 9 | | | | |
| 10 | | | | |
| 11 | | | | |
| 12 | | | | |
| 13 | | | | |
| 14 | | | | |
| 15 | | | | |
| 16 | | | | |
| 17 | | | | |
| 18 | | | | |
| 19 | | | | |
| 20 | | | | |
| 21 | | | | |
| 22 | | | | |
| 23 | | | | |
| 24 | | | | |

#### Resource samples
| Hour | Container | CPU% | Memory |
|------|-----------|------|--------|
| 0 | kernel | | |
| 0 | postgres | | |
| 0 | nats | | |
| 6 | kernel | | |
| 6 | postgres | | |
| 6 | nats | | |
| 12 | kernel | | |
| 12 | postgres | | |
| 12 | nats | | |
| 18 | kernel | | |
| 18 | postgres | | |
| 18 | nats | | |
| 24 | kernel | | |
| 24 | postgres | | |
| 24 | nats | | |

#### Summary
- **Min cache-hit ratio**: 
- **Max error count**: 
- **Readyz failures**: 
- **Memory growth (24h)**: 
- **P95 latency**:
- **Verdict**: [PASS / FAIL / INCONCLUSIVE]

### Run 2 (if needed)
- **Start**: TBD
- **End**: TBD
- **Notes**: TBD

## Verdict

**This soak run**: [PENDING / PASS / FAIL]

Pass if:
1. All cache-hit ratio samples >= 0.97
2. No 5xx errors observed
3. /readyz returned 200 on every check
4. Memory growth < 20%
5. No sustained anomaly for > 1 hour

**Launch-blocking if FAIL**: Remediation ExecPlan required before launch gate sign-off.
