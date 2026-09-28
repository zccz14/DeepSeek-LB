ALTER TABLE api_calls ADD COLUMN error_code TEXT;

CREATE INDEX api_calls_error_code_time_idx ON api_calls(error_code, created_at DESC)
WHERE error_code IS NOT NULL;
