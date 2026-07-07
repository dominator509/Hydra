CREATE TABLE adapter_kv (
    adapter_id text NOT NULL,
    k text NOT NULL,
    v text NOT NULL,
    PRIMARY KEY (adapter_id, k)
);

CREATE TABLE autonomy_cell (
    tenant_id uuid NOT NULL,
    domain text NOT NULL,
    action text NOT NULL,
    kind text,
    kind_key text GENERATED ALWAYS AS (COALESCE(kind, '')) STORED,
    level text NOT NULL,
    cfg jsonb NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (tenant_id, domain, action, kind_key)
);

CREATE INDEX autonomy_cell_tenant_domain_idx
    ON autonomy_cell (tenant_id, domain);

-- revert: DROP INDEX autonomy_cell_tenant_domain_idx; DROP TABLE autonomy_cell; DROP TABLE adapter_kv;
