# EP-023 Tenant Export and Retention Preview

Plan status: COMPLETE

## 1. Purpose / Big Picture

Close the next code-owned EP-010 privacy/data gap without inventing a legal retention policy or performing destructive cleanup. Hydra needs a Store-owned, tenant-scoped export projection and a deterministic retention preview so operators can inspect export/purge impact before a separately authorized retention scheduler exists.

## 2. Scope

- Add a typed Store export projection for canonical CRM entities, edges, and the append-only event/audit history belonging to one Hydra tenant.
- Add a deterministic, non-destructive retention preview that reports soft-deleted entity candidates and aged event/outbox/ledger records without deleting anything.
- Add authenticated admin-only local REST endpoints for export and retention preview, with tenant authority derived from the verified session.
- Add focused tenant-isolation, soft-delete, pagination/limit, and serialization tests.
- Update commands, architecture, security, operations, readiness, and decision records to describe the implemented boundary and its limits.

## 3. Non-goals

- No hard delete, purge, truncation, destructive migration, automatic scheduler, cron service, or production database operation.
- No invented retention duration, legal conclusion, or customer-facing export format beyond a versioned deterministic JSON document.
- No external Nexus mutation path; Nexus v1 remains read/propose/governed execution through its existing seam.
- No second CRM abstraction, raw SQL outside Store, Node/npm toolchain, or new dependency.
- No claim that a retention policy, backup cadence, staging drill, privacy review, or human sign-off is complete.

## 4. Context and Orientation

`PRODUCTION_READINESS.md` identifies export and retention/purge implementation as an open gate, while `OPERATIONS.md` states that no scheduler exists. `crates/store` already owns tenant-scoped entity reads, soft-delete semantics, append-only event history, outbox state, and the TOKENKILLER ledger. The new read-only projection must preserve those boundaries and must not expose secrets, prompts, or raw credentials.

## 5. Files to Read First

`AGENTS.md`; `COMMANDS.md`; `.agent/PLANS.md`; `.agent/EXECUTION_RULES.md`; `.agent/state/execplan-index.md`; `ARCHITECTURE.md`; `SECURITY.md`; `TESTING.md`; `PRODUCTION_READINESS.md`; `OPERATIONS.md`; `DECISIONS.md`; `crates/store/src/lib.rs`; `crates/store/src/entities.rs`; `crates/store/src/events.rs`; `crates/store/src/outbox.rs`; `crates/store/src/ledger.rs`; `crates/fabric/src/rest/mod.rs`; `crates/fabric/src/rest/entities.rs`; `crates/fabric/src/auth/session.rs`; `crates/fabric/src/services.rs`; `migrations/0001_entity_and_edge.sql`; `migrations/0002_event_log_and_outbox.sql`; `migrations/0005_tk_ledger.sql`.

## 6. Files to Change

