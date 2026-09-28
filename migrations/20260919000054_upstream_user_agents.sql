-- Keep a separate upstream User-Agent override for every supported downstream identity.
INSERT OR IGNORE INTO app_meta(key,value,updated_at) VALUES
    ('upstream_user_agent_codex_cli_rs','',unixepoch()),
    ('upstream_user_agent_pi','',unixepoch()),
    ('upstream_user_agent_opencode','',unixepoch());

-- Preserve the pre-identity single UA as the CodeX override during the transition.
UPDATE app_meta
SET value=(SELECT value FROM app_meta WHERE key='upstream_user_agent'), updated_at=unixepoch()
WHERE key='upstream_user_agent_codex_cli_rs'
  AND value=''
  AND EXISTS (SELECT 1 FROM app_meta WHERE key='upstream_user_agent' AND value<>'');
