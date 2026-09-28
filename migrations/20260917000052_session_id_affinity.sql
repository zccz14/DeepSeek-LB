ALTER TABLE api_calls ADD COLUMN session_id TEXT;

CREATE INDEX api_calls_session_time_idx
ON api_calls(session_id, created_at DESC);
