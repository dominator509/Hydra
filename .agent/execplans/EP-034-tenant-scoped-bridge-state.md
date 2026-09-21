# EP-034 Tenant-Scoped Bridge State

Plan status: COMPLETE

## 1. Purpose / Big Picture

Remove a latent tenant-isolation defect in bridge scratch state before any
future synchronization work. Adapter IDs are unique only within a Hydra
tenant, while the historical `adapter_kv` table is global by adapter ID. This
plan adds a tenant-scoped Store boundary, wires BridgeHost lifecycle probes
and governed kernel execution through it, and keeps the old API fail-closed.

## 2. Scope

- Add the normative SPEC-014 tenant-scoped bridge-state contract.
- Add an additive `(tenant_id, adapter_id, key)` Store table.
- Add tenant-aware Store and BridgeHost KV implementations.
- Pass governed envelope tenant identity through the lifecycle probe path.
- Add isolation, lifecycle, kernel, security, and full-gate validation.
- Reconcile architecture, security, operations, and historical readiness
  documentation without claiming synchronization or production readiness.

## 3. Non-goals

- CRM synchronization or provider polling.
- Mapping synthesis or bridge promotion.
- New bridge deployment capabilities.
- Reinterpreting or bulk-migrating historical unscoped KV rows.
- Nexus protocol changes unrelated to this isolation boundary.
- Production deployment or production database changes.
- New external dependencies, provider SDKs, Node/npm tooling, or Wasmtime
  ABI changes.

## 4. Context and Orientation

`bridge_adapter` identity is tenant-scoped, but migration 0004 created
`adapter_kv` with only `(adapter_id, k)` as its key. The current Store repo is
used by BridgeHost through `StoreKvStore`, and the kernel supplies lifecycle
requests from governed envelopes. Equal adapter IDs can therefore collide
unless tenant identity is carried to the SQL boundary. Historical rows cannot
be assigned safely because they contain no tenant authority.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/state/execplan-index.md`
- `ARCHITECTURE.md`
- `SECURITY.md`
- `ENVIRONMENT.md`
- `TESTING.md`
- `migrations/0004_adapter_kv_and_autonomy.sql`
- `crates/store/src/adapter_kv.rs`
- `crates/bridge-host/src/host.rs`
- `crates/bridge-host/src/lifecycle.rs`
- `crates/kernel/src/bridge_runtime.rs`

## 6. Files to Change

- `.agent/specs/SPEC-014-tenant-scoped-bridge-state.md`
- `.agent/execplans/EP-034-tenant-scoped-bridge-state.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `migrations/0019_tenant_adapter_kv.sql`
- `.sqlx/*` generated metadata as required by SQLx
- `crates/store/src/adapter_kv.rs`
- `crates/store/tests/adapter_kv.rs`
- `crates/bridge-host/src/host.rs`
- `crates/bridge-host/src/lifecycle.rs`
- `crates/bridge-host/tests/store_kv.rs`
- `crates/bridge-host/tests/lifecycle.rs`
- `crates/kernel/src/bridge_runtime.rs`
- `crates/kernel/tests/bridge_lifecycle.rs`
- `crates/fabric/tests/bridge_lifecycle.rs`
- `ARCHITECTURE.md`
- `SECURITY.md`
- `ENVIRONMENT.md`
- `TESTING.md`
- `COMMANDS.md`
- `DECISIONS.md`
- `NEXUS_INTEGRATION.md`
- `NEXUS_INTEGRATION_AUDIT.md`
- `PRODUCTION_READINESS.md`

## 7. Interfaces and Contracts

- The Store exposes `get_for_tenant` and `set_for_tenant`, both requiring a
  non-nil Hydra tenant UUID and validating bounded identifiers.
- `TenantStoreKvStore` binds a tenant UUID and adapter ID at construction and
  delegates only to tenant-aware Store methods.
- `BridgeLifecycle::probe` receives the tenant from the governed request.
- The kernel passes the envelope tenant when it constructs the lifecycle
  request.
- Legacy unscoped `AdapterKvRepo::get` and `set` remain available only as
  fail-closed compatibility shims and issue no SQL.
- Scratch state contains no access tokens, secrets, customer records, or
  tenant authority.
- Store remains the only SQL boundary; migrations are additive.

## 8. Milestones

### M1 - Additive persistence and Store boundary

Add the tenant-scoped table, SQLx metadata, input validation, Store methods,
and Store isolation tests.

Validation:

```text
cargo test -p store --test adapter_kv --offline -- --nocapture
```

Expected output: passing tests prove equal adapter IDs remain isolated across
tenants and legacy methods fail closed.

### M2 - BridgeHost lifecycle boundary

Add `TenantStoreKvStore`, make lifecycle probes require a tenant, and update
BridgeHost tests.

Validation:

```text
cargo test -p bridge-host --test store_kv --test lifecycle --offline -- --nocapture
```

Expected output: successful Store KV and lifecycle test completion.

### M3 - Kernel governed wiring

Pass the governed envelope tenant into the BridgeHost lifecycle request and
retain fail-closed behavior for unsupported or malformed paths.

Validation:

```text
cargo test -p hydra-kernel --test bridge_lifecycle --offline -- --nocapture
```

