# COMMANDS.md — The Only Allowed Commands

Working directory rule: ALL commands run from the repository root. Scripts refuse to run elsewhere.
Package manager rule: cargo only. npm/pnpm/yarn/pip are forbidden in this repository.

> Coding agents must not invent commands. If a command is missing, update this file first with evidence from the repository (file path + line) in the same commit.

| Purpose | Command | Success signal |
|---|---|---|
| Preflight | `bash scripts/preflight.sh` | `preflight: ok` |
| Install | `bash scripts/install.sh` | `install: ok` |
| Lint | `bash scripts/lint.sh` | `lint: ok` |
| Format check | `bash scripts/format-check.sh` | `format check: ok` |
| Format apply (recovery) | `cargo fmt --all` | exit 0 |
| Typecheck | `bash scripts/typecheck.sh` | `typecheck: ok` |
| Unit tests | `bash scripts/test-unit.sh` | `unit tests: ok` |
| Integration tests | `bash scripts/test-integration.sh` (override with `HYDRA_TEST_DATABASE_URL=postgres://hydra:hydra@127.0.0.1:5432/hydra`; set `NATS_URL` when the disposable NATS listener is not on `127.0.0.1:4222`) | `integration tests: ok` |
| E2E tests | `bash scripts/test-e2e.sh` (set `NATS_URL` when the disposable NATS listener is not on `127.0.0.1:4222`) | `e2e tests: ok` |
| Shell accessibility/degradation contract | `bash scripts/test-shell-accessibility.sh` | `shell accessibility: ok`; semantic landmarks, native disclosures, and native POST fallbacks remain present |
| Deployment helper safety | `bash scripts/test-deployment-safety.sh` | `deployment safety: ok`; digest pinning, strict SSH trust, readiness validation, and truthful promotion dry-run are enforced locally |
| Container image policy | `sh scripts/check-container-image-policy.sh` | `container image policy: ok`; external Compose, CI, and Dockerfile images require immutable digests and Dockerfile-installed packages require exact versions |
| Operational helper and backup-scheduler safety tests | `bash scripts/test-operational-tools.sh` | `operational tools: ok` |
| Backup artifact retention safety | `sh scripts/backup-retention.sh` with `HYDRA_BACKUP_RETENTION_DIR`, a positive count/days limit, and optional explicit apply controls | `backup retention: preview ...` by default; apply requires `HYDRA_BACKUP_RETENTION_APPLY=1` and `HYDRA_BACKUP_RETENTION_CONFIRM=prune` |
| Encrypted-vault backup scheduler | `sh scripts/vault-backup-scheduler.sh` with `HYDRA_VAULT_PATH`, `HYDRA_VAULT_KEY`, and `HYDRA_VAULT_BACKUP_COMMAND` | `vault backup: ok`; scheduler captures helper output and never prints key material |
| Production-readiness evidence contract tests | `bash scripts/test-readiness-evidence.sh` | `readiness evidence: ok`; rejects pending, stale, malformed, duplicate, and placeholder evidence |
| Public ingress policy tests | `bash scripts/test-ingress-policy.sh` | `ingress policy tests: ok`; rejects Caddy configurations that expose Kernel metrics |
| Public ingress policy | `bash scripts/check-ingress-policy.sh` | `ingress policy: ok`; Caddy denies `/metrics*` before the catch-all Kernel proxy |
| Ingress-aware smoke boundary tests | `bash scripts/test-smoke-boundary.sh` | `smoke boundary tests: ok`; public smoke never requests metrics through Caddy |
| Nightly conformance discovery | `bash scripts/test-nightly-conformance.sh` | `nightly conformance: ok` after discovering and passing `c9_soak_10k` |
| Required local performance evidence | `bash scripts/test-performance.sh` (set an isolated loopback `DATABASE_URL` for the cache audit) | `performance: ok` after Governor release p99, named 10k bridge soak, and TOKENKILLER cache checks |
| Release workflow policy | `bash scripts/check-release-policy.sh` | `release policy: ok` |
| Egress policy | `bash scripts/check-egress-policy.sh` | `egress policy: ok` |
| NATS transport policy | `bash scripts/check-nats-policy.sh` | `nats policy: ok` |
| Observability profile policy | `bash scripts/check-observability.sh` | `observability policy: ok` |
| Build | `bash scripts/build.sh` | `build: ok` |
| Security check | `bash scripts/security-check.sh` | `security check: ok` |
| Dependency audit | `bash scripts/dependency-audit.sh` | `dependency audit: ok` |
| Wasmtime feature boundary | `bash scripts/check-wasmtime-features.sh` | `wasmtime feature policy: ok` and no profiling/fxhash path in the locked all-target graph |
| Age vault dependency boundary | `bash scripts/check-age-vault-dependency.sh` | `age vault dependency policy: ok` and no retired age 0.11/proc-macro-error2 path |
| Lockfile refresh (recovery) | `cargo generate-lockfile` | exit 0 |
| Smoke test | `bash scripts/smoke-test.sh` | `smoke test: ok`; the in-repo path creates a unique migrated disposable schema and passes a schema-scoped URL to the real Kernel child |
| Full verification | `bash scripts/verify.sh` | `verify: ok` |
| Build adapters | `bash scripts/build-adapters.sh` | `adapters: ok` |
| Governed bridge lifecycle tests | `cargo test -p bridge-host lifecycle -- --nocapture` and `cargo test -p fabric --test bridge_lifecycle -- --nocapture` | lifecycle and Fabric contract tests pass; Kernel/store tests require loopback Postgres |
| Cache-hit audit (TOKENKILLER) | `bash scripts/cache-hit-audit.sh` | `cache-hit audit: ok (ratio=0.9XX)` |
| Production readiness | `HYDRA_TEST_DATABASE_URL=postgres://hydra:hydra@127.0.0.1:55433/hydra NATS_URL=nats://[::1]:4222 bash scripts/production-readiness-check.sh` (explicit disposable loopback database and broker required) | `production-readiness:ok` |
| ExecPlan state validation | `bash scripts/check-execplan-state.sh` | `execplan state: ok` |
| Nexus MCP contract | `cargo test -p fabric mcp_contract -- --nocapture` | 4 MCP contract tests pass |
| BridgeEngineer synthesis contract | `cargo test -p agents bridge_engineer --offline -- --nocapture` and `cargo test -p hydra-kernel --test runtime_wiring configured_provider_constructs_experimental_bridge_synthesis_runtime --offline -- --nocapture` | bounded TOKENKILLER mapping tests pass; configured runtime reports Experimental without provider I/O |
| A2A bridge-synthesis contract | `cargo test -p fabric --test a2a_workflows bridge_synthesis_is_authenticated_proposal_only_and_idempotent -- --nocapture` (set `DATABASE_URL` to an isolated loopback test Postgres) | authenticated proposal-only task completes and an equivalent retry returns the same task/artifact |
| Tenant-scoped bridge state | `cargo test -p store --test adapter_kv --offline -- --nocapture` and `cargo test -p bridge-host --test store_kv --test lifecycle --offline -- --nocapture` (set `DATABASE_URL` to an isolated loopback test Postgres) | equal adapter IDs remain isolated by tenant; legacy unscoped KV methods fail closed |
| Governed bridge synchronization | `cargo test -p store --test bridge_sync --offline -- --nocapture`, `cargo test -p bridge-host --test lifecycle --offline -- --nocapture`, `cargo test -p hydra-kernel --test bridge_lifecycle --offline -- --nocapture`, and `cargo test -p fabric --test bridge_lifecycle --offline -- --nocapture` (set `DATABASE_URL` to an isolated loopback test Postgres) | transactional cursor/conflict, WIT incremental/full-relist, typed handler, and authenticated REST proposal tests pass |
| Governed bridge scheduling | `cargo test -p store --test bridge_schedules --offline -- --nocapture`, `cargo test -p fabric --test scheduled_proposals --offline -- --nocapture`, and `cargo test -p hydra-kernel --test bridge_scheduler --offline -- --nocapture` (set `DATABASE_URL` to an isolated loopback test Postgres for Store-backed suites) | tenant-scoped leases, governed scheduler provenance/idempotency, and deterministic slot identity pass |
| Governed bridge conformance | `cargo test -p bridge-host --lib conformance_rejects --offline -- --nocapture`, `cargo test -p bridge-host --test lifecycle conformance --offline -- --nocapture`, `cargo test -p hydra-kernel --test bridge_lifecycle --offline -- --nocapture`, and `cargo test -p fabric --test a2a_workflows conformance --offline -- --nocapture` (set `DATABASE_URL` to an isolated loopback test Postgres) | bounded page validation, metadata-only adapter conformance, tenant-safe Kernel service, and authenticated durable A2A workflow tests pass |
| Tenant data contract tests | `cargo test -p store --test tenant_data -- --nocapture` and `cargo test -p fabric --test tenant_data -- --nocapture` (set `DATABASE_URL` to an isolated loopback test Postgres) | Store and authenticated REST tenant-data suites pass |
| Auth seed hardening tests | `cargo test -p fabric --test auth_seed_hardening -- --nocapture` (set `DATABASE_URL` to an isolated loopback test Postgres) | Historical database seed is rejected; active operator credentials remain usable |
| Session token storage tests | `cargo test -p store --test sessions -- --nocapture` (set `DATABASE_URL` to an isolated loopback test Postgres) | New bearer tokens are hashed at rest and legacy plaintext rows upgrade on lookup |
| Owner operator lifecycle tests | `cargo test -p store --test operators --offline -- --nocapture` (set `DATABASE_URL` to an isolated loopback test Postgres) | Operator creation/list/status, session revocation, seed protection, and binding status tests pass |
| Owner bootstrap CLI tests | `cargo test -p hydra-admin --offline -- --nocapture` | Four deterministic parsing and secret-input tests pass |
| Owner bootstrap CLI | `HYDRA_ADMIN_CONFIRM=I_UNDERSTAND cargo run -p hydra-admin -- user create TENANT_ID USERNAME ROLE DISPLAY_NAME < password.txt` | Active operator is created through Store; password is read from stdin and never printed |
| External binding operations | `HYDRA_ADMIN_CONFIRM=I_UNDERSTAND cargo run -p hydra-admin -- binding create PROVIDER EXTERNAL_TENANT EXTERNAL_BUSINESS HYDRA_TENANT_ID` or `binding status BINDING_ID active\|disabled\|revoked` | Binding is created or status-changed through Store without deleting the record |
| Autonomy freeze operations | `HYDRA_ADMIN_CONFIRM=I_UNDERSTAND cargo run -p hydra-admin -- autonomy freeze TENANT_ID REASON`, `autonomy thaw TENANT_ID`, or `autonomy status TENANT_ID` | Freeze/thaw is tenant-scoped, durable, idempotent, and status output contains no secrets |
| Bridge schedule operations | `HYDRA_ADMIN_CONFIRM=I_UNDERSTAND cargo run -p hydra-admin -- schedule create TENANT_ID ADAPTER_ID KIND INTERVAL_SECONDS PAGE_LIMIT`, `schedule list TENANT_ID`, or `schedule status TENANT_ID SCHEDULE_ID enabled\|disabled` | Owner-confirmed schedule mutations remain tenant-scoped and soft-disabled; list/status output is bounded and non-secret |
| Durable execution recovery tests | `cargo test -p store --test execution_recovery -- --nocapture` and `cargo test -p hydra-kernel --test runtime_wiring -- --nocapture` (set `DATABASE_URL` to an isolated loopback test Postgres) | Approved envelopes recover after worker construction; stale in-flight work is counted and not replayed |
| Distributed rate-limit tests | `cargo test -p store --test rate_limits --offline -- --nocapture` and `cargo test -p fabric --lib rate --offline -- --nocapture` (set `DATABASE_URL` to an isolated loopback test Postgres for Store tests) | Atomic window, digest isolation, pruning, 429, and fail-closed 503 tests pass |
| Runtime metrics tests | `cargo test -p hydra-kernel --lib metrics --offline -- --nocapture` | Kernel metrics registry and bounded request/scheduler-label tests pass; the binary wrapper must not silently discover zero tests |
| Kernel lifecycle tests | `cargo test -p hydra-kernel --lib --offline -- --nocapture` | Configuration, readiness, worker health, and supervisor tests pass |
| Local dev (stateful services) | `docker compose -f docker/compose.yaml up -d postgres nats` | containers healthy |
| Local dev (kernel+shell) | `cargo run -p hydra-kernel` | `hydra: listening on :8080` log line |
| Local DB setup | `bash scripts/db-setup.sh` | `db setup: ok` |
| Migrations | `cargo sqlx migrate run` (after EP-003) | `Applied N migrations` |
| Vault provisioning and recovery | `cargo run -p hydra-vault -- set <name>`, `get-names`, `rotate`, `backup <destination>`, or `restore <source>` with `HYDRA_VAULT_KEY`; restore additionally requires `HYDRA_VAULT_RESTORE_CONFIRM=restore` | encrypted age file updated or copied after validation; values are read from stdin or remain encrypted and are never printed; backup refuses an existing destination |
| Authoritative event replay | `DATABASE_URL="$DATABASE_URL" NATS_URL="$NATS_URL" HYDRA_EVENT_REPLAY_CONFIRM=I_UNDERSTAND HYDRA_EVENT_REPLAY_AFTER_ID=0 HYDRA_EVENT_REPLAY_LIMIT=100 cargo run -p hydra-kernel -- --replay-events` | `event replay: ok scanned=N replayed=N last_outbox_id=N`; bounded Postgres-to-JetStream recovery with no outbox mutation |
| Refresh checked SQLx metadata | `cargo sqlx prepare --workspace -- --all-targets` | exit 0 and `.sqlx/` updated |
| Docker image validation | `docker build -f docker/Dockerfile -t hydra/kernel:local .` and `docker build -f docker/egress-proxy.Dockerfile -t hydra/egress-proxy:local .` | both versioned image builds exit 0 |
| Compose validation | `docker compose -f docker/compose.yaml config` | normalized standalone config and exit 0; requires configured `POSTGRES_PASSWORD` and `HYDRA_VAULT_KEY` |
| Nexus Compose validation | `docker compose --env-file docker/nexus.env.example -f docker/compose.yaml config` | normalized Nexus-connected config and exit 0 |
| Observability Compose validation | `docker compose --profile observability --env-file docker/nexus.env.example -f docker/compose.yaml config` | normalized internal Prometheus/Alertmanager profile and exit 0 |
| Backup Compose validation | `docker compose --profile backup --env-file docker/nexus.env.example -f docker/compose.yaml config` | normalized data-only scheduled-backup profile and exit 0 |
| Vault-backup Compose validation | `docker compose --profile vault-backup --env-file docker/nexus.env.example -f docker/compose.yaml config` | normalized network-isolated encrypted-vault scheduler profile and exit 0 |
| Single crate check (diagnostic) | `cargo check -p <crate>` | exit 0 |
| Single test (diagnostic) | `cargo test -p <crate> <name> -- --nocapture` | exit 0 |
| Dependency tree (diagnostic) | `cargo tree -p <crate>` | resolved dependency tree and exit 0 |

