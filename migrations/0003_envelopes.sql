CREATE TABLE envelope (
    id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL,
    state text NOT NULL,
    doc jsonb NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX envelope_tenant_state_updated_idx
    ON envelope (tenant_id, state, updated_at DESC);

CREATE TABLE envelope_transition (
    envelope_id uuid NOT NULL REFERENCES envelope(id) ON DELETE RESTRICT,
    ts timestamptz NOT NULL DEFAULT now(),
    from_state text NOT NULL,
    to_state text NOT NULL,
    actor text NOT NULL
);

CREATE INDEX envelope_transition_envelope_ts_idx
    ON envelope_transition (envelope_id, ts);

-- revert: DROP INDEX envelope_transition_envelope_ts_idx; DROP TABLE envelope_transition; DROP INDEX envelope_tenant_state_updated_idx; DROP TABLE envelope;
