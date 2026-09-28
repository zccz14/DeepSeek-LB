DROP TABLE credit_ledger_entries;
DROP TABLE payment_orders;

DELETE FROM app_meta
WHERE key IN (
    'nowpayments_api_base',
    'nowpayments_api_key',
    'nowpayments_ipn_secret',
    'nowpayments_topup_multiplier_nanos'
);

INSERT OR IGNORE INTO app_meta(key,value,updated_at) VALUES
    ('midas_api_base','https://midas.ntnl.io/api',unixepoch()),
    ('midas_agreement_id','',unixepoch()),
    ('midas_agreement_api_key','',unixepoch());
