# EP-003 Data & Persistence

## 1. Purpose / Big Picture
Implement SPEC-002: migrations, store repositories with the outbox invariant, event_log immutability, envelope persistence, TK ledger table, adapter_kv — Postgres becomes the durable spine.

## 2. Scope
migrations/0001..0006, crates/store (repos+testkit), scripts/db-setup.sh real, kernel wiring for pool + outbox relay task, integration tests.

## 3. Non-goals
No HTTP surface; no NATS publish beyond the relay task skeleton (subjects only, consumed later); no schema for auth (EP-006) or observability extras.

## 4. Context and Orientation
SPEC-002 lists exact tables. sqlx offline mode: run `cargo sqlx prepare --workspace` after queries compile. DeepSeek usage-field probe (ASSUMPTION A3) happens here as M4 side-quest because ledger columns depend on it — offline fake is authoritative for CI.

## 5. Files to Read First
SPEC-002, ARCHITECTURE persistence boundaries, crates/store/src/lib.rs, ENVIRONMENT.md (DATABASE_URL), reference/tokenkiller/ledger.rs (columns).

## 6. Files to Change
migrations/000{1..6}_*.sql, crates/store/src/{lib.rs,entities.rs,edges.rs,events.rs,envelopes.rs,ledger.rs,adapter_kv.rs,autonomy.rs,testkit.rs}, crates/store/tests/integration_*.rs, crates/kernel/src/{main.rs,relay.rs,config.rs}, scripts/db-setup.sh, .sqlx/ (prepared), Cargo.toml (sqlx, tokio, async-nats).

## 7. Interfaces and Contracts
`Store::new(pool)`; `entities.upsert(tenant, Entity) -> Result<Entity,StoreError>` (same-tx event+outbox), `.get/.list(kind,cursor)/.soft_delete`; `events.append`; `envelopes.save/transition/list(state)`; `ledger.record(LedgerRow)` + `ledger.route_ratio(route, window)`; `autonomy.matrix(tenant) -> PolicyMatrix`. Outbox relay: SELECT ... FOR UPDATE SKIP LOCKED batch 100 → publish `hydra.events.<tenant>` → mark published; at-least-once documented.

## 8. Milestones
M1 Migrations apply. Edits: six migration files exactly per SPEC-002 incl. REVOKE on event_log and unique/gin indexes; db-setup.sh: `cargo sqlx database create && cargo sqlx migrate run` + ok line. Validation: `bash scripts/db-setup.sh && cargo sqlx migrate run` (2nd run idempotent) → `db setup: ok` + `Applied 0 migrations`. Recovery: syntax errors name the file; fix forward, never edit an applied migration (create 0007 fix if already applied — Decision Log).
M2 Entity/edge/event repos + outbox invariant. Validation: `cargo test -p store --test integration_entities` → `m2: ok` echo. Expected: `m2: ok`. Recovery: sqlx macro errors → `cargo sqlx prepare -p store`.
M3 Envelope + autonomy repos. Validation: `cargo test -p store --test integration_envelopes` → `m3: ok`. Recovery: version-conflict test uses two pooled conns; ensure test pool size ≥2.
M4 TK ledger + usage-probe note. Edits: ledger repo + route_ratio SQL (sum hit/(hit+miss) filtered window); document in Decision Log the live-probe procedure (twin identical requests to DeepSeek if key present; else mark deferred-to-EP-010 D4). Validation: `cargo test -p store --test integration_ledger` → `m4: ok`. Recovery: window math off-by-one → fixture with fixed timestamps.
M5 Kernel wiring + relay. Edits: config::validate full table; pool init; relay tokio task with graceful shutdown; /readyz now checks PG+NATS ping. Validation: `bash scripts/test-integration.sh` → `integration tests: ok` AND `bash scripts/smoke-test.sh` (readyz asserted). Recovery: NATS absent locally → compose up first (COMMANDS local dev row).

## 9. Concrete Steps
Milestone order; `docker compose up -d postgres nats` before M1.

## 10. Validation and Acceptance
integration suite green; event_log UPDATE rejected test present; verify.sh green; diff ⊆ §6.

## 11. Idempotence and Recovery
Migrations forward-only; tests per-schema (testkit creates `hydra_test_<rand>` schema, drops after); resume = rerun failing integration test file.

## 12. Progress
- [ ] M1 - [ ] M2 - [ ] M3 - [ ] M4 - [ ] M5

## 13. Surprises & Discoveries
## 14. Decision Log
## 15. Outcomes & Retrospective
