ALTER TABLE hydra_user
    ADD COLUMN IF NOT EXISTS auth_source TEXT NOT NULL DEFAULT 'operator';

ALTER TABLE hydra_user
    ADD COLUMN IF NOT EXISTS disabled_at TIMESTAMPTZ;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'hydra_user_auth_source_check'
          AND conrelid = 'hydra_user'::regclass
    ) THEN
        ALTER TABLE hydra_user
            ADD CONSTRAINT hydra_user_auth_source_check
            CHECK (auth_source IN ('operator', 'development_seed'));
    END IF;
END
$$;

UPDATE hydra_user
SET auth_source = 'development_seed',
    disabled_at = COALESCE(disabled_at, now())
WHERE id = '00000000-0000-0000-0000-000000000001'
  AND username = 'admin';

CREATE INDEX IF NOT EXISTS idx_hydra_user_auth_source
    ON hydra_user (auth_source, disabled_at);

-- revert: DROP INDEX IF EXISTS idx_hydra_user_auth_source; ALTER TABLE hydra_user DROP CONSTRAINT IF EXISTS hydra_user_auth_source_check; ALTER TABLE hydra_user DROP COLUMN IF EXISTS disabled_at; ALTER TABLE hydra_user DROP COLUMN IF EXISTS auth_source;
