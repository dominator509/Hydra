CREATE TABLE event_log (
    seq bigserial PRIMARY KEY,
    tenant_id uuid NOT NULL,
    ts timestamptz NOT NULL DEFAULT now(),
    actor text NOT NULL,
    kind text NOT NULL,
    payload jsonb NOT NULL
);

CREATE INDEX event_log_tenant_ts_idx
    ON event_log (tenant_id, ts DESC);

REVOKE UPDATE, DELETE ON event_log FROM PUBLIC;

CREATE FUNCTION event_log_prevent_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'event_log is append-only';
END;
$$;

CREATE TRIGGER event_log_append_only
    BEFORE UPDATE OR DELETE ON event_log
    FOR EACH ROW
    EXECUTE FUNCTION event_log_prevent_mutation();

CREATE TABLE outbox (
    id bigserial PRIMARY KEY,
    event jsonb NOT NULL,
    published_at timestamptz
);

CREATE INDEX outbox_unpublished_idx
    ON outbox (id)
    WHERE published_at IS NULL;

-- revert: DROP INDEX outbox_unpublished_idx; DROP TABLE outbox; DROP TRIGGER event_log_append_only ON event_log; DROP FUNCTION event_log_prevent_mutation(); DROP INDEX event_log_tenant_ts_idx; DROP TABLE event_log;
