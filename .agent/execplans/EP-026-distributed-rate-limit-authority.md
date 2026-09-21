# EP-026 Distributed Rate-Limit Authority

Plan status: COMPLETE

## 1. Purpose / Big Picture

Replace the Kernel's process-local rate-limit state with a Store-owned, atomic Postgres window authority for the real runtime. A single local process may enforce a limit today, but multiple Kernel replicas can otherwise each admit their own quota. Hydra must preserve its current 429 contract, fail closed when the distributed authority is unavailable, avoid persisting raw principal or IP identifiers, and retain an explicit local/test constructor for deterministic isolated unit tests.

## 2. Scope

- Add an additive Store migration and repository for hashed rate-limit window keys.
- Implement an atomic fixed-window increment/check operation with bounded counts and deterministic retry-after values.
- Hash Fabric-derived principal/IP keys before persistence; no raw identity or address may enter Postgres.
- Add bounded expired-window pruning without a second queue or scheduler dependency.
- Make the real Kernel construct the Store-backed limiter; keep `RateLimiter::new` local-only for tests and compatibility.
- Convert outer middleware and authenticated local/Nexus rate checks to the asynchronous authority path.
- Map database authority failure to a non-secret `503` fail-closed response rather than falsely returning `429`.
- Add Store, Fabric, and Kernel boundary tests for limits, key isolation, hash non-disclosure, backend failure, and runtime wiring.
- Reconcile security, environment, readiness, operations, audit, commands, ADR, and plan-state documentation.

## 3. Non-goals

- No Redis, NATS, external quota service, or new runtime dependency.
- No caller-selectable rate-limit policy, tenant authority, or raw key persistence.
- No change to the existing 60 requests/60 seconds default in the Kernel.
- No removal of local deterministic test constructors.
- No production database, staging deployment, push, merge, tag, release, or destructive cleanup.

## 4. Context and Orientation

`crates/fabric/src/rate.rs` stores `(Instant, count)` in a process-local `Mutex<HashMap>`. The Kernel constructs `RateLimiter::new(60, 60)` and installs it as outer middleware; local and Nexus authentication paths also call the same limiter after identity resolution. `store::Store` is the only SQL boundary and already exists in Kernel, so a Store-owned atomic window repository is the smallest compatible distributed authority. The key material is derived in Fabric from authenticated principal or peer IP and must be SHA-256 digested before Store receives it.

The database table is operational state, not CRM/CDM data, audit authority, or an authorization source. The limiter only answers admission for an already-authenticated or network-derived request key; it never establishes tenant identity or scope.

## 5. Files to Read First

`AGENTS.md`; `COMMANDS.md`; `.agent/PLANS.md`; `.agent/EXECUTION_RULES.md`; `.agent/state/execplan-index.md`; `SECURITY.md`; `ENVIRONMENT.md`; `OPERATIONS.md`; `PRODUCTION_READINESS.md`; `DECISIONS.md`; `crates/fabric/src/rate.rs`; `crates/fabric/src/services.rs`; `crates/fabric/src/rest/mod.rs`; `crates/fabric/src/rest/nexus.rs`; `crates/kernel/src/main.rs`; `crates/store/src/lib.rs`; `crates/store/src/testkit.rs`; `migrations/0003_envelopes.sql`; current Fabric rate/auth tests.

## 6. Files to Change

- `.agent/execplans/EP-026-distributed-rate-limit-authority.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`
- `migrations/0017_rate_limit_windows.sql` (new)
- `crates/store/src/rate_limits.rs` (new)
- `crates/store/src/lib.rs`
- `crates/store/tests/rate_limits.rs` (new)
- `crates/fabric/src/rate.rs`
- `crates/fabric/src/rest/mod.rs`
- `crates/fabric/src/rest/nexus.rs`
- `crates/kernel/src/main.rs`
- `crates/fabric/tests/rate_limits.rs` (new, if boundary coverage needs HTTP wiring)
- `COMMANDS.md`
- `ENVIRONMENT.md`
- `SECURITY.md`
- `OPERATIONS.md`
- `PRODUCTION_READINESS.md`
- `NEXUS_INTEGRATION_AUDIT.md` (append current verification only)
- `DECISIONS.md`
- `.sqlx/` (refresh checked query metadata)

## 7. Interfaces and Contracts

- `store::RateLimitsRepo` accepts only a versioned 64-character digest, a positive request limit, and a positive window length; it returns `allowed` and a bounded retry-after value.
- The atomic Store operation increments one window under a Postgres row lock/upsert, caps the stored count at `max_requests + 1`, and resets the window using database time.
- Fabric's digest includes a stable namespace/version prefix and never stores the raw `principal:<tenant>:<id>` or `network:<ip>` key.
- `RateLimiter::new` remains local-only and synchronous for unit tests. `RateLimiter::with_store(store, max, window)` is the production path and exposes `check_async`.
- `rate_limit_middleware`, local-auth middleware, and Nexus auth middleware use `check_async`; if Store fails, the response is `503` with a generic problem type and no database detail.
- The existing 429 response and `Retry-After` behavior remain unchanged for an exceeded window.
- Expired digests are pruned opportunistically at most once per minute per process; pruning errors fail closed for the current request rather than being silently ignored.

## 8. Milestones

### M1 - Activate and verify the distributed-limit finding

Activate EP-026 after EP-025, run state/preflight, and prove the process-local map and Kernel constructor from current symbols.

Validation: `bash scripts/check-execplan-state.sh` and `bash scripts/preflight.sh`

Expected: `execplan state: ok` and `preflight: ok`.

Recovery: repair plan state before Rust edits; preserve historical completion evidence.

### M2 - Store-backed atomic window repository

