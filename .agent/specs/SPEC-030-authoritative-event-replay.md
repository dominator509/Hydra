# SPEC-030 Authoritative Event Replay

Status: ACCEPTED for EP-050 implementation.

## Purpose

Provide a bounded recovery operation for rebuilding or catching up the
Hydra/Nexus JetStream delivery surface from the authoritative Postgres outbox.
Postgres audit/outbox state remains the source of truth; replay is delivery,
not a second CRM mutation path.

## Normative requirements

1. Replay reads only validated canonical outbox records through the Store
   repository. Kernel, CLI, Nexus, and NATS consumers must not issue arbitrary
   SQL.
2. Replay is explicitly invoked with `--replay-events`, requires
   `HYDRA_EVENT_REPLAY_CONFIRM=I_UNDERSTAND`, and is bounded to a positive
   configured batch limit no greater than 1000.
3. The cursor is an opaque monotonic outbox row ID supplied by the operator;
   the command returns the last replayed row ID without persisting a second
   cursor or changing canonical records.
4. Each publish uses the canonical event subject, serialized canonical event,
   original trace context where valid, and the stable event ID as the
   JetStream message ID. Replays may be delivered again and consumers must
   deduplicate by event ID.
5. A publish failure stops the batch and returns failure. The command must not
   report success for a partial batch or mark an outbox row published.
6. Event payloads and secrets are never logged. Output contains only bounded
   counts/cursor metadata and redacted failure codes.
7. The command requires explicit `DATABASE_URL` and `NATS_URL`; it must not
   use a development database, tenant, identity, or fallback broker.

## Acceptance

Focused Store and Kernel tests prove row ordering, limit bounds, stable event
IDs, trace preservation, confirmation/configuration failures, and no
canonical-row mutation. A disposable Postgres/NATS replay round trip passes,
the full verifier passes, and documentation keeps JetStream secondary to the
Postgres outbox.
