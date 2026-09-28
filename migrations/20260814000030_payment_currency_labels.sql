ALTER TABLE payment_orders ADD COLUMN pay_currency_label TEXT NOT NULL DEFAULT '';

UPDATE payment_orders
SET pay_currency_label = CASE pay_currency
    WHEN 'usdc' THEN 'USDC (ETH)'
    WHEN 'usdttrc20' THEN 'USDT (TRC20)'
    WHEN 'usdtbsc' THEN 'USDT (BSC)'
    ELSE UPPER(pay_currency)
END;

DELETE FROM app_meta WHERE key = 'nowpayments_ipn_callback_url';
