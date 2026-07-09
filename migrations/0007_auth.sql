CREATE TABLE hydra_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,           -- argon2id encoded
    display_name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE hydra_session (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES hydra_user(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL,
    token TEXT NOT NULL UNIQUE,            -- random opaque session token
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_session_token ON hydra_session(token);

CREATE TABLE hydra_role (
    user_id UUID NOT NULL REFERENCES hydra_user(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('viewer','operator','approver','admin')),
    PRIMARY KEY (user_id, tenant_id)
);

-- Seed dev user (password = "hydra-dev", pre-computed argon2id hash)
INSERT INTO hydra_user (id, tenant_id, username, password_hash, display_name)
VALUES (
    '00000000-0000-0000-0000-000000000001',
    '00000000-0000-0000-0000-000000000001',
    'admin',
    '$argon2id$v=19$m=19456,t=2,p=1$dGVzdHNhbHRmb3JkZXZzZWVk$nw4dFA2YRXmBPZqFRqNZT8YOcDxVHIKGKEfnKFg5m9M',
    'Dev Admin'
)
ON CONFLICT (username) DO NOTHING;

INSERT INTO hydra_role (user_id, tenant_id, role)
VALUES ('00000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000001', 'admin')
ON CONFLICT DO NOTHING;

-- revert: DROP TABLE hydra_role; DROP TABLE hydra_session; DROP TABLE hydra_user;
