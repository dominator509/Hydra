ALTER TABLE bridge_adapter
    ADD COLUMN config JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(config) = 'object');

-- Revert: ALTER TABLE bridge_adapter DROP COLUMN config;
