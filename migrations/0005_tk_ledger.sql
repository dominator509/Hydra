CREATE TABLE tk_ledger (
    id bigserial PRIMARY KEY,
    ts timestamptz NOT NULL DEFAULT now(),
    tenant_id uuid NOT NULL,
    route text NOT NULL,
    provider text NOT NULL,
    prefix_sha text NOT NULL,
    hit_tokens integer NOT NULL,
    miss_tokens integer NOT NULL,
    out_tokens integer NOT NULL,
    out_bytes integer NOT NULL,
    aborted boolean NOT NULL DEFAULT false,
    cost_cents integer NOT NULL
);

CREATE INDEX tk_ledger_route_ts_idx
    ON tk_ledger (route, ts DESC);

CREATE INDEX tk_ledger_tenant_route_ts_idx
    ON tk_ledger (tenant_id, route, ts DESC);

-- revert: DROP INDEX tk_ledger_tenant_route_ts_idx; DROP INDEX tk_ledger_route_ts_idx; DROP TABLE tk_ledger;
