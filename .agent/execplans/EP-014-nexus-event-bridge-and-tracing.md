# EP-014 Nexus Event Bridge and Tracing

Plan status: COMPLETE

## 1. Purpose / Big Picture
Create a durable, versioned, replayable, correlation-preserving event contract from authoritative Hydra Postgres state to Nexus over actual NATS JetStream, with publish acknowledgement before outbox completion and W3C trace propagation across the request/execution/bridge/relay path.

## 2. Scope
Canonical event type/envelope and schemas; additive outbox metadata; transactional envelope/entity events; JetStream stream bootstrap and ack-aware relay; stable event IDs/retry/parking; event health/readiness; W3C trace context; deterministic durable fake Nexus consumer with replay/dedup/restart tests; contract fixtures and secret scans.

## 3. Non-goals
NATS as source of truth, inbound NATS mutation commands, real Nexus repository dependency, production deployment, global exactly-once delivery, PII in subjects, raw customer documents/tokens/secrets in events, GraphQL, A2A, model provider federation, or completion of EP-010 staging readiness.

## 4. Context and Orientation
Current outbox rows contain ad hoc JSON, relay publishes to `hydra.events.<tenant>` with core NATS, flushes, and marks published without JetStream acknowledgement. Envelope transitions do not emit outbox events. `/readyz` checks only basic NATS connectivity. EP-013 provides invocation/correlation and tenant-safe transitions. SPEC-010 sections 14 through 16 control this plan.

## 5. Files to Read First
`.agent/specs/SPEC-010-nexus-interoperability.md`; `ARCHITECTURE.md`; `OBSERVABILITY.md`; `SECURITY.md`; `crates/cdm/src/{lib.rs,entity.rs}`; `crates/governor/src/envelope.rs`; `crates/store/src/{entities.rs,envelopes.rs,event_log.rs,outbox.rs,lib.rs}`; `crates/kernel/src/{relay.rs,main.rs,config.rs,telemetry.rs}`; `crates/fabric/src/{lib.rs,services.rs}`; `migrations/0002_event_log_and_outbox.sql`; current NATS/relay tests; `docker/compose.yaml`.

## 6. Files to Change
`Cargo.toml`; `Cargo.lock` only if an accepted tracing dependency is added; `deny.toml` only for evidence-backed license configuration; `DECISIONS.md`; `ARCHITECTURE.md`; `OBSERVABILITY.md`; `ENVIRONMENT.md`; `NEXUS_INTEGRATION.md`; `crates/cdm/src/lib.rs`; `crates/cdm/src/events.rs` (new); `crates/cdm/tests/event_contract.rs` (new); `crates/store/src/lib.rs`; `crates/store/src/entities.rs`; `crates/store/src/envelopes.rs`; `crates/store/src/events.rs`; `crates/store/src/outbox.rs`; `migrations/0012_interoperability_events.sql` (new); `.sqlx/` metadata for changed queries; `crates/fabric/src/lib.rs`; `crates/fabric/src/trace_context.rs` (new); `crates/fabric/src/services.rs`; `crates/kernel/src/config.rs`; `crates/kernel/src/main.rs`; `crates/kernel/src/relay.rs`; `crates/kernel/src/event_stream.rs` (new); `crates/kernel/src/telemetry.rs`; `crates/kernel/tests/integration_event_bridge.rs` (new); `crates/kernel/tests/support/fake_nexus_consumer.rs` (new); `crates/kernel/tests/fixtures/events/hydra.crm.entity.created.v1.json` (new); `crates/kernel/tests/fixtures/events/hydra.crm.envelope.executed.v1.json` (new); `docker/compose.yaml` only for test JetStream configuration if required; this plan and `.agent/state/execplan-index.md`.

M4 justified additions after tracing the real asynchronous call graph: `crates/store/Cargo.toml`; `crates/store/src/{approvals.rs,idempotency.rs,trace_context.rs}`; `migrations/0013_trace_context.sql`; `crates/fabric/src/auth/{principal.rs,oidc.rs,authorization.rs}`; `crates/fabric/src/rest/nexus.rs`; `crates/fabric/tests/governed_nexus_mutations.rs`; `crates/kernel/src/{event_status.rs,execution_registry.rs,executor.rs}`; `crates/kernel/tests/{integration_executor.rs,trace_and_event_readiness.rs}`. These files are required to restore trace context after durable asynchronous dispatch, propagate approval/proposal traces, and make the existing status route/runtime readiness truthful; ADR-0024 and the Decision Log record the scope expansion.