Add migration `0017_rate_limit_windows.sql`, Store repository methods, checked queries, pruning, and isolated migration-backed tests for counts, reset, digest validation, and key separation.

Validation: `cargo test -p store --test rate_limits --offline -- --nocapture` and `cargo sqlx prepare --workspace -- --all-targets`.

Expected: Store tests pass and no raw key values are persisted.

Recovery: use the documented loopback Postgres; never target a non-test database or delete shared state.

### M3 - Fabric and Kernel wiring

Add hashed async checks, explicit backend failure mapping, update local/Nexus/outer middleware, and construct the Store-backed limiter in the real Kernel. Preserve local/test constructors.

Validation: `cargo test -p fabric --lib rate --offline -- --nocapture`, `cargo test -p hydra-kernel --bin hydra-kernel --offline -- --nocapture`, and `cargo check -p hydra-kernel --tests --offline`.

Expected: 429/503 contracts, hash non-disclosure, and real Kernel wiring pass.

Recovery: keep local fallback only in explicitly local/test construction; do not silently fall back from a configured Store authority in production.

### M4 - Documentation and acceptance wiring

Document the distributed limiter, failure behavior, operational cleanup, and local-only test constructor. Add ADR-0036, command references, audit/readiness appendices, and state-checker coverage.

Validation: `bash scripts/check-execplan-state.sh` and `bash scripts/preflight.sh`.

Expected: `execplan state: ok` and `preflight: ok`.

Recovery: preserve the existing rate-limit contract and document any local-only evidence.

### M5 - Full isolated verification and reconciliation

Run focused Store/Fabric/Kernel tests and the complete isolated verifier against loopback Postgres and JetStream. Complete only when the production Kernel uses distributed rate state and all gates pass.

Validation: explicit isolated `bash scripts/verify.sh`.

Expected: exit 0 through `verify: ok`; no production or staging action.

Recovery: follow AGENTS.md §7 and retain EP-010 staging/human gaps.

## 9. Concrete Steps

1. Activate EP-026 and validate state/preflight.
2. Add the additive rate-window migration and Store repository/tests.
3. Add digesting, async middleware checks, failure mapping, and Kernel wiring.
4. Refresh SQLx metadata and add boundary tests.
5. Update operations/security/readiness/audit/decision/commands.
6. Run focused and full isolated gates.
7. Reconcile EP-026 and leave EP-010 partial.

## 10. Validation and Acceptance

- Concurrent Store requests for one digest cannot admit more than the configured window limit.
- Different digests are isolated and window reset uses database time.
- Raw principal/IP key text is absent from stored rate-limit rows and responses.
- Exceeded requests return 429 with bounded `Retry-After`.
- Store authority failure returns 503 fail-closed and does not reveal SQL details.
- Local/test constructors retain deterministic synchronous behavior.
- The production Kernel constructs the Store-backed limiter; all relevant middleware uses the async path.
- Expired rows are pruned opportunistically and pruning is bounded.
- SQLx metadata, focused tests, state/preflight, and full isolated verification pass.
- EP-010 remains partial for staging, human review, and production launch evidence.

## 11. Idempotence and Recovery

The migration is additive with a revert note. Upsert/check is safe to retry; database row locking determines one authoritative increment. Pruning is safe to repeat and only removes expired operational windows. If the Store is unavailable, requests fail closed with 503 until the authority returns; no process-local fallback is permitted in the production constructor. If interrupted, resume the first unchecked milestone.

## 12. Progress

- [x] M1 - Distributed-limiter finding verified and EP-026 activated.
- [x] M2 - Store-backed atomic window repository complete.
- [x] M3 - Fabric and Kernel wiring complete.
- [x] M4 - Documentation and acceptance wiring complete.
- [x] M5 - Full isolated verification and truthful reconciliation complete.

## 13. Surprises & Discoveries

The initial combined focused command exceeded its 180-second timeout without
emitting a compiler error. A narrower Fabric library check then passed, after
which the individual Fabric and Kernel suites passed. Preflight also required
the direct Git-Bash executable because the wrapper shell did not expose Cargo
to child Bash processes; no repository toolchain or source fallback was used.

## 14. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-11 | Use Postgres as the distributed rate-limit authority | Store is already the only SQL boundary and is available to every Kernel replica; adding Redis/NATS would create a second operational dependency and authority. |
| 2026-08-11 | Persist only a versioned SHA-256 digest | Principal IDs, tenant IDs, and peer IPs are operationally sensitive and do not belong in rate-limit rows; the digest preserves isolation without raw identity leakage. |
| 2026-08-11 | Fail closed on authority failure | A production limiter that silently falls back to per-process quotas would create an unbounded multi-replica bypass and falsely signal protection. |

## 15. Outcomes & Retrospective

M2 added migration `0017_rate_limit_windows.sql`, checked Store queries, and
three isolated database tests covering atomic concurrency, key isolation,
reset, pruning, and validation. M3 added versioned SHA-256 keying, async
authority checks, generic 503 failure mapping, local/test compatibility, and
the real Kernel Store-backed constructor. M4 reconciled commands, security,
environment, operations, readiness, audit, and ADR-0036 documentation.

M5 passed `bash scripts/preflight.sh` (`preflight: ok`),
`bash scripts/lint.sh` (`lint: ok`), focused Store/Fabric/Kernel tests,
`bash scripts/check-execplan-state.sh` (`execplan state: ok`), and the
isolated full verifier, which exited 0 through `verify: ok` after 897.7
seconds. `git diff --check` reported no whitespace errors. No production or
staging database, deployment, push, merge, tag, release, or external provider
was used. EP-010 remains partial until staging quota behavior, multi-replica
drills, security review, and human sign-off exist.
