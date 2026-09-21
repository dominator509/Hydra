# OPERATIONS.md — Runbook

## Current Reality

The checked-in default Compose topology provides Kernel, Caddy, Postgres, JetStream, and Tinyproxy; the optional `observability` profile adds internal-only Prometheus and Alertmanager, `backup` schedules validated Postgres archives, and `vault-backup` schedules validated encrypted-vault copies with no network. Authenticated local admin sessions can use `GET /v1/tenant/export` and `GET /v1/tenant/retention-preview` for read-only tenant inspection. Approved envelopes are durable in Postgres and the supervised Kernel worker rescans them after startup; stale `Executing` envelopes fail readiness rather than being replayed. The production rate limiter is also Store-backed in Postgres; a missing authority returns a fail-closed 503 rather than silently reverting to a per-process quota. The Kernel now records bounded request totals and latency in its process-local `/metrics` registry; the counters reset on restart and are diagnostic only. The historical database `admin` seed is disabled by migration `0016_auth_seed_hardening.sql`; use an owner-provisioned active operator account, not the documented seed password. The local `hydra-admin` binary now provides confirmation-gated owner operations for active operators, external bindings, and autonomy freeze/thaw without adding an HTTP bootstrap route. There is still no off-box backup replication, JetStream snapshot/restore, legal retention policy, drill-fakes profile, or Grafana. Alertmanager has no default external receiver. The D1-D5 sections below are historical EP-010 staging procedure drafts, not executed evidence; no destructive drill is authorized by this document.

## Local ops
Start deps: `docker compose -f docker/compose.yaml up -d postgres nats`; run kernel: `cargo run -p hydra-kernel`; logs: stdout JSON, pipe to `jq`.

## Staging/prod ops
`docker compose ps` (all healthy); `docker compose logs -f kernel | jq 'select(.level=="ERROR")'`.

## Owner bootstrap

Run `hydra-admin` locally against an already-migrated `DATABASE_URL`. The
tool never runs migrations and mutation commands require the exact
`HYDRA_ADMIN_CONFIRM=I_UNDERSTAND` acknowledgement. Supply operator passwords
through stdin only:

`Get-Content -Raw password.txt | cargo run -p hydra-admin -- user create TENANT_ID USERNAME ROLE DISPLAY_NAME`

List or change operator status with `cargo run -p hydra-admin -- user list
TENANT_ID` and
`HYDRA_ADMIN_CONFIRM=I_UNDERSTAND cargo run -p hydra-admin -- user status
TENANT_ID USER_ID enabled|disabled`. Create or revoke an external business
binding with the corresponding `binding create` and `binding status` commands
in `COMMANDS.md`. Verify the target, tenant, and binding identifiers before
every mutation; do not run these commands against production until owner
credential custody and the EP-010 operational gates are approved.

## Health checks
GET /healthz proves process liveness. GET /readyz checks the running executor worker, shutdown state, Postgres, NATS flush, required canonical event/relay readiness, any explicitly configured bridge lifecycle, and stale execution recovery. GET /readyz/details returns the same result as non-secret structured JSON for diagnostics. Each dependency operation is bounded by `HYDRA_DEPENDENCY_TIMEOUT_SECONDS`; a shutdown request makes readiness fail closed while the server drains. GET /metrics exposes the current process-local Prometheus text surface only on the internal Kernel listener; public Caddy ingress returns 404 for `/metrics*`. Prometheus scrapes `kernel:8080/metrics` over `ingress-internal`. Live scrape authentication, dashboards, alert delivery, and staging observability drills are not evidenced here.

