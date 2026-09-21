CREATE TABLE bridge_sync_state (
    tenant_id UUID NOT NULL,
    adapter_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    cursor TEXT NOT NULL DEFAULT '',
    revision BIGINT NOT NULL DEFAULT 0 CHECK (revision >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, adapter_id, kind),
    FOREIGN KEY (tenant_id, adapter_id)
        REFERENCES bridge_adapter (tenant_id, adapter_id)
        ON DELETE RESTRICT,
    CHECK (length(adapter_id) BETWEEN 1 AND 128),
    CHECK (length(kind) BETWEEN 1 AND 128),
    CHECK (length(cursor) <= 2048)
);

CREATE TABLE bridge_sync_run (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    adapter_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    start_cursor TEXT NOT NULL,
    next_cursor TEXT,
    status TEXT NOT NULL DEFAULT 'running'
        CHECK (status IN ('running', 'succeeded', 'failed')),
    applied_upserts INTEGER NOT NULL DEFAULT 0 CHECK (applied_upserts >= 0),
    applied_deletes INTEGER NOT NULL DEFAULT 0 CHECK (applied_deletes >= 0),
    conflict_count INTEGER NOT NULL DEFAULT 0 CHECK (conflict_count >= 0),
    correlation_id TEXT,
    causation_id TEXT,
    envelope_id UUID,
    error TEXT,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ,
    FOREIGN KEY (tenant_id, adapter_id)
        REFERENCES bridge_adapter (tenant_id, adapter_id)
        ON DELETE RESTRICT,
    CHECK (length(adapter_id) BETWEEN 1 AND 128),
    CHECK (length(kind) BETWEEN 1 AND 128),
    CHECK (length(start_cursor) <= 2048),
    CHECK (next_cursor IS NULL OR length(next_cursor) <= 2048),
    CHECK (error IS NULL OR length(error) <= 1024)
);

CREATE UNIQUE INDEX bridge_sync_run_active_unique
    ON bridge_sync_run (tenant_id, adapter_id, kind)
    WHERE status = 'running';

CREATE INDEX bridge_sync_run_tenant_lookup_idx
    ON bridge_sync_run (tenant_id, adapter_id, kind, started_at DESC);

CREATE TABLE bridge_sync_conflict (
    id BIGSERIAL PRIMARY KEY,
    tenant_id UUID NOT NULL,
    run_id UUID NOT NULL,
    adapter_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    external_ref TEXT NOT NULL,
    operation TEXT NOT NULL CHECK (operation IN ('upserted', 'deleted')),
    conflict_kind TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (run_id) REFERENCES bridge_sync_run (id) ON DELETE RESTRICT,
    CHECK (length(adapter_id) BETWEEN 1 AND 128),
    CHECK (length(kind) BETWEEN 1 AND 128),
    CHECK (length(external_ref) BETWEEN 1 AND 512),
    CHECK (length(conflict_kind) BETWEEN 1 AND 128),
    CHECK (length(reason) BETWEEN 1 AND 1024)
);

CREATE INDEX bridge_sync_conflict_tenant_lookup_idx
    ON bridge_sync_conflict (tenant_id, adapter_id, kind, created_at DESC);

-- Sync state is additive and does not backfill historical adapter rows.
-- Revert: DROP INDEX bridge_sync_conflict_tenant_lookup_idx; DROP TABLE bridge_sync_conflict; DROP INDEX bridge_sync_run_tenant_lookup_idx; DROP INDEX bridge_sync_run_active_unique; DROP TABLE bridge_sync_run; DROP TABLE bridge_sync_state;
