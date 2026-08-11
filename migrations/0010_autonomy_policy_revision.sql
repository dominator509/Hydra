CREATE TABLE autonomy_policy_revision (
    tenant_id UUID PRIMARY KEY,
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO autonomy_policy_revision (tenant_id, revision)
SELECT DISTINCT tenant_id, 1
FROM autonomy_cell;

CREATE FUNCTION autonomy_policy_revision_bump()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    affected_tenant UUID;
BEGIN
    IF TG_OP = 'DELETE' THEN
        affected_tenant := OLD.tenant_id;
    ELSE
        affected_tenant := NEW.tenant_id;
    END IF;

    INSERT INTO autonomy_policy_revision (tenant_id, revision, updated_at)
    VALUES (affected_tenant, 1, now())
    ON CONFLICT (tenant_id) DO UPDATE
    SET revision = autonomy_policy_revision.revision + 1,
        updated_at = now();

    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER autonomy_policy_revision_changed
    AFTER INSERT OR UPDATE OR DELETE ON autonomy_cell
    FOR EACH ROW
    EXECUTE FUNCTION autonomy_policy_revision_bump();

-- Additive migration. Policy revisions provide exact tenant cache invalidation.