- `.agent/execplans/EP-023-tenant-export-and-retention-preview.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `crates/store/src/lib.rs`
- `crates/store/src/tenant_data.rs`
- `crates/store/tests/tenant_data.rs`
- `crates/fabric/src/rest/mod.rs`
- `crates/fabric/src/rest/openapi.rs`
- `crates/fabric/src/rest/tenant_data.rs`
- `crates/fabric/src/services.rs`
- `crates/fabric/src/lib.rs`
- `crates/fabric/tests/tenant_data.rs`
- `crates/fabric/tests/integration_contracts.rs`
- `crates/kernel/src/main.rs`
- `.sqlx/` (generated checked-query metadata)
- `COMMANDS.md`
- `ARCHITECTURE.md`
- `SECURITY.md`
- `OPERATIONS.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`
- `NEXUS_INTEGRATION_AUDIT.md` (append current verification only)

## 7. Interfaces and Contracts

- `GET /v1/tenant/export` returns a deterministic versioned JSON export for the authenticated local session tenant. It requires `Admin`; it never accepts a tenant ID from the query, path, body, or header as authority.
- `GET /v1/tenant/retention-preview` returns counts and oldest/newest timestamps for soft-deleted entities and aged append-only operational records. It is read-only and requires `Admin`.
- The export includes canonical entity records, relationship edges, and append-only event records for the bound tenant. Outbox delivery metadata and TOKENKILLER prompt/response content are not exported as customer data; only safe aggregate metadata may appear in the preview.
- Responses include `schema_version`, `tenant_id`, and typed arrays/counts. Ordering is stable by primary key or timestamp plus primary key.
- Store SQL is parameterized and tenant-scoped. Cross-tenant requests return authorization failure or an empty tenant projection without existence disclosure.
- Soft-deleted entities remain represented in the export with `deleted_at` state where the Store projection exposes it; no code path deletes them.
- Retention preview reports candidates only. No retention duration is selected by this plan, and no preview endpoint mutates state.

## 8. Milestones

### M1 - Activate plan and reconcile state

Add EP-023 to the authoritative index, extend the state checker, and record the EP-022-to-EP-023 transition.

Validation: `bash scripts/check-execplan-state.sh`

Expected: `execplan state: ok` with EP-023 as the only `ACTIVE` plan.

Recovery: repair plan/index/checker agreement before Rust edits.

### M2 - Implement Store-owned tenant projection

Add typed export/preview structures and parameterized Store queries with deterministic ordering, bounded limits, and tenant filters. Add integration tests using the isolated test database proving same-tenant export, cross-tenant denial, soft-delete visibility, and preview non-mutation.

Validation: `cargo test -p store --test tenant_data -- --nocapture`

Expected: all named tenant-data tests pass.

Recovery: narrow to the failing Store test and inspect SQL/query metadata; do not bypass tenant predicates or use destructive SQL.

### M3 - Add authenticated local REST surface

Add admin-only routes using the existing verified `AuthCtx`, with no caller-selected tenant authority. Add Fabric tests for role denial, header mismatch denial, deterministic response shape, and bounded request parameters.

Validation: `cargo test -p fabric --test tenant_data -- --nocapture`

Expected: all named REST boundary tests pass.

Recovery: fix route/auth ownership in Fabric; do not add endpoint-local tenant parsing.

### M4 - Documentation and mandatory gate wiring

Update commands and operating/security/readiness documentation, add ADR-0033, and extend the state checker and required test gate if appropriate. State clearly that preview is not purge and no scheduler exists.

Validation: `bash scripts/check-execplan-state.sh` and `bash scripts/preflight.sh`

Expected: `execplan state: ok` and `preflight: ok`.

Recovery: correct the owning documentation or checker assertion; never mark staging evidence from local tests.

### M5 - Full local acceptance and truthful reconciliation

Run focused Store/Fabric tests, checked SQL metadata validation, formatting/lint/security/dependency gates, the isolated integration and E2E suites, and the full verifier. Update this plan and the index only after evidence is complete.

Validation: explicit isolated `bash scripts/verify.sh`

Expected: terminal `verify: ok`; no destructive operation, staging deployment, tag, push, or production database access.

Recovery: follow AGENTS.md section 7 and preserve any incomplete external evidence as open.

## 9. Concrete Steps

1. Activate EP-023 and validate the plan-state ledger.
2. Implement Store projection types and tenant-scoped queries.
3. Add Store integration tests and refresh checked SQL metadata.
4. Add admin-only Fabric routes and boundary tests.
5. Update documentation and ADR-0033.
6. Run the focused and full gates with isolated services.
7. Reconcile EP-023 status and EP-010 evidence without claiming production readiness.

## 10. Validation and Acceptance

- EP-023 is the only active plan and the state checker passes.
- Tenant export and retention preview use Store-owned parameterized SQL and verified session tenancy.
- Admin-only authorization, deterministic JSON shape, stable ordering, bounded results, cross-tenant denial, and soft-delete semantics are tested.
- The preview is read-only and no scheduler/purge policy is claimed.
- `bash scripts/preflight.sh` and the full verifier pass with explicit isolated services.
- EP-010 remains partial; no production or staging evidence is fabricated.

## 11. Idempotence and Recovery

All endpoints are read-only and rerunnable. Export serialization is deterministic for the same committed database state. No migration is required. If interrupted, resume the first unchecked milestone and preserve unrelated worktree changes.

## 12. Progress

- [x] M1 - EP-023 active and plan-state ledger reconciled (`bash scripts/check-execplan-state.sh` -> `execplan state: ok`).
- [x] M2 - Store-owned tenant export and retention preview implemented and tested (`cargo test -p store --test tenant_data -- --nocapture` -> 3 passed; checked SQLx metadata refreshed).
- [x] M3 - Authenticated admin-only REST surface implemented and tested (`cargo test -p fabric --test tenant_data -- --nocapture` -> 1 passed).
- [x] M4 - Documentation, ADR, OpenAPI contract, and mandatory state/test wiring updated (`bash scripts/check-execplan-state.sh` -> `execplan state: ok`; `bash scripts/preflight.sh` -> `preflight: ok`).
- [x] M5 - Full local acceptance and truthful reconciliation recorded (`bash scripts/verify.sh` -> exit 0 through the terminal `verify: ok` path in 805.9s with isolated loopback Postgres/NATS).

## 13. Surprises & Discoveries

- 2026-08-11: The first route test used the development bearer and correctly exposed that dev-only identity intentionally accepts a caller-selected tenant. The production-like test now uses a persisted tenant-bound admin session, so cross-tenant header substitution exercises the normal fail-closed session path.
- 2026-08-11: The HTTP response DTOs initially derived only `Serialize`, which was sufficient for handlers but not for typed contract tests. Adding `Deserialize` keeps the versioned response types usable by deterministic clients without changing the wire shape.
- 2026-08-11: No migration was needed; all Store queries are additive reads over existing tenant-scoped tables, so checked SQLx metadata was refreshed without changing schema history.
- 2026-08-11: The full verifier completed in 805.9 seconds with the isolated `DATABASE_URL` and `NATS_URL`; no staging, external provider, production database, deployment, tag, push, or destructive operation was used.

## 14. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-11 | Implement export and retention preview before purge scheduling | This closes a code-owned privacy/data visibility gap while avoiding an invented legal retention period or destructive operation. |
| 2026-08-11 | Use persisted local admin sessions in the REST boundary test | Development identity intentionally permits arbitrary tenant selection only in `HYDRA_ENV=dev`; the endpoint contract must also be proven against the normal session-bound path. |

## 15. Outcomes & Retrospective

Complete. EP-023 adds `TenantDataRepo` with the `hydra.tenant-data.v1` export contract, deterministic 10,000-record bounds, same-tenant entity/edge/event projection, soft-delete visibility, and non-destructive retention metrics. Fabric exposes `/v1/tenant/export` and `/v1/tenant/retention-preview` behind the existing verified local session middleware and `Admin` role; the production Kernel wires `StoreTenantDataService` instead of the unavailable placeholder. OpenAPI lists both routes.

The Store suite passed 3 tests, the Fabric tenant-data suite passed 1 loopback HTTP test, the existing Fabric OpenAPI integration contract passed, checked SQLx metadata refreshed, `bash scripts/check-execplan-state.sh` printed `execplan state: ok`, `bash scripts/preflight.sh` printed `preflight: ok`, and the explicit isolated full verifier exited 0 through `verify: ok` in 805.9 seconds. EP-010 remains partial: no purge, scheduler, legal retention policy, staging privacy/export demonstration, restore drill, or human sign-off was performed.
