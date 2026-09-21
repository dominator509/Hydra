CREATE TABLE tenant_adapter_kv (
    tenant_id UUID NOT NULL,
    adapter_id TEXT NOT NULL,
    k TEXT NOT NULL,
    v TEXT NOT NULL,
    PRIMARY KEY (tenant_id, adapter_id, k)
);

CREATE INDEX tenant_adapter_kv_tenant_adapter_idx
    ON tenant_adapter_kv (tenant_id, adapter_id);

-- Revert: DROP INDEX tenant_adapter_kv_tenant_adapter_idx; DROP TABLE tenant_adapter_kv;
