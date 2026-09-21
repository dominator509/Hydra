CREATE TABLE bridge_sync_schedule (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    adapter_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    interval_seconds BIGINT NOT NULL
        CHECK (interval_seconds BETWEEN 60 AND 86400),
    page_limit INTEGER NOT NULL
        CHECK (page_limit BETWEEN 1 AND 100),
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    next_due_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    lease_token UUID,
    lease_expires_at TIMESTAMPTZ,
    last_envelope_id UUID,
    last_started_at TIMESTAMPTZ,
    last_finished_at TIMESTAMPTZ,
    last_error TEXT
        CHECK (last_error IS NULL OR length(last_error) <= 1024),
    revision BIGINT NOT NULL DEFAULT 0 CHECK (revision >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, adapter_id, kind),
    FOREIGN KEY (tenant_id, adapter_id)
        REFERENCES bridge_adapter (tenant_id, adapter_id)
        ON DELETE RESTRICT,
    CHECK (length(adapter_id) BETWEEN 1 AND 128),
    CHECK (length(kind) BETWEEN 1 AND 128),
    CHECK (lease_expires_at IS NULL OR lease_token IS NOT NULL)
);

CREATE INDEX bridge_sync_schedule_due_idx
    ON bridge_sync_schedule (enabled, next_due_at, tenant_id)
    WHERE enabled = TRUE;

-- Schedule rows are disabled rather than deleted so operator history remains.
-- Revert: DROP INDEX bridge_sync_schedule_due_idx; DROP TABLE bridge_sync_schedule;
