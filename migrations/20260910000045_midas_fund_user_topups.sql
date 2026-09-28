CREATE TABLE midas_topup_baselines (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE RESTRICT,
    amount_usd_nanos INTEGER NOT NULL CHECK(amount_usd_nanos >= 0),
    created_at INTEGER NOT NULL
);

INSERT OR IGNORE INTO app_meta(key,value,updated_at) VALUES
    ('midas_fund_user_id','',unixepoch()),
    ('midas_fund_api_key','',unixepoch());
