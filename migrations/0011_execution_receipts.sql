CREATE TABLE execution_receipt (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    envelope_id UUID NOT NULL,
    capability TEXT NOT NULL CHECK (capability <> ''),
    handler TEXT NOT NULL CHECK (handler <> ''),
    outcome TEXT NOT NULL CHECK (outcome IN ('verified', 'failed')),
    affected_targets UUID[] NOT NULL DEFAULT '{}',
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    invocation JSONB NOT NULL DEFAULT '{}'::jsonb,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, envelope_id),
    FOREIGN KEY (tenant_id, envelope_id)
        REFERENCES envelope (tenant_id, id)
        ON DELETE RESTRICT
);

CREATE INDEX execution_receipt_tenant_recorded_idx
    ON execution_receipt (tenant_id, recorded_at DESC, envelope_id);

CREATE TRIGGER execution_receipt_append_only
    BEFORE UPDATE OR DELETE ON execution_receipt
    FOR EACH ROW
    EXECUTE FUNCTION execution_provenance_prevent_mutation();

-- Additive migration. Execution receipts are immutable and tenant-scoped.