M5 justified the direct test-only `futures-util` declaration in `Cargo.toml` and `crates/kernel/Cargo.toml`: the fake Nexus pull consumer must drive the async JetStream message stream directly, and the version was already present transitively in the lockfile. ADR-0025 records the bounded dependency decision.

## 7. Interfaces and Contracts
Canonical event fields and names MUST match SPEC-010. `event_id` is deterministically stable for an outbox row. Postgres event/audit and outbox writes are authoritative and transactional with mutations. A `JetStreamPublisher` seam waits for server acknowledgement before `published_at`; failure leaves the row pending. Unserializable rows are parked with redacted error evidence. Subjects contain stable non-PII taxonomy, not tenant/business/customer values. Durable consumer validates schema, deduplicates by event ID, preserves correlation, acks processing, and resumes after restart.

## 8. Milestones
M1 Canonical event types/schemas. Add typed envelope, normalized event names, schema fixtures, canonical serialization and secret-shaped fixture scan. Validation: `cargo test -p cdm event_contract -- --nocapture`. Expected: schema/version/name/secret tests pass. Recovery: keep event payload minimal and typed; a breaking fixture change requires a new version, not snapshot overwrite.

M2 Transactional store/outbox events. Add migration/metadata, stable ID assignment, actor/binding/correlation fields, entity and envelope transition events. Validation: `cargo test -p store interoperability_events -- --nocapture`. Expected: event ID stability, transition/entity origin, correlation/causation, and transaction rollback tests pass. Recovery: narrow to one mutation transaction; never emit before durable state commit.

M3 JetStream stream and ack-aware relay. Create/verify stream, define retention, publish through JetStream, ack before mark, retry safely, and park serialization failures. Validation: `cargo test -p hydra-kernel event_relay -- --nocapture`. Expected: failed publish remains pending, ack permits mark, retry preserves one logical event. Recovery: use a fake publisher for ack state first, then the actual local JetStream diagnostic; do not substitute core NATS flush.

M4 Trace propagation and readiness. Parse/emit W3C trace context without PII baggage, carry business correlation separately, expose stream/relay status in `/readyz` and `/v1/nexus/events/status`. Validation: `cargo test -p hydra-kernel trace_and_event_readiness -- --nocapture`. Expected: full-path correlation survives and required event outage makes Nexus-connected readiness unhealthy. Recovery: test parser/formatter separately from infrastructure; invalid external trace context starts a new trace but never changes business provenance.

M5 Durable fake Nexus consumer and full gates. Run a local durable consumer through receive/validate/dedup/ack/restart and refresh docs/metadata/audits. Validation: `cargo test -p hydra-kernel integration_event_bridge -- --nocapture` then `bash scripts/verify.sh`. Expected: consumer test passes and `verify: ok`. Recovery: inspect stream/consumer state and sequence; use unique test stream/consumer names and bounded cleanup against test infrastructure only.

## 9. Concrete Steps
Execute M1-M5 in order. Define schemas before producers, transactional records before relay, ack semantics before readiness, and readiness before consumer E2E. Review every subject/payload/log for PII/secrets. Update Progress/Decision Log after each command. Activate EP-015 only after acceptance passes.

## 10. Validation and Acceptance
All user-required EP-014 tests pass: stable event ID, failure/ack marking, logical dedup, schema validation, correlation/causation full path, transition events, bridge origin, secret-free fixtures, unhealthy readiness, durable consumer replay/restart. JetStream is real transport, Postgres remains source of truth, and full verify is green.

## 11. Idempotence and Recovery
Stream bootstrap is create-or-verify. Migration is additive. Stable event IDs make relay retry safe. Consumer dedup store is deterministic. Parking does not delete authoritative rows. Test resources are uniquely named and cleaned only in test NATS. If interrupted, resume from Progress and pending outbox state without manually marking rows published.

