CREATE TABLE entity (
    id uuid PRIMARY KEY,
    kind text NOT NULL,
    tenant_id uuid NOT NULL,
    body jsonb NOT NULL,
    origin text NOT NULL DEFAULT 'native',
    origin_ref text,
    version bigint NOT NULL,
    deleted_at timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX entity_tenant_origin_origin_ref_unique
    ON entity (tenant_id, origin, origin_ref)
    WHERE origin_ref IS NOT NULL;

CREATE INDEX entity_tenant_kind_deleted_idx
    ON entity (tenant_id, kind, deleted_at);

CREATE INDEX entity_body_gin_idx
    ON entity
    USING gin (body jsonb_path_ops);

CREATE TABLE edge (
    src uuid NOT NULL REFERENCES entity(id) ON DELETE RESTRICT,
    rel text NOT NULL,
    dst uuid NOT NULL REFERENCES entity(id) ON DELETE RESTRICT,
    body jsonb NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (src, rel, dst)
);

-- revert: DROP TABLE edge; DROP INDEX entity_body_gin_idx; DROP INDEX entity_tenant_kind_deleted_idx; DROP INDEX entity_tenant_origin_origin_ref_unique; DROP TABLE entity;
