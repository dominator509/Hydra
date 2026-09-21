# EP-037 - Governed Bridge Full-Relist Synchronization

Plan status: COMPLETE

## 1. Purpose / Big Picture

Implement the full-relist diff fallback required by `wit/hydra-bridge.wit`
when an active adapter declares `incremental-sync: false`. Reuse the existing
authenticated, Governor-controlled `hydra.bridges.sync` command and durable
sync run rather than adding a second synchronization abstraction. The result
must be bounded, tenant-safe, atomic, soft-delete-only, and truthful about
what the adapter actually supports.

## 2. Scope

Add a BridgeHost full-relist read method with cursor-loop and resource-bound
validation, a Store transaction that diffs canonical bridge-origin entities,
and Kernel dispatch that chooses incremental or full-relist from the
persisted descriptor. Add deterministic tests and update the synchronization
specification and operational documentation.

## 3. Non-goals

- No scheduler, background worker, or autonomous retry loop.
- No new public REST/MCP capability or provider-specific API.
- No adapter activation, write-back, mapping synthesis, canary, promotion, or
  deployment.
- No hard delete, purge, destructive migration, or new CRM abstraction.
- No new dependency, Wasm artifact, SQL authority outside Store, or LLM path.
- No staging deployment or EP-010 production-readiness claim.

## 4. Context and Orientation

EP-035 implements governed manual incremental synchronization through
`changes_since` and `BridgeSyncRepo::apply_page`. EP-036 validates the
read-only lifecycle but deliberately does not synchronize. The WIT contract
states that `incremental-sync: false` falls back to full-relist diffing, while
the current `SyncAdapterHandler` rejects that descriptor. The smallest
reversible correction is to keep the existing Store run lease, collect bounded
validated list pages in BridgeHost, and apply the complete snapshot through
one Store transaction.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/specs/SPEC-003-api-contracts.md`
- `.agent/specs/SPEC-015-bridge-synchronization.md`
- `.agent/specs/SPEC-017-bridge-full-relist.md`
- `wit/hydra-bridge.wit`
- `crates/bridge-host/src/lifecycle.rs`
- `crates/bridge-host/tests/lifecycle.rs`
- `crates/store/src/bridge_sync.rs`
- `crates/store/tests/bridge_sync.rs`
- `migrations/0020_bridge_synchronization.sql`
- `crates/kernel/src/bridge_runtime.rs`
- `crates/kernel/tests/bridge_lifecycle.rs`
- `ARCHITECTURE.md`
- `SECURITY.md`
- `TESTING.md`

## 6. Files to Change (== Expected Changed Files)

- `.agent/specs/SPEC-017-bridge-full-relist.md`
- `.agent/execplans/EP-037-bridge-full-relist.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `COMMANDS.md`
- `crates/bridge-host/src/lifecycle.rs`
- `crates/bridge-host/src/lib.rs`
- `crates/bridge-host/tests/lifecycle.rs`
- `crates/store/src/bridge_sync.rs`
- `crates/store/src/lib.rs`
- `crates/store/tests/bridge_sync.rs`
- `crates/kernel/src/bridge_runtime.rs`
- `crates/kernel/tests/bridge_lifecycle.rs`
- `ARCHITECTURE.md`
- `NEXUS_INTEGRATION.md`
- `NEXUS_INTEGRATION_AUDIT.md`
- `OPERATIONS.md`
- `SECURITY.md`
- `TESTING.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`

## 7. Interfaces and Contracts

- `BridgeLifecycle::full_relist` accepts tenant, adapter identity, stored
  component reference/digest, stored grant/config, requested kind, and page
  limit; it returns only owned validated records plus bounded metadata.
- The host enforces fixed maximum pages, records, aggregate bytes, valid
  cursors, requested kind, and unique `(kind, id)` identities. It calls only
  `describe`, `probe`, and `list` for this path.
- `BridgeSyncRepo::apply_full_relist` receives tenant/run/adapter/kind,
  validated upsert changes, and event provenance. It atomically diffs active
  bridge-origin entities, soft-deletes missing rows, emits canonical events,
  and advances state only on commit.
- `hydra.bridges.sync` input and capability schema remain backward compatible.
  Receipt details add `strategy: incremental | full_relist` and counts but no
  raw record data.

## 8. Milestones

### M1 - Contract and plan-state activation

Add SPEC-017, activate EP-037 in the index, extend the plan-state validator,
and record the WIT fallback and bounded manual scope. Validate with
`bash scripts/preflight.sh` and `bash scripts/check-execplan-state.sh`; expect
`preflight: ok` and `execplan state: ok`.

### M2 - Bounded BridgeHost full-relist

Add owned request/result records and a read-only list loop. Validate cursors,
page sizes, total pages/records/bytes, duplicate identities, requested kind,
and adapter descriptor truth. Add fixture tests for multi-page success,
repeated cursor, over-bound data, invalid record, and digest mismatch.
Validate with the focused BridgeHost lifecycle tests; all full-relist tests
must pass.

### M3 - Atomic Store diff

Add Store-owned full-relist application without a migration. Preserve origin
identity, no-op unchanged rows, update changed rows, revive matching
tombstones, soft-delete absent active rows, append events/outbox atomically,
and leave state unchanged on failure. Validate with Store bridge sync tests,
including cross-tenant and rollback assertions.

### M4 - Kernel mode selection and integration

