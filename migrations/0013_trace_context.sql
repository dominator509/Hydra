ALTER TABLE envelope
    ADD COLUMN trace_context jsonb;

ALTER TABLE envelope_transition
    ADD COLUMN trace_context jsonb;

-- revert: ALTER TABLE envelope_transition DROP COLUMN trace_context; ALTER TABLE envelope DROP COLUMN trace_context;
