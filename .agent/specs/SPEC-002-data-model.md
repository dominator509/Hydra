# SPEC-002 Data Model & Persistence
Status: Accepted | Owner: djw | Phase: 2 | ExecPlans: EP-003

## Goal
Durable tenant-isolated CDM with event audit and TK ledger; additive-migration discipline.

## Non-goals
Query API surface (EP-004); analytics warehouse.

## Entities / tables (migration 0001..0006)
entity(id uuid pk, kind text, tenant_id uuid, body jsonb, origin text default 'native', origin_ref text, version bigint, deleted_at timestamptz null, updated_at) — unique(tenant_id,origin,origin_ref) where origin_ref not null; gin(body jsonb_path_ops); btree(tenant_id,kind,deleted_at).
edge(src,rel,dst,body jsonb, pk(src,rel,dst)).
event_log(seq bigserial pk, tenant_id, ts, actor, kind, payload jsonb) — append-only (REVOKE UPDATE/DELETE from app role).
outbox(id, event jsonb, published_at null) same-tx as mutations; relay marks published.
envelope(id uuid pk, tenant_id, state text, doc jsonb, updated_at) + envelope_transition(envelope_id, ts, from, to, actor).
adapter_kv(adapter_id, k, v, pk(adapter_id,k)).
autonomy_cell(tenant_id, domain, action, kind null, level, cfg jsonb, pk(tenant_id,domain,action,coalesce(kind,''))).
tk_ledger(id, ts, tenant_id, route, provider, prefix_sha, hit_tokens int, miss_tokens int, out_tokens int, out_bytes int, aborted bool, cost_cents int).
secret_grant(adapter_id, origins text[], secret_names text[], dsn_name text null, fuel bigint).

## Relationships & integrity
edges reference entities (FK, ON DELETE RESTRICT — soft-delete makes hard cascade moot); envelope.doc validated against governor schema on write; every entity write inserts event+outbox in the same transaction (repository invariant, integration-tested).

## Retention
event_log 180d prune; tk_ledger 365d rollup-then-prune; soft-deleted purge at 30d.

## Migration rules
sqlx, forward-only, additive in v1; each file ends with `-- revert: <manual note>`; destructive ⇒ STOP.

## Error states
StoreError{Conflict(version), NotFound, SchemaViolation, TenantMismatch}.

## Security/data rules
app role lacks UPDATE/DELETE on event_log; all queries take tenant_id first param; sqlx checked macros only.

## Required tests
integration: same-tx outbox invariant (kill between insert? simulate via failing second stmt and assert rollback); version conflict on concurrent upsert; tenant cross-read returns NotFound; event_log immutability (UPDATE fails).

## Acceptance
`bash scripts/test-integration.sh` → `integration tests: ok`; `cargo sqlx migrate run` idempotent on rerun.