Update the typed sync handler to select incremental mode only when the
persisted descriptor advertises it; otherwise execute the bounded full-relist
path. Keep all adapter configuration Store-resolved and all receipts
metadata-only. Add Kernel tests proving fallback, no raw payload leakage,
tenant isolation, idempotent unchanged relists, and failure without partial
state.

### M5 - Documentation and full verification

Update the synchronization contract, architecture, operations, security,
testing, Nexus audit, readiness, commands, and decisions. Run formatting,
SQLx validation, focused tests, preflight, security, dependency, state, and
the full verifier against disposable services. Keep EP-037 active if any
required gate fails; do not treat Docker/staging absence as passed evidence.

## 9. Concrete Steps

1. Confirm the exact generated WIT raw-record types and existing Store origin
   identity helpers before editing.
2. Implement the host list loop with explicit fixed bounds and sanitized
   errors; never invoke adapter mutation exports.
3. Implement Store full-relist diffing inside the existing run transaction;
   reuse event/provenance helpers and soft-delete semantics.
4. Change only the existing sync handler's mode selection and receipt shape.
5. Add focused tests before ticking each milestone.
6. Update documentation and the plan index only after all required validation
   commands pass.

## 10. Validation and Acceptance

Required outputs:

- `bash scripts/preflight.sh` -> `preflight: ok`.
- `bash scripts/check-execplan-state.sh` -> `execplan state: ok`.
- Focused BridgeHost, Store, and Kernel full-relist tests pass.
- `cargo fmt --all -- --check` exits 0.
- SQLx metadata/check validation exits 0.
- `bash scripts/security-check.sh` -> `security check: ok`.
- `bash scripts/dependency-audit.sh` -> `dependency audit: ok`.
- `bash scripts/verify.sh` -> `verify: ok`.
- `git diff --check` exits 0 and changed files are within section 6.
- No raw adapter records, secrets, or caller tenant authority appear in
  receipts, logs, event subjects, or tests.
- No production deployment, push, tag, or non-test database operation occurs.

## 11. Idempotence and Recovery

The host list loop is read-only and can be repeated with the same stored
adapter artifact. Store application is protected by the existing one-active-
run lease and transaction; unchanged rows are not versioned again. If the
host or validation fails, call the existing run-failure path and leave the
sync cursor and canonical entities unchanged. After interruption, rerun
preflight and the milestone validation to determine whether the migration-free
code/test state is present, then continue from the first unchecked milestone.

## 12. Progress

- [x] M1 - Contract and plan-state activation
- [x] M2 - Bounded BridgeHost full-relist
- [x] M3 - Atomic Store diff
- [x] M4 - Kernel mode selection and integration
- [x] M5 - Documentation and full verification

## 13. Surprises & Discoveries

Record exact findings here as milestones execute. Do not convert missing
Docker, external providers, or staging evidence into a passing result.

- 2026-08-12 - The repository preflight reports only the expected local notes
  that `.env` is absent and Docker is unavailable; `preflight: ok` and the
  state validator both passed, so no external service is required for the
  contract activation milestone.
- 2026-08-12 - The checked-in memcrm fixture returns an empty list and declares
  incremental sync, so multi-page, duplicate, cursor, and byte behavior is
  covered by deterministic host accumulator tests while the lifecycle fixture
  proves the real Wasmtime list call. Kernel fallback coverage uses a test-only
  persisted descriptor capability flip; the checked-in artifact remains
  unchanged.
- 2026-08-12 - The Store revival test initially expected version 2, but the
  correct append-only version sequence is create=1, soft-delete=2,
  revive/update=3. The assertion was corrected; implementation behavior was
  already correct.
- 2026-08-12 - The first full verifier reached the real Kernel smoke test
  before the disposable database had been migrated; `/healthz` correctly
  returned a generic rate-limiter-unavailable 503 because request admission
  fails closed when its Store authority is unavailable. Applying the 21
  additive migrations to the loopback-only test database made the narrowed
  smoke test pass, and the unchanged full verifier then completed with
  `verify: ok`.

## 14. Decision Log

- 2026-08-12 - Selected full-relist as the next seam because the normative
  WIT contract requires it and the current sync handler rejects that
  descriptor. Reuse the existing governed capability and persistence tables
  to avoid a second synchronization abstraction.
- 2026-08-12 - Keep the complete relist in one bounded Store transaction so
  missing-record soft deletes cannot be committed ahead of later page or
  validation failures.
- 2026-08-12 - Reused the existing `hydra.bridges.sync` capability and
  `BridgeSyncRepo` run lease instead of adding a new capability or migration;
  this preserves external compatibility and keeps SQL in Store.
- 2026-08-12 - Used a manual SQLx `FromRow` implementation because the pinned
  workspace enables SQLx macros but not the derive feature; no dependency or
  workspace feature change was necessary.

## 15. Outcomes & Retrospective

EP-037 completed 2026-08-12. BridgeHost now provides bounded list
pagination, Store applies an atomic snapshot diff, and Kernel selects the
mode from the persisted descriptor. Documentation, security/dependency
gates, plan-state validation, and the unchanged full verifier passed. The
full verifier first exposed an uninitialized disposable database; no source
or gate was weakened, the documented migrations were applied, and the
repeated acceptance run completed in 660.7 seconds with `verify: ok`.
No scheduler, staging, production, deployment, push, tag, or non-test
database operation occurred.
