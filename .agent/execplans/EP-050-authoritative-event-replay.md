# EP-050 Authoritative Event Replay

Plan status: COMPLETE

## 1. Purpose / Big Picture

Add a bounded, owner-confirmed recovery command that replays validated
canonical events from Hydra's Postgres outbox into JetStream after broker
loss, stream replacement, or a consumer rebuild. Preserve Postgres as the
source of truth and make duplicate delivery explicit and deduplicable.

## 2. Scope

- Activate EP-050 as the only active plan.
- Add SPEC-030 and state/index evidence.
- Add Store-owned ordered outbox replay access.
- Add `hydra-kernel --replay-events` with explicit confirmation and limits.
- Add focused and disposable service-backed replay tests.
- Update commands, operations, deployment, security, readiness, and ADR
  documentation truthfully.
- Run the full repository verifier.

## 3. Non-goals

- No direct SQL outside Store, no CRM mutation, and no outbox mutation.
- No automatic replay of stale ActionEnvelopes or execution tokens.
- No live JetStream filesystem copying, snapshot API invention, NATS source of
  truth, consumer-side mutation, or unbounded replay.
- No new dependency unless the existing async-nats API is insufficient and the
  dependency review explicitly authorizes one.
- No production deployment, staging drill, push, tag, or production DB.

## 4. Context and Orientation

EP-014 implemented acknowledged JetStream publishing and EP-049 documented
that the current async-nats client has no snapshot/restore API. Outbox rows
remain durable and canonical, but the normal relay marks successfully
published rows and will not re-emit them after a broker volume loss. The
existing Kernel binary already owns the event publisher and is the narrowest
place for a recovery-only command, while Store remains the only SQL owner.

## 5. Files to Read First

- `AGENTS.md`
- `COMMANDS.md`
- `.agent/PLANS.md`
- `.agent/EXECUTION_RULES.md`
- `.agent/state/execplan-index.md`
- `.agent/specs/SPEC-029-backup-artifact-lifecycle.md`
- `crates/store/src/outbox.rs`
- `crates/store/src/lib.rs`
- `crates/kernel/src/main.rs`
- `crates/kernel/src/event_stream.rs`
- `crates/kernel/tests/integration_event_bridge.rs`
- `crates/kernel/tests/trace_and_event_readiness.rs`
- `scripts/test-integration.sh`
- `OPERATIONS.md`
- `DEPLOYMENT.md`
- `SECURITY.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`
- `.agent/specs/SPEC-030-authoritative-event-replay.md`

## 6. Files to Change

- `crates/store/src/outbox.rs`
- `crates/store/tests/integration_event_replay.rs`
- `crates/kernel/src/main.rs`
- `crates/kernel/tests/event_replay.rs`
- `.sqlx/` (checked query metadata refreshed for the new Store query; preserve
  existing repository snapshots)
- `COMMANDS.md`
- `OPERATIONS.md`
- `DEPLOYMENT.md`
- `SECURITY.md`
- `PRODUCTION_READINESS.md`
- `DECISIONS.md`
- `.agent/specs/SPEC-030-authoritative-event-replay.md`
- `.agent/execplans/EP-050-authoritative-event-replay.md`
- `.agent/state/execplan-index.md`
- `scripts/check-execplan-state.sh`

## 7. Interfaces and Contracts

- Store exposes a bounded `list_for_replay(after_id, limit)` repository method
  returning validated `OutboxRecord` values in ascending outbox ID order.
- `hydra-kernel --replay-events` requires `DATABASE_URL`, `NATS_URL`,
  `HYDRA_EVENT_REPLAY_CONFIRM=I_UNDERSTAND`, and optional
  `HYDRA_EVENT_REPLAY_AFTER_ID`/`HYDRA_EVENT_REPLAY_LIMIT` values.
- Limit defaults to a small bounded value and must be within `1..=1000`.
- The command publishes each record with the canonical subject and event ID,
  stops on the first failure, and prints only `replayed`, `scanned`, and
  `last_outbox_id` metadata.
- No row is marked published, no claim token is created, and no event is
  deleted or altered.

## 8. Milestones

### M1 - Activate contract and state

Add SPEC-030, EP-050, the index row/transition, and checker coverage. Run
preflight and state validation. Expected: `preflight: ok` and
`execplan state: ok`.

### M2 - Add Store replay access

Implement bounded ordered replay selection using Store-owned parameterized SQL
and integration tests for limits, ordering, tenant/event validation, and no
mutation. Refresh checked SQLx metadata if required. Expected focused Store
tests pass.

### M3 - Add confirmation-gated Kernel command

Wire the existing JetStream publisher into `--replay-events`, parse and bound
configuration, preserve trace context, stop on failure, and add unit tests for
configuration/output safety. Expected Kernel test target compiles and focused
tests pass.

