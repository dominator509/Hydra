CREATE TABLE a2a_task (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    context_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    workflow TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    requester_principal_id TEXT NOT NULL,
    requester_principal_type TEXT NOT NULL,
    correlation_id TEXT NOT NULL,
    causation_id TEXT,
    objective_id TEXT,
    input JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'submitted'
        CHECK (status IN ('submitted', 'working', 'completed', 'canceled', 'failed', 'rejected')),
    artifact JSONB,
    history JSONB NOT NULL DEFAULT '[]'::jsonb,
    revision BIGINT NOT NULL DEFAULT 0 CHECK (revision >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, message_id)
);

CREATE INDEX a2a_task_tenant_status_idx
    ON a2a_task (tenant_id, status, updated_at DESC);

CREATE INDEX a2a_task_tenant_context_idx
    ON a2a_task (tenant_id, context_id, updated_at DESC);

-- A2A tasks are durable workflow metadata, not a second CRM source of truth.
-- revert: DROP INDEX a2a_task_tenant_context_idx; DROP INDEX a2a_task_tenant_status_idx; DROP TABLE a2a_task;
