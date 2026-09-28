ALTER TABLE api_calls ADD COLUMN downstream_user_agent TEXT;
ALTER TABLE api_calls ADD COLUMN upstream_model TEXT;

INSERT INTO app_meta(key,value,updated_at)
VALUES('upstream_user_agent','',unixepoch());