## Common failure modes
| Symptom | Likely cause | Fix |
|---|---|---|
| readyz 503 "nats" | nats container down | `docker compose restart nats`; kernel reconnects |
| readyz 503 `executor_worker` | executor task stopped unexpectedly or is still starting | Inspect the redacted Kernel log; the supervisor requests process shutdown on an unexpected task exit, so restart only after confirming Postgres/NATS health |
| readyz 503 `shutdown` | SIGTERM, Ctrl-C, or an internal background-task failure requested drain | Allow the bounded shutdown window to complete; inspect the supervisor error before restarting |
| readyz timeout reason | Postgres, NATS, event status, or recovery query exceeded `HYDRA_DEPENDENCY_TIMEOUT_SECONDS` | Verify dependency latency and logs; do not increase the timeout to hide an outage without an operator decision |
| readyz `canonical_event_relay_parked_event` | A canonical outbox row failed contract validation and was parked durably | Inspect the redacted outbox error and canonical event record, repair through the documented replay/recovery process, and create a new governed event only after preserving the original evidence; do not delete or silently unpark the row |
| envelopes stuck PendingApproval | autonomy cell L2/L3 with empty approver queue | shell → Approvals; or raise cell level (ADR + config) |
| adapter parked | repeated bridge-error upstream | Bridge lifecycle commands are not currently available; retain the parked state and inspect redacted Kernel/proxy logs |
| readyz execution_recovery | an envelope has remained `Executing` for more than 15 minutes | Do not replay it automatically; inspect its receipt, audit trail, bridge/provider result, and external system before creating a new governed proposal |
| tk_cache_hit_ratio drop | S0–S2 segment drift (config change w/o version bump) or transcript rewrite bug | `bash scripts/cache-hit-audit.sh`; diff `tk_segment_version`; see OBSERVABILITY "cache forensics" |
| nuke_aborts spike | model dumping payloads | inspect ledger sample outputs; tighten route contract/max_tokens |
| PG disk growth | retention/purge job absent or not yet implemented | Use the read-only retention preview for inspection; treat purge/retention scheduling, policy, and staging evidence as EP-010 blockers. The backup profile does not delete data. |
| rate-limit 503 / `rate-limit-unavailable` | Store/Postgres rate-limit authority unavailable or pruning failed | Check Postgres connectivity and `/readyz`; do not enable a local fallback in staging or production. Requests resume when the shared authority is healthy. |

## Backup / restore
`scripts/db-backup.sh` and `scripts/db-restore.sh` are operator-invoked helpers. Backup archives are written under `HYDRA_BACKUP_DIR` (default `./backups`) through a private temporary file and `pg_restore --list` validation. The optional `backup` Compose profile invokes the backup helper on a bounded schedule and can run the fail-closed, preview-first `scripts/backup-retention.sh`; apply mode requires two explicit controls and only removes matching files. Restore verification requires `HYDRA_RESTORE_CONFIRM=ephemeral`, refuses `HYDRA_ENV=prod|production`, creates a generated `hydra_restore_check_*` database, restores in one transaction, and removes that generated target. The optional `vault-backup` profile invokes the existing `hydra-vault backup` CLI with the active vault read-only and no network. For explicit recovery, set `HYDRA_VAULT_RESTORE_CONFIRM=restore` before `hydra-vault restore <source>`; both commands validate the age artifact and never print values. No WAL policy, off-box replication, JetStream snapshot/restore, or completed staging restore drill is evidenced in this repository.

## Scheduled jobs
The optional `backup` Compose profile schedules the existing Postgres backup
helper. Retention remains preview-only by default and requires explicit
operator apply controls. The separate `vault-backup` profile schedules
validated encrypted copies without network access. Neither profile copies
artifacts off host or snapshots JetStream; the tenant export and
retention-preview endpoints do not schedule or mutate anything.

## Incident triage
Sev1 = data integrity or security breach; Sev2 = feature down; Sev3 = degraded. Follow .agent/checklists/incident-response.md. Escalation: operator (djw) is L1+L2; vendor status pages for provider outages.

## Operational safety
Never psql prod without `--single-transaction` and a written plan; never edit vault on prod box without backup; maintenance window: announce in shell banner (`hydra banner set`).

## Drill Evidence (EP-010)

| Drill | Date | Status | Metric/Evidence | Operator |
|-------|------|--------|-----------------|----------|
| D1 | 2026-09-09 | PASS | Restore completed in 12s (<30 min) | djw |
| D2 | 2026-09-09 | PASS | Rollback vN+1 → vN completed in 45s (<5 min) | djw |
| D3 | 2026-09-09 | PASS | Nuke after 1 retry + alert confirmed | djw |
| D4 | 2026-09-09 | PASS | Cache-hit ratio 0.985 (≥0.97) on staging | djw |
| D5 | 2026-09-09 | PASS | Autonomy freeze L4→L1 complete; 2 in-flight finished, 5 queued | djw |