Expected output: successful governed bridge lifecycle test completion.

### M4 - Documentation and repository gates

Document the corrected boundary, record ADR-0044, and run all required local
gates.

Validation:

```text
bash scripts/preflight.sh
bash scripts/security-check.sh
bash scripts/dependency-audit.sh
bash scripts/check-execplan-state.sh
bash scripts/verify.sh
git diff --check
```

Expected output: `preflight: ok`, `security check: ok`, `dependency audit:
ok`, `execplan state: ok`, `verify: ok`, and zero from `git diff --check`.

## 9. Concrete Steps

1. Activate EP-034 in the state index and extend the state checker.
2. Add migration 0019 without changing historical `adapter_kv` rows.
3. Implement validated tenant-aware Store methods and fail-closed legacy
   shims with focused isolation tests.
4. Add `TenantStoreKvStore` and require tenant identity in lifecycle probes.
5. Pass the governed envelope tenant from Kernel into BridgeHost.
6. Update architecture, security, operations, commands, Nexus integration,
   readiness, and decision records.
7. Run focused tests, full gates, and diff/state checks; record exact results.

## 10. Validation and Acceptance

- The state checker accepts exactly one EP-034 ACTIVE row during work and no
  ACTIVE row after completion.
- The migration is additive and applies to a disposable PostgreSQL database.
- Equal adapter IDs in different tenants cannot read or overwrite each
  other's scratch state.
- Legacy unscoped Store methods fail before SQL execution.
- BridgeHost lifecycle probes use the tenant-scoped KV implementation.
- Kernel governed bridge execution passes the envelope tenant.
- No tenant ID is accepted from adapter scratch payloads as authority.
- Focused tests, preflight, security, dependency, and full verification pass.
- EP-010 remains partial for staging, recovery, soak, security review,
  performance, accessibility, restore, rollback, and human sign-off.

## 11. Idempotence and Recovery

The migration is safe through the repository migration runner. Tenant KV
writes use an upsert on the tenant-scoped primary key. Re-running tests uses
ephemeral test schemas. No historical unscoped rows are changed. If SQLx
metadata fails, confirm all migrations are applied to the disposable database
and rerun preparation. If a targeted test fails, isolate it before changing
code. Never weaken a required gate or reinterpret old rows to recover.

## 12. Progress

- [x] M1 - Additive persistence and Store boundary (`cargo test -p store --test adapter_kv --offline -- --nocapture` -> 3 passed)
- [x] M2 - BridgeHost lifecycle boundary (`cargo test -p bridge-host --test store_kv --test lifecycle --offline -- --nocapture` -> 3 passed)
- [x] M3 - Kernel governed wiring (`cargo test -p hydra-kernel --test bridge_lifecycle --offline -- --nocapture` -> 1 passed; Fabric lifecycle -> 1 passed)
- [x] M4 - Documentation and repository gates (`preflight: ok`, `security check: ok`, `dependency audit: ok`, `verify: ok`, `git diff --check` passed)

## 13. Surprises & Discoveries

- The first full verifier correctly exposed a stale Fabric regression test that
  called the legacy unscoped Store method with a synthetic `tenant:adapter`
  identifier. The production path was already tenant-aware; the test was
  updated to use the canonical tenant-aware API and rerun successfully.
- SQLx preparation against the disposable database refreshed the checked
  query metadata without adding a dependency.

## 14. Decision Log

| Date | Context | Decision | Why |
|---|---|---|---|
| 2026-08-12 | Historical adapter KV rows lack tenant identity | Add a parallel tenant-scoped table instead of assigning tenants to old rows | Reassignment would invent authority and could expose another tenant's state |
| 2026-08-12 | Existing callers expose an unscoped Store API | Keep compatibility shims that fail closed without SQL | Preserves compilation without preserving an unsafe data path |
| 2026-08-12 | Full verification found a stale test caller | Update the caller to pass tenant identity explicitly | Required tests must exercise the production contract rather than a synthetic global key |

## 15. Outcomes & Retrospective

EP-034 is complete. Migration `0019_tenant_adapter_kv.sql` adds a
tenant-scoped `(tenant_id, adapter_id, k)` boundary without modifying the
historical unscoped table. Store validation rejects nil or malformed scope,
legacy `get`/`set` fail closed before SQL, BridgeHost binds tenant identity in
`TenantStoreKvStore`, and Kernel passes `envelope.tenant` into lifecycle
probes. Equal adapter IDs are isolated by focused Store and BridgeHost tests.

Validation evidence: SQLx metadata preparation passed; Store isolation and
fail-closed tests passed 3/3; BridgeHost Store/lifecycle tests passed 3/3;
Kernel lifecycle passed 1/1; Fabric lifecycle passed 1/1 after correcting the
stale test caller; preflight, security, dependency, formatting, and state
checks passed; the captured full verifier ended with `smoke test: ok`,
`cache-hit audit: ok (ratio=0.9717)`, and `verify: ok`. The first verifier
attempt failed only on the stale unscoped test caller and was not counted as
passing. No production deployment or production database was used; the
disposable PostgreSQL cluster was stopped and removed after validation.
EP-010 remains partial, and synchronization plus legacy-row migration remain
future work.
