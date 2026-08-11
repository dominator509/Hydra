ALTER TABLE envelope
    ADD COLUMN revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0);

CREATE UNIQUE INDEX envelope_tenant_id_unique
    ON envelope (tenant_id, id);

ALTER TABLE envelope_transition
    ADD COLUMN tenant_id UUID,
    ADD COLUMN invocation JSONB NOT NULL DEFAULT '{}'::jsonb;

UPDATE envelope_transition AS transition
SET tenant_id = envelope.tenant_id
FROM envelope
WHERE transition.envelope_id = envelope.id;

ALTER TABLE envelope_transition
    ALTER COLUMN tenant_id SET NOT NULL,
    ADD CONSTRAINT envelope_transition_tenant_envelope_fk
        FOREIGN KEY (tenant_id, envelope_id)
        REFERENCES envelope (tenant_id, id)
        ON DELETE RESTRICT;

CREATE INDEX envelope_transition_tenant_ts_idx
    ON envelope_transition (tenant_id, ts, envelope_id);

CREATE TABLE idempotency_record (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    origin_system TEXT NOT NULL CHECK (origin_system <> ''),
    idempotency_key TEXT NOT NULL CHECK (idempotency_key <> ''),
    capability TEXT NOT NULL CHECK (capability <> ''),
    request_hash TEXT NOT NULL CHECK (request_hash ~ '^[0-9a-f]{64}$'),
    envelope_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, origin_system, idempotency_key, capability),
    FOREIGN KEY (tenant_id, envelope_id)
        REFERENCES envelope (tenant_id, id)
        ON DELETE RESTRICT
);

CREATE INDEX idempotency_record_envelope_idx
    ON idempotency_record (tenant_id, envelope_id);

CREATE TABLE approval_assertion (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    envelope_id UUID NOT NULL,
    human_actor_id TEXT NOT NULL CHECK (human_actor_id <> ''),
    delegated_by TEXT NOT NULL CHECK (delegated_by <> ''),
    authentication_strength TEXT NOT NULL CHECK (authentication_strength <> ''),
    approved_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    request_id TEXT,
    correlation_id TEXT,
    objective_id TEXT,
    task_id TEXT,
    decision TEXT NOT NULL CHECK (decision IN ('approved', 'rejected')),
    comment TEXT CHECK (comment IS NULL OR length(comment) <= 2000),
    FOREIGN KEY (tenant_id, envelope_id)
        REFERENCES envelope (tenant_id, id)
        ON DELETE RESTRICT
);

CREATE INDEX approval_assertion_envelope_idx
    ON approval_assertion (tenant_id, envelope_id, approved_at DESC);

CREATE FUNCTION execution_provenance_prevent_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION '% is append-only', TG_TABLE_NAME;
END;
$$;

CREATE TRIGGER idempotency_record_append_only
    BEFORE UPDATE OR DELETE ON idempotency_record
    FOR EACH ROW
    EXECUTE FUNCTION execution_provenance_prevent_mutation();

CREATE TRIGGER approval_assertion_append_only
    BEFORE UPDATE OR DELETE ON approval_assertion
    FOR EACH ROW
    EXECUTE FUNCTION execution_provenance_prevent_mutation();

-- Additive migration. Approval and idempotency records are intentionally immutable.
