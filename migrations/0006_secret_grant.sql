CREATE TABLE secret_grant (
    adapter_id text PRIMARY KEY,
    origins text[] NOT NULL,
    secret_names text[] NOT NULL,
    dsn_name text,
    fuel bigint NOT NULL CHECK (fuel > 0)
);

-- revert: DROP TABLE secret_grant;