## 12. Progress
- [x] M1 - Canonical event types and schemas (`cargo test -p cdm event_contract -- --nocapture`: 4 passed, 2026-08-11)
- [x] M2 - Transactional store/outbox events (`cargo test -p store interoperability_events -- --nocapture`: 3 passed; full Store suite: 19 passed, 2026-08-11)
- [x] M3 - JetStream stream and ack-aware relay (`cargo test -p hydra-kernel event_relay -- --nocapture`: 5 passed including real local JetStream; full Kernel: 29 passed; SQLx offline check passed, 2026-08-11)
- [x] M4 - W3C trace propagation and event readiness (`cargo test -p hydra-kernel trace_and_event_readiness -- --nocapture --test-threads=1`: 3 passed; full Store/Fabric/Kernel: 21/131/32 passed, 2026-08-11)
- [x] M5 - Durable fake consumer and full verification (`cargo test -p hydra-kernel integration_event_bridge -- --nocapture --test-threads=1`: 1 passed; SQLx refresh/offline compile, cargo-deny, cargo-audit, formatting, diff, and plan-state gates passed; `bash scripts/verify.sh`: exit 0 and `verify: ok` terminal path in 973.5 seconds, 2026-08-11)

## 13. Surprises & Discoveries
- 2026-08-11: Activated only after EP-013's focused suites, checked SQL metadata, unmasked integration gate, and full verifier passed.
- 2026-08-11: The queued plan reserved migration `0010`, but EP-013 truthfully consumed `0010` and `0011` for policy revision and execution receipts. EP-014 uses the next additive migration, `0012_interoperability_events.sql`.
- 2026-08-11: The pre-EP-014 execution-provenance test queried the removed ad hoc `envelope.transition` event shape. The runtime correctly wrote `hydra.crm.envelope.queued.v1`; the test was updated to assert the canonical top-level version, envelope, correlation, and causation fields.
- 2026-08-11: Migration `0012_interoperability_events.sql` was applied only to the isolated local database at `127.0.0.1:55432`; no production database was contacted.
- 2026-08-11: The pinned `async-nats` dependency existed with its JetStream module compiled out. Enabling only its existing `jetstream` feature exposed the required two-stage send plus server-ack API without adding a second NATS client.
- 2026-08-11: A broader Kernel pass initially failed seven telemetry tests because this shell inherits `RUST_LOG=warn` and the test subscriber incorrectly honored that production environment filter. The test-only subscriber now uses a deterministic `trace` filter; the isolated telemetry suite passed 8 tests and full Kernel passed 29.
- 2026-08-11: W3C trace restoration across asynchronous execution required a second additive migration and trace-aware Store approval/idempotency plus Kernel Executor paths that the queued file list had not named. Migration `0013_trace_context.sql` was applied only to the isolated database at `127.0.0.1:55432`; old rows remain valid with null trace context.
- 2026-08-11: The first M4 full-path assertion incorrectly required even the proposal event to have a child span. The proposal correctly retains Fabric's server span; downstream Executor/entity events derive children while preserving the trace ID. The corrected test requires one shared trace ID and at least one downstream child.
- 2026-08-11: The populated isolated NATS server rejected a second stream owning `hydra.crm.>` with error code `10065`. The readiness outage test now uses a unique non-overlapping subject, while the real relay test bootstraps/verifies the one canonical stream and isolates by event ID. Both suites pass regardless of broker test order.
- 2026-08-11: The first durable-consumer fixture was correctly rejected because its entity-created payload omitted the required entity reference. Adding the contract-required reference made the focused test pass without weakening validation.
- 2026-08-11: JetStream permits only one stream to own the canonical subject set. M5 therefore creates a unique durable consumer on the canonical test stream and removes only that consumer; it never creates or deletes a competing stream.

