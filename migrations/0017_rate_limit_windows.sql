CREATE TABLE rate_limit_window (
    key_digest TEXT PRIMARY KEY
        CHECK (length(key_digest) = 64),
    window_started_at TIMESTAMPTZ NOT NULL,
    request_count BIGINT NOT NULL
        CHECK (request_count >= 0)
);

CREATE INDEX rate_limit_window_started_idx
    ON rate_limit_window (window_started_at);

-- revert: DROP INDEX IF EXISTS rate_limit_window_started_idx; DROP TABLE IF EXISTS rate_limit_window;