### M4 - Disposable replay round trip and documentation

Run the command against disposable loopback Postgres/NATS with synthetic
canonical events, prove stable message IDs and repeat delivery, and update
commands/operations/deployment/security/readiness/ADR documentation.

### M5 - Full acceptance and closeout

Run the resource-safe full verifier, state checker, and diff check. Expected:
`verify: ok`, `execplan state: ok`, and no production action.

## 9. Concrete Steps

1. Confirm EP-049 is complete and no active plan exists.
2. Add SPEC-030 and activate EP-050.
3. Add Store's bounded replay query and integration coverage.
4. Add the explicit Kernel replay command using the existing publisher.
5. Run focused disposable replay tests and contract the stable output.
6. Reconcile operational and security documentation, emphasizing duplicate
   delivery and consumer deduplication.
7. Run the full verifier and close only after every required marker passes.

## 10. Validation and Acceptance

EP-050 is accepted only when:

- the plan state checker passes with exactly one valid active/completed state;
- replay selection is bounded, ordered, validated, and SQL remains in Store;
- missing/invalid confirmation, URLs, cursor, and limits fail closed;
- canonical event IDs, subjects, payloads, and trace carriers are preserved;
- replay never changes outbox publication state or CRM state;
- a publish failure cannot produce a success marker;
- disposable Postgres/NATS replay tests, security, dependency, preflight, and
  full gates pass; and
- no production or staging operation occurs.

## 11. Idempotence and Recovery

Re-running a range republishes the same event IDs and is safe only for
consumers that deduplicate by event ID. The command does not advance a cursor
or mark rows; operators may resume from the last reported outbox ID. A failed
batch stops without claiming subsequent rows. If the broker is unavailable,
the command fails closed and the Postgres outbox remains unchanged.

## 12. Progress

- [x] M1 - Contract and active-plan state (`preflight: ok`; `execplan state: ok`, 2026-08-12)
- [x] M2 - Store replay access (`integration_event_replay`: 1 passed; checked SQLx metadata refreshed, 2026-08-12)
- [x] M3 - Confirmation-gated Kernel command (`hydra-kernel` binary tests: 23 passed; replay bounds/cursor and direct metadata output covered, 2026-08-12)
- [x] M4 - Disposable replay round trip and documentation (real Kernel child replayed twice; fake Nexus received one logical event; outbox publication fields unchanged; command/runbook/deployment/security/readiness/ADR updated, 2026-08-12)
- [x] M5 - Full acceptance and closeout (`bash scripts/verify.sh` exited 0 after 1093.6s with the repository's terminal `verify: ok` path; `cargo fmt --all -- --check`; `git diff --check`; no production/staging action, 2026-08-12)

## 13. Surprises & Discoveries

- The existing Kernel event publisher already validates the configured stream,
  canonical subject, acknowledgement stream, and message sequence, so replay
  can reuse it without a new NATS abstraction.
- The existing outbox repository validates canonical event documents while
  materializing rows; replay must use that path rather than a raw JSON query.
- The first replay integration attempt used `127.0.0.1:4222`, but the
  disposable NATS listener was configured on IPv6 loopback `[::1]:4222`; the
  test default and focused command were corrected to the actual service
  endpoint.
- The first success assertion relied on an `info!` log event and was hidden
  when the inherited `RUST_LOG` filter suppressed INFO. The CLI now prints one
  deterministic, non-secret metadata line for success instead of making the
  gate depend on log verbosity.
- The initial Store replay fixture omitted the CDM-required `party` display
  name and failed during entity validation; the fixture was corrected rather
  than weakening the canonical schema.

## 14. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-12 | Replay from Postgres outbox rather than snapshotting live NATS files | Postgres outbox is authoritative and the current client has no supported snapshot API; raw file copying risks inconsistent recovery. |
| 2026-08-12 | Require operator confirmation and a maximum batch size of 1000 | Replay can duplicate deliveries and must not become an unbounded accidental load or side-effect trigger. |
| 2026-08-12 | Do not mark replayed rows published | Replay is recovery delivery, not normal relay acknowledgement bookkeeping; canonical outbox history must remain unchanged. |
| 2026-08-12 | Make replay success a direct metadata line rather than an INFO event | Recovery gates and operators need a stable success signal even when `RUST_LOG` filters informational tracing; the line contains no payload or secret. |

## 15. Outcomes & Retrospective

EP-050 is complete. The repository now has an executable, bounded
Postgres-authoritative JetStream recovery path with honest duplicate-delivery
semantics. Focused Store and Kernel tests, fail-closed CLI checks, the real
Kernel child replay round trip against disposable Postgres/JetStream, checked
SQLx metadata, formatter/diff checks, and the full verifier passed. Staging
broker recovery, off-box protection, JetStream snapshot/restore policy, and
operator/human sign-off remain separate EP-010 evidence.
