CREATE TABLE external_tenant_binding (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider TEXT NOT NULL,
    external_tenant_id TEXT NOT NULL,
    external_business_id TEXT NOT NULL,
    hydra_tenant_id UUID NOT NULL,
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'disabled', 'revoked')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, external_tenant_id, external_business_id)
);

CREATE INDEX idx_external_tenant_binding_hydra_tenant
    ON external_tenant_binding (hydra_tenant_id, status);

-- Binding records are disabled or revoked in place. Hydra exposes no delete path.
