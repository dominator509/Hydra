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
migrations/000{1..6}_*.sql, crates/store/{Cargo.toml,src/{lib.rs,entities.rs,edges.rs,events.rs,envelopes.rs,ledger.rs,adapter_kv.rs,autonomy.rs,testkit.rs},tests/integration_*.rs}, crates/kernel/{Cargo.toml,src/{main.rs,relay.rs,config.rs},tests/smoke_healthz.rs}, scripts/{db-setup.sh,install.sh}, .sqlx/ (prepared), Cargo.toml, Cargo.lock, COMMANDS.md, DECISIONS.md, README.md, .gitignore, REPO_BRIEF.md, .serena/project.yml, vendor/sqlx/**, vendor/sqlx-macros-core/**.

## 7. Interfaces and Contracts
`Store::new(pool)`; `entities.upsert(tenant, Entity) -> Result<Entity,StoreError>` (same-tx event+outbox), `.get/.list(kind,cursor)/.soft_delete`; `events.append`; `envelopes.save/transition/list(state)`; `ledger.record(LedgerRow)` + `ledger.route_ratio(route, window)`; `autonomy.matrix(tenant) -> PolicyMatrix`. Outbox relay: SELECT ... FOR UPDATE SKIP LOCKED batch 100 → publish `hydra.events.<tenant>` → mark published; at-least-once documented.

## 8. Milestones
M1 Migrations apply. Edits: six migration files exactly per SPEC-002 incl. REVOKE on event_log and unique/gin indexes; db-setup.sh: `cargo sqlx database create && cargo sqlx migrate run` + ok line. Validation: `bash scripts/db-setup.sh && cargo sqlx migrate run` (2nd run idempotent) → `db setup: ok`; with sqlx-cli 0.8.6 the no-op rerun exits 0 silently instead of printing `Applied 0 migrations`. Recovery: syntax errors name the file; fix forward, never edit an applied migration (create 0007 fix if already applied — Decision Log).
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
- [x] M1 - [x] M2 - [x] M3 - [x] M4 - [x] M5

## 13. Surprises & Discoveries
- 2026-07-07 - The documented recovery path for missing SQLx tooling surfaced two Windows/Git-Bash wrinkles before any SQL failed: `scripts/install.sh` was using `command -v cargo-sqlx`/`cargo-deny`, and Git Bash on this host did not see freshly installed cargo subcommands until `$HOME/.cargo/bin` was added to PATH. Fixing the repo scripts was enough to unblock EP-003 without touching global user state.
- 2026-07-07 - `sqlx-cli` 0.8.6 does not print `Applied 0 migrations` on an idempotent rerun here; it exits 0 silently once all migrations are current. The plan now records that observed behavior so the next agent does not misread the empty stdout as a failure.
- 2026-07-07 - `cargo sqlx prepare --workspace` rebuilt more of the workspace than just `store`, which is expected because the workspace-level metadata pass typechecks every crate before writing `.sqlx/`. The pass still completed successfully and produced the root query snapshot the later verify gates need.
- 2026-07-07 - `governor::ActionEnvelope` stores history timestamps as RFC3339 strings, while `envelope_transition.ts` is a real `timestamptz`. Casting through `($2::text)::timestamptz` in the insert query was the smallest way to keep the persisted row and the serialized governor doc aligned without inventing a second transition timestamp source.
- 2026-07-07 - The M4 DeepSeek usage-field side quest stays documentation-only for now: this repo still has no live `.env` and no `DEEPSEEK_API_KEY`, so the authoritative move in this turn was to implement the `tk_ledger` storage/query surface and explicitly defer the live twin-request probe to EP-010 D4.
- 2026-07-07 - The first M5 compile failure was not a schema bug: `cargo sqlx prepare --workspace` needs `DATABASE_URL` exported while it compiles the checked macros. Running prepare immediately after `db-setup` without that env left both `store` and `kernel` queries unresolved even though the database itself was ready.
- 2026-07-07 - `verify.sh` exposed one more SQLx nuance after the code was green: the workspace snapshot also has to cover integration-test macros, so the durable recovery command here is `cargo sqlx prepare --workspace -- --all-targets`, not just the default target set.
- 2026-07-07 - `/readyz` itself was healthy; the timeout came from the smoke harness. Manual `curl` against a logged `cargo run -p hydra-kernel` session returned `200 OK` for both `/healthz` and `/readyz`, and swapping the test client's `shutdown()` call for `flush()` made the scripted probe deterministic on this Windows host.
- 2026-07-07 - Trimming `async-nats` to `default-features = false, features = ["ring"]` was enough to clear the new `cargo deny` license rejection and keep the kernel's plain NATS connect/publish/flush path intact. The remaining verify blocker is narrower: `cargo audit` still reports `RUSTSEC-2023-0071` through `sqlx` 0.8.6's optional `sqlx-mysql` lockfile metadata even though `cargo tree --target all -i sqlx-mysql` shows it is not in Hydra's active graph.
- 2026-07-07 - The `cargo audit` finding was coming from SQLx's optional backend metadata, not Hydra's active Postgres dependency graph. `cargo tree --target all -i rsa` stayed empty while `Cargo.lock` still carried `sqlx-mysql`, so the smallest repo-local recovery was to vendor a Postgres-only `sqlx` facade and trim `sqlx-macros-core` until the lockfile stopped advertising MySQL and SQLite baggage Hydra never enables.
- 2026-07-07 - The trimmed `vendor/sqlx-macros-core` copy still emits upstream `unexpected_cfgs` warnings for `mysql`, `_sqlite`, and macro-only cfg names when the repo builds with `-W unexpected-cfgs`. The warnings are noisy but non-fatal; `verify.sh` still finished green, so EP-003 records the warning debt instead of stretching into a vendor-cleanup refactor.
## 14. Decision Log
- 2026-07-07 - Updated `scripts/install.sh` even though it is outside §6. Reason: EP-003's documented recovery path (`bash scripts/install.sh`) was itself failing on this Windows host because cargo subcommands were detected with `command -v` instead of the actual `cargo <subcommand>` surface the repo uses elsewhere. Smallest reversible fix to keep the repo's own recovery path truthful.
- 2026-07-07 - Added an append-only trigger to `event_log` in addition to the required `REVOKE UPDATE/DELETE`. Smallest reversible way to make SPEC-002's immutability guarantee testable under the local `hydra` role, which owns the table and would otherwise bypass a privilege-only guard.
- 2026-07-07 - Used a generated `kind_key` column on `autonomy_cell` so PostgreSQL can enforce the spec's `pk(tenant_id,domain,action,coalesce(kind,''))` behavior directly. Smallest reversible way to preserve nullable `kind` while still getting a concrete primary-key target for upserts later in the plan.
- 2026-07-07 - Added `sqlx` to the workspace and generated `.sqlx/` metadata during M2 rather than deferring all offline-query work to the end of EP-003. Smallest reversible choice because SPEC-002 requires checked macros, and the repo's normal `verify.sh` / `typecheck` surface cannot rely on a live `DATABASE_URL`.
- 2026-07-07 - Implemented M2 around public repo structs (`Store`, `EntitiesRepo`, `EventsRepo`, `EdgesRepo`) plus a schema-isolated `TestDb` harness, while leaving envelopes/ledger/autonomy as thin placeholders until their own milestones. Smallest reversible path that proves the entity/event/outbox invariants without pre-baking later-plan behavior.
- 2026-07-07 - Added `governor` as a path dependency of `store` in M3 so the envelope repo can persist and transition the real `ActionEnvelope` type instead of a shadow persistence-only copy. Smallest reversible choice that preserves EP-002's already-verified state machine semantics at the storage boundary.
- 2026-07-07 - Stored the full serialized envelope doc in `envelope.doc` and mirrored only the hot-listing fields (`tenant_id`, `state`, `updated_at`) into dedicated columns. Smallest reversible way to satisfy SPEC-002's table shape while keeping later layers free to deserialize the exact governor envelope without column-per-field duplication.
- 2026-07-07 - Added `LedgerRow`, `LedgerRepo::record`, `route_ratio`, and `month_to_date_cents` in M4 instead of waiting for the TOKENKILLER crate to exist. Smallest reversible path because SPEC-002 assigns the persistence contract to `store`, and the reference ledger math already defines the fields later crates will share.
- 2026-07-07 - Recorded the DeepSeek usage-field probe as deferred-to-EP-010 D4 because `DEEPSEEK_API_KEY` is absent in the current local environment (`preflight` still reports no `.env`). Smallest truthful choice that preserves the M4 schema contract without faking a live-provider validation the repo cannot currently perform.
- 2026-07-07 - Cloned the kernel shutdown watch sender into the graceful-shutdown future instead of moving the original sender. Smallest reversible fix to satisfy Rust's async ownership rules while preserving the post-serve shutdown signal path for the relay task.
- 2026-07-07 - Added ADR-0011 in `DECISIONS.md` for the `sqlx` and `async-nats` dependency set because AGENTS.md §8 requires a durable dependency record before merge.
- 2026-07-07 - Updated `COMMANDS.md` after `verify.sh` proved the earlier SQLx recovery note was incomplete for this repo. Smallest truthful fix was to point recovery at `cargo sqlx prepare --workspace -- --all-targets`, because checked macros also exist in the `store` integration tests.
- 2026-07-07 - Disabled `sqlx` default features once `cargo audit` showed `rsa` entering through `sqlx-mysql`. Smallest reversible hardening step because Hydra only uses the Postgres runtime/macros/migrate surface.
- 2026-07-07 - Disabled `async-nats` default features and kept only `ring` so the kernel stops pulling the optional websocket/root-bundle stack that `cargo deny` rejected on license grounds. Smallest reversible path because Hydra's M5 relay only needs direct NATS TCP connect/publish/flush, not websockets or JetStream helpers.
- 2026-07-07 - Added explicit `version = "0.1.0"` alongside the `store` crate's local `cdm` and `governor` path dependencies after `cargo deny` flagged them as wildcard dependencies. Smallest reversible change that satisfied the repo's deny policy without changing any workspace resolution.
- 2026-07-07 - Added `cargo generate-lockfile` to `COMMANDS.md` as the documented lockfile-refresh recovery after dependency-feature pruning. This became necessary once `cargo audit` and `cargo deny` were both reading the lockfile more strictly than the active `cargo tree` graph.
- 2026-07-07 - Updated `README.md`, `.gitignore`, `REPO_BRIEF.md`, and `.serena/project.yml` even though they are outside the original code-only file list. Reason: the user explicitly asked for repo-level README/gitignore work plus Serena optimization, and these were the smallest durable changes that improved onboarding without altering runtime behavior.
- 2026-07-07 - Replaced `TcpStream::shutdown()` with `flush()` in `crates/kernel/tests/smoke_healthz.rs` after a manual `cargo run -p hydra-kernel` plus `curl` proved `/readyz` was healthy. Smallest reversible fix was to correct the test harness instead of loosening the readiness contract.
- 2026-07-07 - Added `vendor/sqlx` and `vendor/sqlx-macros-core` even though they were outside the earlier file list. Reason: `cargo audit` was still failing on `RUSTSEC-2023-0071` through SQLx 0.8.6's optional MySQL metadata with no upstream fixed release to upgrade to, and a repo-local Postgres-only vendor patch was the smallest reversible way to keep the checked-macro surface while restoring a green `verify.sh`.
- 2026-07-07 - Kept the vendored `sqlx-macros-core` warning-only even though `-W unexpected-cfgs` now prints upstream cfg noise. Smallest truthful choice for EP-003 was to ship the audit-clean patch once `verify.sh` was green and record the warning debt for a later upstream sync instead of expanding scope into vendor-internal lint surgery.
## 15. Outcomes & Retrospective
- EP-003 now lands the full persistence seam: forward-only migrations, schema-isolated `store` repos/tests, kernel Postgres+NATS readiness, outbox relay wiring, and committed `.sqlx/` metadata.
- The most useful recovery move was narrowing failures aggressively: SQLx macro errors were solved by re-running `prepare` with `DATABASE_URL`, and the `/readyz` timeout only became obvious as a smoke-harness issue after manual endpoint probes against a logged kernel session.
- Live DeepSeek usage probing remains the only intentionally deferred slice. The repo still lacks a real `.env` and `DEEPSEEK_API_KEY`, so that validation stays documented as an external follow-up rather than being faked locally.
- Final repo-contract validation is green: `bash scripts/verify.sh` now prints `verify: ok`, including `cargo audit` and `cargo deny`, after constraining SQLx to the Postgres-only surface Hydra actually ships.
- Remaining technical debt in this slice is limited and explicit: `vendor/sqlx-macros-core` still emits `unexpected_cfgs` warnings under the repo's lint flags, and the repo should drop the vendor patch set once upstream SQLx ships an audit-clean release for the same Postgres feature surface.
