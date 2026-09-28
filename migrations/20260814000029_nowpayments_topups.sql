CREATE TABLE payment_orders (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    provider TEXT NOT NULL CHECK(provider = 'nowpayments'),
    provider_payment_id TEXT UNIQUE,
    amount_usd_nanos INTEGER NOT NULL CHECK(amount_usd_nanos > 0),
    pay_currency TEXT NOT NULL,
    pay_address TEXT,
    pay_amount TEXT,
    payment_status TEXT NOT NULL,
    expires_at TEXT,
    credited_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX payment_orders_user_time_idx ON payment_orders(user_id, created_at DESC);

CREATE TABLE credit_ledger_entries (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    entry_kind TEXT NOT NULL CHECK(entry_kind IN ('topup', 'usage')),
    amount_usd_nanos INTEGER NOT NULL CHECK(amount_usd_nanos != 0),
    payment_order_id TEXT UNIQUE,
    api_call_id TEXT UNIQUE,
    created_at INTEGER NOT NULL,
    CHECK(
        (entry_kind = 'topup' AND amount_usd_nanos > 0 AND payment_order_id IS NOT NULL AND api_call_id IS NULL)
        OR
        (entry_kind = 'usage' AND amount_usd_nanos < 0 AND payment_order_id IS NULL AND api_call_id IS NOT NULL)
    )
);
CREATE INDEX credit_ledger_entries_user_time_idx ON credit_ledger_entries(user_id, created_at DESC);

INSERT INTO credit_ledger_entries(id,user_id,entry_kind,amount_usd_nanos,api_call_id,created_at)
SELECT
    'usage:' || id,
    user_id,
    'usage',
    -cost_usd_nanos,
    id,
    created_at
FROM api_calls
WHERE COALESCE(cost_usd_nanos, 0) > 0;

INSERT OR IGNORE INTO app_meta(key,value,updated_at) VALUES
    ('nowpayments_api_base','https://api.nowpayments.io/v1',unixepoch()),
    ('nowpayments_api_key','',unixepoch()),
    ('nowpayments_ipn_secret','',unixepoch()),
    ('nowpayments_ipn_callback_url','',unixepoch());
