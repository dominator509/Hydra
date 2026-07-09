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

## Drill Evidence (EP-010)

| Drill | Date | Status | Metric/Evidence | Operator |
|-------|------|--------|-----------------|----------|
| D1 | TBD | PENDING | Restore from backup < 30 min | - |
| D2 | TBD | PENDING | Rollback vN+1 → vN < 5 min | - |
| D3 | TBD | PENDING | Nuke spike → alert → single repair | - |
| D4 | TBD | PENDING | Cache-hit ≥ 0.97 on staging | - |
| D5 | TBD | PENDING | Autonomy freeze L4→L1 completes | - |

## Drill Procedures

### D1 — Restore Drill

**Purpose**: Verify database restore from nightly backup completes within 30 minutes.

**Prerequisites**:
- Staging instance deployed at a vN tag
- Docker compose environment with Postgres volume
- Valid nightly backup dump (`scripts/db-backup.sh` has been run)
- Restore script: `scripts/db-restore.sh`

**Procedure**:
1. Record start time: `DR1_START=$(date -u +%s)`
2. Stop the staging kernel: `docker compose -f docker/compose.yaml stop kernel`
3. Drop the Postgres container and volume: `docker compose -f docker/compose.yaml rm -sfv postgres`
4. Remove old volume: `docker volume rm hydra_postgres_data || true`
5. Restore from the latest nightly dump:
   ```
   docker compose -f docker/compose.yaml up -d postgres
   sleep 5  # wait for PG readiness
   sh scripts/db-restore.sh /backups/hydra-$(date -u +%Y-%m-%d).dump
   ```
6. Start kernel: `docker compose -f docker/compose.yaml up -d kernel`
7. Wait for readyz: `curl -fsS http://localhost:8080/readyz`
8. Run smoke: `bash scripts/smoke-test.sh`
9. Record end time: `DR1_END=$(date -u +%s)`
10. Compute duration: `DR1_RTO=$(( DR1_END - DR1_START ))`

**Expected Outcome**: Smoke green; total RTO ≤ 1800 seconds (30 minutes).

**Evidence Log Entry**: `| D1 | <ISO date> | PASS | Restore completed in <N>s (<30 min) | <operator> |`

**Failure Recovery**: Restore failure is launch-blocking. File a remediation ExecPlan immediately.

### D2 — Rollback Drill

**Purpose**: Verify that deploying vN+1 then rolling back to vN completes in under 5 minutes with no data loss.

**Prerequisites**:
- Staging instance at tag vN (`git checkout vN`)
- Next version tag vN+1 built and available as a Docker image
- `ROLLBACK.md` procedure current

**Procedure**:
1. Record deploy-vN+1 start time: `DR2_START=$(date -u +%s)`
2. Deploy vN+1: `docker compose -f docker/compose.yaml up -d kernel`
3. Verify vN+1 healthy: `curl -fsS http://localhost:8080/readyz`
4. Run smoke on vN+1: `bash scripts/smoke-test.sh`
5. Initiate rollback to vN per `ROLLBACK.md`:
   ```
   docker compose -f docker/compose.yaml down kernel
   docker tag hydra-kernel:vN+1 hydra-kernel:vN-rollback
   docker tag hydra-kernel:vN hydra-kernel:vN+1  # restore previous tag
   docker compose -f docker/compose.yaml up -d kernel
   ```
6. Verify vN healthy: `curl -fsS http://localhost:8080/readyz`
7. Run smoke on vN: `bash scripts/smoke-test.sh`
8. Record end time: `DR2_END=$(date -u +%s)`
9. Compute duration: `DR2_RTO=$(( DR2_END - DR2_START ))`

**Expected Outcome**: Rollback smoke green; total time ≤ 300 seconds (5 minutes).

**Evidence Log Entry**: `| D2 | <ISO date> | PASS | Rollback vN+1→vN completed in <N>s (<5 min) | <operator> |`

**Failure Recovery**: Rollback failure is launch-blocking. File a remediation ExecPlan immediately.

### D3 — Nuke Drill

**Purpose**: Verify that a 1MB dump from a fake provider triggers tk_output_nuked with exactly one repair retry, and an alert fires documenting the event.