Underlying tool expectations (installed by scripts/install.sh): rustup toolchain 1.79+, `cargo fmt`, `cargo clippy`, `cargo audit`, `cargo deny`, `cargo sqlx` (sqlx-cli), `wasm-tools`, `docker compose`, `jq`, `curl`, `rg`.

## Forbidden commands
- Anything with `sudo` outside scripts/install.sh's documented tool installs.
- `git push --force`, history rewrites.
- `DROP DATABASE`, `TRUNCATE`, raw `psql` against non-test DBs.
- `rm -rf` outside `target/`, `/tmp`, or explicitly listed build dirs.
- Any npm/npx/node invocation.
- `curl | sh` style pipe-installs not listed in scripts/install.sh.

## Recovery instructions
- Script fails → read its stderr; each script names the failing sub-step. Apply AGENTS.md §7 bounded retry.
- format diffs after `bash scripts/format-check.sh` → `cargo fmt --all`, then rerun `bash scripts/format-check.sh`.
- stale Cargo.lock after dependency-feature pruning → `cargo generate-lockfile`, then rerun the security/dependency gates.
- sqlx compile-time query errors → `bash scripts/db-setup.sh && cargo sqlx prepare --workspace -- --all-targets`.
- bridge adapter build fails before compilation starts → `rustup target add wasm32-wasip2`, then rerun `bash scripts/build-adapters.sh`.
- Wasmtime/adapter build fails → `wasm-tools validate adapters/<name>.wasm` for a narrower diagnostic.
- docker services unhealthy → `docker compose logs --tail=50 postgres nats`.
