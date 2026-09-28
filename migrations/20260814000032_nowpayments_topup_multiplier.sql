ALTER TABLE payment_orders ADD COLUMN credit_multiplier_nanos INTEGER NOT NULL DEFAULT 1000000000 CHECK(credit_multiplier_nanos > 0);
CREATE INDEX payment_orders_nowpayments_uncredited_idx ON payment_orders(provider, credited_at, created_at);

INSERT OR IGNORE INTO app_meta(key,value,updated_at) VALUES
    ('nowpayments_topup_multiplier_nanos','1000000000',unixepoch());