**Prerequisites**:
- Staging deployed at a vN tag
- Compose profile `drill-fakes` available with a dump-fake provider route
- Alerting infrastructure configured (docker/alerts.yaml)

**Procedure**:
1. Start the dump-fake provider: `docker compose --profile drill-fakes up -d fake-provider`
2. Configure a staging route pointing at the fake provider
3. Trigger an envelope through the route
4. Monitor the ledger for `tk_output_nuked`:
   ```
   curl -s http://localhost:8080/metrics | grep tk_output_nuked
   ```
5. Verify exactly one repair retry occurred (check ledger or kernel logs):
   ```
   docker compose logs kernel | grep -c "repair_attempt"
   ```
   Expected: `1`
6. Check the alerts endpoint or log for the nuke alert:
   ```
   docker compose logs kernel | grep -E "ALERT|nuke|tk_output_nuked"
   ```
7. Verify no further retries occurred (SPEC-009 TK5 violation if >1):
   ```
   docker compose logs kernel | grep -c "repair_attempt"
   ```
   Must be exactly 1.

**Expected Outcome**: Envelope fails with tk_output_nuked after exactly one repair retry; alert fired.

**Evidence Log Entry**: `| D3 | <ISO date> | PASS | Nuke after 1 retry + alert confirmed | <operator> |`

**Failure Recovery**: Two retries observed = SPEC-009 TK5 violation. File a regression test + fix (≤5-line rule or follow-up plan).

### D4 — Cache Drill

**Purpose**: Verify replay corpus cache-hit ratio ≥ 0.97 against staging. If DEEPSEEK_API_KEY is present, also run a 20-call live sample. Then bump an S1 segment version and confirm the ratio dip is visible via prefix_sha forensics.

**Prerequisites**:
- Staging deployed at a vN tag
- Replay corpus present: `crates/tokenkiller/tests/replay_corpus.rs`
- Bulk of corpus runs against deepseek fake (included in drill-fakes profile)
- Optional: `DEEPSEEK_API_KEY` environment variable set for live sample

**Procedure**:
1. Run the corpus replay against staging:
   ```
   bash scripts/cache-hit-audit.sh
   ```
2. Capture the ratio: note `tk-corpus ratio: 0.xxxx`
3. If `DEEPSEEK_API_KEY` is set, also run a 20-call live sample:
   ```
   DEEPSEEK_API_KEY="$DEEPSEEK_API_KEY" cargo test -p tokenkiller --test replay_corpus -- --nocapture --live-sample 20
   ```
4. Bump one S1 segment version to simulate config drift:
   - Edit segment version in the relevant config or test fixture
   - Re-run cache-hit-audit.sh and verify ratio dips below 0.97
5. Use prefix_sha forensics to attribute the dip:
   ```
   curl -s http://localhost:8080/metrics | grep tk_cache_prefix_sha
   ```
6. Revert the segment version bump
7. Re-run `bash scripts/cache-hit-audit.sh` to confirm return to ≥ 0.97

**Expected Outcome**: Cache-hit ratio ≥ 0.97; segment dip visible and attributed via prefix_sha.

**Evidence Log Entry**: `| D4 | <ISO date> | PASS | Cache-hit ratio 0.xxx (≥0.97) on staging | <operator> |`

**Failure Recovery**: Ratio < 0.97 on staging but OK in CI => diff staging `tk_segment_version` vs repo — config drift is the usual culprit.

### D5 — Autonomy Freeze Drill

**Purpose**: Verify that an Auditor agent (or CLI command) can drop a cell from L4 to L1, that in-flight L4 envelopes complete normally, and that new envelopes queue instead of being processed at L4.

**Prerequisites**:
- Staging deployed at a vN tag
- At least one cell configured at autonomy level L4 with active envelope flow
- CLI access to the kernel

**Procedure**:
1. Ensure the target cell has running L4 envelopes. Use the shell to confirm:
   ```
   hydra cell status <cell-name>  # should show level: L4, active envelopes: >= 1
   ```
2. Freeze the cell from L4 to L1:
   ```
   hydra cell freeze <cell-name> L1
   ```
   Or via Auditor agent: dispatch an autonomy-freeze command targeting the cell.
3. Verify in-flight L4 envelopes complete:
   - Monitor envelope states: they should transition to their terminal state (Approved/Failed/Nuked)
   - `docker compose logs kernel | grep <envelope-id>` should show completion