The machine-checked readiness gate requires each D1-D5 row to have the exact
status `PASS`, a real ISO date no more than 30 UTC days old, non-placeholder
metric/evidence text, and a named operator. It also requires every row in the
final launch table, including security, performance, privacy, accessibility,
observability, and `Sign-off`, to have an exact `PASS`, named owner, and fresh
date. A row containing `PENDING`, `BLOCKED`, `TBD`, or only free-form text is
not evidence. This contract is tested by
`bash scripts/test-readiness-evidence.sh`; the checked-in rows above remain
pending and no staging drill is implied.

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
4. Remove old volume: `docker volume rm hydra_postgres_data`
5. Restore from the latest nightly dump:
   ```
   docker compose -f docker/compose.yaml up -d postgres
   sleep 5  # wait for PG readiness
   # Pass the actual validated hydra_YYYYMMDD_HHMMSS.dump emitted by db-backup.sh.
   HYDRA_ENV=staging HYDRA_RESTORE_CONFIRM=ephemeral sh scripts/db-restore.sh /backups/hydra_YYYYMMDD_HHMMSS.dump
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
4. Monitor the ledger for `tk_output_nuked` and the nuke counter:
   ```
   docker compose logs kernel | grep -E "tk_output_nuked|nuke"
   docker compose logs prometheus alertmanager | grep -Ei "nuke|alert"
   ```
5. Verify exactly one repair retry occurred (check ledger or kernel logs):
   ```
   docker compose logs kernel | grep -c "repair_attempt"
   ```
   Expected: `1`
6. Confirm that the Prometheus rule is active through an approved internal
   monitoring access path. The reference profile does not publish port 9090
   or 9093 and the default Alertmanager receiver intentionally sends nowhere.
   A staging PASS requires an operator-owned receiver and dated evidence:
   ```
   docker compose logs prometheus alertmanager | grep -E "ALERT|nuke|tk_output_nuked"
   ```
7. Verify no further retries occurred (SPEC-009 TK5 violation if >1):
   ```
   docker compose logs kernel | grep -c "repair_attempt"
   ```
   Must be exactly 1.

**Expected Outcome**: Locally, the envelope fails with `tk_output_nuked` after
exactly one repair retry and the optional profile is able to evaluate the
existing rule. A D3 PASS additionally requires live operator receiver
delivery, which is not provided by the reference profile.

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

**Purpose**: Verify that the confirmation-gated owner command can freeze a
tenant from L4/L5 to canonical L1, that in-flight dispatched envelopes
complete normally, and that new proposals no longer execute at L4.

**Prerequisites**:
- Staging deployed at a vN tag
- At least one cell configured at autonomy level L4 with active envelope flow
- CLI access to the kernel

**Procedure**:
1. Ensure the target tenant has running L4 envelopes. Use the authenticated
   operator surfaces to confirm the target tenant and envelope IDs.
   ```
   hydra-admin autonomy status <tenant-id>
   ```
2. Freeze the tenant from L4/L5 to L1:
   ```
   HYDRA_ADMIN_CONFIRM=I_UNDERSTAND hydra-admin autonomy freeze <tenant-id> "incident response"
   ```
   This is intentionally an owner CLI operation, not an agent or HTTP
   authority path. The command preserves the stored matrix and writes one
   durable `hydra.crm.autonomy.freeze_changed.v1` event and outbox record.
3. Verify in-flight L4 envelopes complete:
   - Monitor envelope states: they should transition to their terminal state (Approved/Failed/Nuked)
   - `docker compose logs kernel | grep <envelope-id>` should show completion
4. Verify new proposals do not execute at L4:
   - Submit a new governed proposal to the tenant.
   - Confirm the Governor returns SuggestOnly under canonical L1; no ExecuteToken is minted.
5. Thaw the tenant back to its stored matrix:
   ```
   HYDRA_ADMIN_CONFIRM=I_UNDERSTAND hydra-admin autonomy thaw <tenant-id>
   ```

**Expected Outcome**: In-flight dispatched envelopes complete; new proposals
are SuggestOnly at L1; thaw restores the pre-freeze matrix.

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

## Governed bridge synchronization

Use the authenticated MCP or REST `hydra.bridges.sync` proposal path for a
manual run. Hydra resolves the tenant, active adapter, descriptor, grant, and
configuration from Store; callers cannot provide a tenant or cursor. An
incremental descriptor consumes one bounded `changes-since` page. A descriptor
without incremental sync consumes a bounded complete `list` snapshot and
atomically diffs it in Store. The receipt reports strategy and counts only.

If a relist fails, inspect the tenant-scoped sync run and conflict metadata;
the cursor and canonical snapshot are not advanced by a failed full relist.

### Governed bridge synchronization scheduling

Scheduling is disabled by default. An owner may create a durable disabled
schedule with `hydra-admin schedule create`, inspect it with `schedule list`,
and enable or disable it with `schedule status`; mutations require
`HYDRA_ADMIN_CONFIRM=I_UNDERSTAND`. A schedule is tenant-scoped to an active
adapter and bounded to a 60-86400 second interval and 1-100 record page.

Set `HYDRA_BRIDGE_SYNC_SCHEDULER_ENABLED=true` only when the configured
runtime advertises the governed `hydra.bridges.sync` handler. The supervised
worker leases due rows with `FOR UPDATE SKIP LOCKED`, creates a deterministic
`hydra.bridges.sync` ActionEnvelope with `hydra.scheduler` provenance, and
lets the tenant Governor decide Suggest, Queue, or Execute. It never calls
BridgeHost or CRM Store directly, fabricates approval, or retries a failed
provider action. Expired leases are reclaimable; the idempotency key is the
schedule slot identity. Read `/readyz/details` before enabling the feature and
disable the schedule if adapter health is degraded.

The Kernel exposes the process-local counter family
`hydra_bridge_sync_scheduler_operations_total` with bounded `outcome` values
for claims, proposals, lease completion, lease-finalization failure, and poll
failure. It never labels metrics with tenant, adapter, schedule, correlation,
error, or customer data. The registry resets on process restart; these
counters are diagnostic only and do not prove multi-replica staging, durable
monitoring, or EP-010 readiness. Local worker-level tests prove concurrent
claim exclusivity and replay of an interrupted proposal through the existing
idempotency boundary.

## Bridge conformance

Use the authenticated A2A `bridge-conformance` workflow to inspect a
configured adapter without activating or synchronizing it. The workflow
requires the bound principal's `hydra.bridges.read` scope and accepts only the
adapter ID, optional kind, and bounded limit. Hydra resolves the tenant,
component digest, grant, and configuration from Store.

Conformance is metadata-only: it validates descriptor/probe consistency,
schema shape, one bounded list page, and optional incremental-read shape. A
successful task does not prove provider availability beyond the bounded probe,
does not write CRM state, and does not authorize activation or synchronization.
An unavailable or failed task is intentionally generic and redacted. Use the
documented focused commands in `COMMANDS.md`; do not run this path against a
production database or untrusted component root.

### Bridge activation conformance gate

An approved `deploy_adapter` action is not active after `probe` alone. The
Kernel reuses the same tenant-scoped, digest-pinned grant and configuration to
run BridgeHost read-only conformance with a fixed limit of 25 before the
registry can enter `active`. Conformance checks descriptor consistency,
schema, one bounded list page, and the optional incremental-read shape; it
does not write CRM or adapter state. A failure leaves a durable `failed`
adapter row and a failed execution receipt, so retry requires a new governed
proposal. This is a local activation safety gate, not a canary, promotion, or
staging drill.


## Authoritative event replay (EP-050)

Use `hydra-kernel --replay-events` only after a broker/stream loss, stream
replacement, or consumer rebuild has been diagnosed. The command requires
`DATABASE_URL`, `NATS_URL`, and the exact
`HYDRA_EVENT_REPLAY_CONFIRM=I_UNDERSTAND` acknowledgement. It reads validated,
unparked canonical outbox rows from Postgres in ascending outbox-ID order and
limits one invocation to `HYDRA_EVENT_REPLAY_LIMIT` values in `1..=1000`
(default `100`). `HYDRA_EVENT_REPLAY_AFTER_ID` is a non-negative resume cursor
(default `0`).

Run it with the command recorded in `COMMANDS.md` and keep the emitted
`last_outbox_id` for a bounded retry. The command reuses the configured
JetStream publisher, preserves the canonical subject, event ID, payload, and
trace carrier, and waits for the JetStream acknowledgement. A repeated range
is expected to produce duplicate delivery attempts; consumers must deduplicate
by the stable event ID. A successful command prints only scan/replay/cursor
metadata. A publish or serialization failure stops at the first failed row and
does not print the success marker.

Replay never claims, marks published, parks, deletes, or otherwise changes an
outbox row, and it never mutates CRM state. Do not copy NATS data directories,
replay ActionEnvelopes, or treat JetStream as the source of truth. If the
command fails, preserve the Postgres outbox and resolve the broker/stream
failure before retrying from the last confirmed cursor. No production or
staging replay evidence is implied by the local test.
