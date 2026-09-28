ALTER TABLE users
ADD COLUMN official_consumed_usd_nanos INTEGER NOT NULL DEFAULT 0 CHECK(official_consumed_usd_nanos >= 0);

ALTER TABLE api_calls
RENAME COLUMN consumed_usd_before_nanos TO actual_consumed_usd_before_nanos;

ALTER TABLE api_calls
RENAME COLUMN consumed_usd_after_nanos TO actual_consumed_usd_after_nanos;

ALTER TABLE api_calls
ADD COLUMN official_cost_usd_nanos INTEGER NOT NULL DEFAULT 0 CHECK(official_cost_usd_nanos >= 0);

ALTER TABLE api_calls
ADD COLUMN actual_cost_usd_nanos INTEGER NOT NULL DEFAULT 0 CHECK(actual_cost_usd_nanos >= 0);

ALTER TABLE api_calls
ADD COLUMN price_multiplier_nanos INTEGER NOT NULL DEFAULT 100000000 CHECK(price_multiplier_nanos > 0);

ALTER TABLE api_calls
ADD COLUMN official_consumed_usd_before_nanos INTEGER NOT NULL DEFAULT 0 CHECK(official_consumed_usd_before_nanos >= 0);

ALTER TABLE api_calls
ADD COLUMN official_consumed_usd_after_nanos INTEGER NOT NULL DEFAULT 0 CHECK(official_consumed_usd_after_nanos >= official_consumed_usd_before_nanos);

UPDATE api_calls
SET
    official_cost_usd_nanos = MAX(COALESCE(cost_usd_nanos, 0), 0),
    actual_cost_usd_nanos = MAX(COALESCE(cost_usd_nanos, 0), 0) / 10,
    price_multiplier_nanos = 100000000;

WITH running_calls AS (
    SELECT
        id,
        official_cost_usd_nanos,
        actual_cost_usd_nanos,
        COALESCE(
            SUM(official_cost_usd_nanos) OVER (
                PARTITION BY user_id
                ORDER BY created_at, id
                ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
            ),
            0
        ) AS official_after,
        COALESCE(
            SUM(actual_cost_usd_nanos) OVER (
                PARTITION BY user_id
                ORDER BY created_at, id
                ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
            ),
            0
        ) AS actual_after
    FROM api_calls
)
UPDATE api_calls
SET
    official_consumed_usd_before_nanos = running_calls.official_after - running_calls.official_cost_usd_nanos,
    official_consumed_usd_after_nanos = running_calls.official_after,
    actual_consumed_usd_before_nanos = running_calls.actual_after - running_calls.actual_cost_usd_nanos,
    actual_consumed_usd_after_nanos = running_calls.actual_after
FROM running_calls
WHERE api_calls.id = running_calls.id;

UPDATE users
SET
    official_consumed_usd_nanos = COALESCE(
        (
            SELECT SUM(official_cost_usd_nanos)
            FROM api_calls
            WHERE api_calls.user_id = users.id
        ),
        0
    ),
    consumed_usd_nanos = COALESCE(
        (
            SELECT SUM(actual_cost_usd_nanos)
            FROM api_calls
            WHERE api_calls.user_id = users.id
        ),
        0
    );

DELETE FROM credit_ledger_entries WHERE entry_kind = 'usage';

INSERT INTO credit_ledger_entries(id,user_id,entry_kind,amount_usd_nanos,api_call_id,created_at)
SELECT
    'usage:' || id,
    user_id,
    'usage',
    -actual_cost_usd_nanos,
    id,
    created_at
FROM api_calls
WHERE actual_cost_usd_nanos > 0;

INSERT OR IGNORE INTO app_meta(key,value,updated_at) VALUES
    ('model_price_multiplier_nanos','100000000',unixepoch());

ALTER TABLE api_calls DROP COLUMN cost_usd_nanos;