4. Verify new envelopes queue:
   - Submit a new envelope to the cell
   - Check queue depth: `hydra cell queue <cell-name>` should show ≥ 1 queued
   - Confirm no L4 processing: the envelope should not transition out of Queued
5. Thaw the cell back to L4 (or appropriate level):
   ```
   hydra cell thaw <cell-name> L4
   ```

**Expected Outcome**: In-flight envelopes complete; new envelopes queue; no L4 processing after freeze.

**Evidence Log Entry**: `| D5 | <ISO date> | PASS | Autonomy freeze L4→L1 complete; N in-flight finished, M queued | <operator> |`

**Failure Recovery**: If in-flight envelopes are lost or new envelopes process at L4 after freeze, this is a critical autonomy-safety defect. File a remediation ExecPlan immediately.

---

## Appendix: `jq` Cookbook for Log Analysis

All kernel logs are emitted as JSON lines with fields: `ts`, `level`, `target`, `message`, plus optional context fields (`tenant`, `envelope_id`, `adapter_id`, `route`, span ids).  Use `jq` to filter, aggregate, and investigate.

### Filter by level

```bash
# Show only ERROR-level events
docker compose logs kernel | jq 'select(.level == "ERROR")'

# Show WARN and above
docker compose logs kernel | jq 'select(.level == "ERROR" or .level == "WARN")'
```

### Extract specific fields

```bash
# Show timestamp, level, and message (TSV)
docker compose logs kernel | jq -r '[.ts, .level, .message] | @tsv'

# JSON array of just those fields
docker compose logs kernel | jq '{ts, level, message}'
```

### Filter by route or service

```bash
# All deepseek route events
docker compose logs kernel | jq 'select(.route | contains("deepseek"))'

# Envelope-specific events
docker compose logs kernel | jq 'select(.envelope_id != null)'
```

### Time-range analysis

```bash
# Events in the last 5 minutes (if ts is RFC3339)
docker compose logs kernel | jq --arg cut "$(date -u -d '-5 min' +%Y-%m-%dT%H:%M:%SZ)" 'select(.ts >= $cut)'

# Events per minute (crude histogram)
docker compose logs kernel | jq -r '.ts[:16]' | sort | uniq -c | sort -rn
```

### Redaction verification

```bash
# Ensure no secrets leak into logs (should return nothing)
docker compose logs kernel | jq 'select(.secret != null or .password != null or .token != null or .api_key != null or .prompt != null)'
# Expected: no output (all such fields are masked as "***")
```

### Cache forensic queries

```bash
# Find all cache-related events with prefix_sha
docker compose logs kernel | jq 'select(.prefix_sha != null) | {ts, route, prefix_sha, message}'

# Check for tail_sha transitions (indicates segment drift)
docker compose logs kernel | jq 'select(.tail_sha != null) | [.ts, .route, .tail_sha] | @tsv' | sort
```

### Error rate estimation

```bash
# Count ERROR vs total lines as a primitive error ratio
TOTAL=$(docker compose logs kernel | jq -c '.' | wc -l)
ERRORS=$(docker compose logs kernel | jq 'select(.level == "ERROR")' | wc -l)
echo "error ratio: $(echo "scale=4; $ERRORS / $TOTAL" | bc)"
```

### Nuke event tracking

```bash
# Find all nuke-related events
docker compose logs kernel | jq 'select(.message | test("nuke|abort|tk_output"; "i"))'

# Count nuke aborts per route
docker compose logs kernel | jq -r 'select(.level == "WARN" and .message | test("nuke")) | .route // "unknown"' | sort | uniq -c | sort -rn
```

### Dashboard metric debugging

```bash
# Check raw metric values (requires running kernel)
curl -s http://localhost:8080/metrics | grep -E "^#|^hydra_"

# Extract a specific gauge
curl -s http://localhost:8080/metrics | grep "^hydra_tk_cache_hit_ratio " | awk '{print $2}'
```

### Structured log to CSV

```bash
# Export a CSV of key fields for spreadsheet analysis
docker compose logs kernel | jq -r '[.ts, .level, .message, (.route // ""), (.envelope_id // "")] | @csv' > hydra-log-export.csv
```
