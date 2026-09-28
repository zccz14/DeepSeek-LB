-- Every upstream provider declares the client identity (`originator`) it presents to OpenAI.
-- Providers created before this migration were all registered through the CodeX CLI OAuth flow,
-- so they keep `codex_cli_rs`.
ALTER TABLE providers ADD COLUMN originator TEXT NOT NULL DEFAULT 'codex_cli_rs';
