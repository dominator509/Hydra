CREATE TABLE autonomy_freeze (
    tenant_id UUID PRIMARY KEY,
    status TEXT NOT NULL CHECK (status IN ('active', 'frozen')),
    reason TEXT,
    actor TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (
        (status = 'frozen' AND reason IS NOT NULL AND btrim(reason) <> '')
        OR status = 'active'
    )
);

-- A freeze is a policy change even when the autonomy matrix itself is unchanged.
-- Keep the existing revision cache authoritative for every Kernel instance.
-- revert: DROP TABLE autonomy_freeze;