## 14. Decision Log
| Date | Decision | Rationale |
|---|---|---|
| 2026-08-10 | Queue behind EP-013 | Events must carry the provenance and transition semantics established there |
| 2026-08-11 | Activate after EP-013 completion | Durable invocation, approval, transition, receipt, and runtime execution semantics are now verified inputs to the canonical event contract |
| 2026-08-11 | Use migration `0012_interoperability_events.sql` | Migration numbers are append-only; the queued filename became stale after EP-013 added `0010` and `0011` |
| 2026-08-11 | Define the canonical envelope and typed payloads in L1 `cdm` without adding a dependency | Store, relay, and fake Nexus consumer share one provider-neutral contract while timestamps remain RFC3339 strings so L1 does not acquire runtime/time authority |
| 2026-08-11 | Use the exact semantic event type as the NATS subject | The fixed v1 taxonomy is non-PII, routes by event meaning, and avoids tenant/business/customer identifiers in subjects |
| 2026-08-11 | Persist one canonical event document transactionally in both `event_log` and `outbox` | Postgres remains authoritative, event identity is assigned once, and relay retries cannot manufacture a second logical event |
| 2026-08-11 | Add optional `external_binding_id` to durable invocation context with serde defaults | External events can attribute the active Nexus binding while legacy stored envelopes continue to deserialize |
| 2026-08-11 | Emit only normalized external transition events | Internal transition history remains complete, while unsupported or internal-only states do not leak an unstable ad hoc event taxonomy |
| 2026-08-11 | Enable the existing `async-nats` JetStream feature and record ADR-0023 | The pinned client already supplies stream management, `Nats-Msg-Id`, and publish acknowledgement; another messaging abstraction would add authority and dependency surface |
| 2026-08-11 | Lease outbox rows through Store and release SQL before broker I/O | This preserves INV-3, avoids long database locks, permits multiple relays with `SKIP LOCKED`, and makes crash recovery an at-least-once retry of the same event ID |
| 2026-08-11 | Park only invalid canonical rows; retry broker failures | Poison data remains authoritative and inspectable with a redacted reason, while transient infrastructure outages cannot silently discard events |
| 2026-08-11 | Persist a strict W3C carrier separately and add migration `0013` | Business correlation remains durable and authoritative; trace metadata can cross the asynchronous envelope boundary without entering `InvocationContext` or event payloads |
| 2026-08-11 | Do not add OpenTelemetry or accept baggage in this plan | Existing types provide interoperable W3C propagation without an unused exporter graph, and excluding baggage prevents incidental PII/secret propagation |
| 2026-08-11 | Share one typed event-status service between REST and readiness | Nexus-connected health cannot diverge between `/v1/nexus/events/status` and `/readyz`; standalone mode remains independent |
| 2026-08-11 | Expand the active file list for approval/idempotency/Executor trace paths | Async execution cannot be traced truthfully by editing only HTTP and relay files; ADR-0024 documents the anti-drift justification |
| 2026-08-11 | Use a durable pull consumer with explicit ack and a shared event-ID projection | Contract validation occurs before acknowledgement, consumer restart resumes broker position, and duplicate delivery cannot create a second logical projection |
| 2026-08-11 | Add `futures-util` as a Kernel test-only dependency | The fake Nexus harness needs the JetStream stream extension trait directly; the pinned crate already existed transitively and adds no production runtime authority |

## 15. Outcomes & Retrospective
Complete. M1 passed 4 contract tests covering schema validation, serde round-trip, exact v1 names and versions, typed event/payload matching, bridge origin references, and secret-shaped fixture scanning. M2 passed 3 focused tests covering canonical entity create/update/delete events, bridge origin references, envelope proposal/queue provenance, external binding and correlation/causation preservation, stable outbox event identity, and rollback of entity plus audit state when outbox append fails. M3 passed 5 focused relay tests, including an actual isolated JetStream publish acknowledgement and duplicate event-ID sequence check. M4 passed 3 focused tests covering strict W3C parse/fresh behavior, one trace ID with child spans through a real governed proposal/Executor/entity/outbox/relay path, durable business correlation kept outside trace metadata, and a real JetStream outage that fails Nexus-connected readiness while standalone readiness remains independent. Post-M4 full regression totals are Store 21, Fabric 131, and Kernel 32. M5's focused durable-consumer test passed against real JetStream, proving schema-before-ack validation, event-ID deduplication, correlation preservation, and resume through a recreated client bound to the same durable. SQLx metadata refresh, offline full-workspace compilation, cargo-deny, cargo-audit, formatting, patch whitespace, and plan-state checks passed. The unchanged full verifier exited zero in 973.5 seconds; because `scripts/verify.sh` uses `set -eu` and emits `verify: ok` only after every prescribed sub-script, its successful terminal path satisfies EP-014 acceptance. Known vendored SQLx `unexpected_cfgs` diagnostics and the policy-allowed transitive RustSec warnings remain documented; no production database, deployment, or external Nexus repository was used.
