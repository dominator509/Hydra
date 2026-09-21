CREATE TABLE bridge_adapter (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    adapter_id TEXT NOT NULL,
    component_ref TEXT NOT NULL,
    component_sha256 TEXT NOT NULL
        CHECK (component_sha256 ~ '^[0-9a-f]{64}$'),
    grant_config JSONB NOT NULL
        CHECK (jsonb_typeof(grant_config) = 'object'),
    descriptor JSONB,
    state TEXT NOT NULL DEFAULT 'inactive'
        CHECK (state IN ('inactive', 'activating', 'active', 'paused', 'failed')),
    last_error TEXT,
    revision BIGINT NOT NULL DEFAULT 0 CHECK (revision >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, adapter_id)
);

CREATE INDEX bridge_adapter_tenant_state_idx
    ON bridge_adapter (tenant_id, state, updated_at DESC);

CREATE TABLE bridge_adapter_transition (
    id BIGSERIAL PRIMARY KEY,
    tenant_id UUID NOT NULL,
    adapter_id TEXT NOT NULL,
    revision BIGINT NOT NULL CHECK (revision >= 0),
    from_state TEXT,
    to_state TEXT NOT NULL,
    event JSONB NOT NULL CHECK (jsonb_typeof(event) = 'object'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, adapter_id, revision),
    FOREIGN KEY (tenant_id, adapter_id)
        REFERENCES bridge_adapter (tenant_id, adapter_id)
        ON DELETE RESTRICT
);

CREATE INDEX bridge_adapter_transition_tenant_adapter_idx
    ON bridge_adapter_transition (tenant_id, adapter_id, revision);

-- Bridge metadata and lifecycle history are not CRM records and do not grant
-- adapter authority. The registry remains tenant-scoped and append-audited.
-- revert: DROP INDEX bridge_adapter_transition_tenant_adapter_idx; DROP TABLE bridge_adapter_transition; DROP INDEX bridge_adapter_tenant_state_idx; DROP TABLE bridge_adapter;
