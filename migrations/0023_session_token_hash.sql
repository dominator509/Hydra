ALTER TABLE hydra_session
    ALTER COLUMN token DROP NOT NULL;

ALTER TABLE hydra_session
    ADD COLUMN IF NOT EXISTS token_hash TEXT;

ALTER TABLE hydra_session
    ADD CONSTRAINT hydra_session_credential_check
    CHECK ((token IS NULL) <> (token_hash IS NULL));

ALTER TABLE hydra_session
    ADD CONSTRAINT hydra_session_token_hash_format_check
    CHECK (token_hash IS NULL OR token_hash ~ '^[0-9a-f]{64}$');

CREATE UNIQUE INDEX IF NOT EXISTS idx_hydra_session_token_hash
    ON hydra_session(token_hash)
    WHERE token_hash IS NOT NULL;

-- Existing sessions remain readable for one lookup so the Store can upgrade
-- them without retrieving every bearer token in a migration.
-- New sessions never populate the plaintext token column.

-- Revert: DROP INDEX IF EXISTS idx_hydra_session_token_hash;
-- ALTER TABLE hydra_session DROP CONSTRAINT IF EXISTS hydra_session_token_hash_format_check;
-- ALTER TABLE hydra_session DROP CONSTRAINT IF EXISTS hydra_session_credential_check;
-- ALTER TABLE hydra_session DROP COLUMN IF EXISTS token_hash;
-- ALTER TABLE hydra_session ALTER COLUMN token SET NOT NULL;
