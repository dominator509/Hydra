ALTER TABLE event_log
    ADD COLUMN event_id uuid;

CREATE UNIQUE INDEX event_log_event_id_uq
    ON event_log (event_id)
    WHERE event_id IS NOT NULL;

ALTER TABLE outbox
    ADD COLUMN event_id uuid,
    ADD COLUMN subject text,
    ADD COLUMN created_at timestamptz NOT NULL DEFAULT now(),
    ADD COLUMN attempt_count integer NOT NULL DEFAULT 0,
    ADD COLUMN last_error text,
    ADD COLUMN parked_at timestamptz,
    ADD COLUMN jetstream_sequence bigint,
    ADD COLUMN claim_token uuid,
    ADD COLUMN claimed_at timestamptz,
    ADD COLUMN trace_context jsonb;

UPDATE outbox
SET event_id = gen_random_uuid(),
    subject = 'hydra.crm.legacy.v1'
WHERE event_id IS NULL OR subject IS NULL;

ALTER TABLE outbox
    ALTER COLUMN event_id SET NOT NULL,
    ALTER COLUMN subject SET NOT NULL,
    ADD CONSTRAINT outbox_attempt_count_nonnegative CHECK (attempt_count >= 0),
    ADD CONSTRAINT outbox_jetstream_sequence_positive
        CHECK (jetstream_sequence IS NULL OR jetstream_sequence > 0),
    ADD CONSTRAINT outbox_claim_pair
        CHECK ((claim_token IS NULL) = (claimed_at IS NULL));

CREATE UNIQUE INDEX outbox_event_id_uq
    ON outbox (event_id);

CREATE INDEX outbox_relay_pending_idx
    ON outbox (id)
    WHERE published_at IS NULL AND parked_at IS NULL;

-- revert: DROP INDEX outbox_relay_pending_idx; DROP INDEX outbox_event_id_uq; ALTER TABLE outbox DROP CONSTRAINT outbox_claim_pair, DROP CONSTRAINT outbox_jetstream_sequence_positive, DROP CONSTRAINT outbox_attempt_count_nonnegative, DROP COLUMN trace_context, DROP COLUMN claimed_at, DROP COLUMN claim_token, DROP COLUMN jetstream_sequence, DROP COLUMN parked_at, DROP COLUMN last_error, DROP COLUMN attempt_count, DROP COLUMN created_at, DROP COLUMN subject, DROP COLUMN event_id; DROP INDEX event_log_event_id_uq; ALTER TABLE event_log DROP COLUMN event_id;
