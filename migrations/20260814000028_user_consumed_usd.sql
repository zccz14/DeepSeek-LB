ALTER TABLE users
ADD COLUMN consumed_usd_nanos INTEGER NOT NULL DEFAULT 0 CHECK(consumed_usd_nanos >= 0);

ALTER TABLE api_calls
ADD COLUMN consumed_usd_before_nanos INTEGER NOT NULL DEFAULT 0 CHECK(consumed_usd_before_nanos >= 0);

ALTER TABLE api_calls
ADD COLUMN consumed_usd_after_nanos INTEGER NOT NULL DEFAULT 0 CHECK(consumed_usd_after_nanos >= consumed_usd_before_nanos);

WITH running_calls AS (
    SELECT
        id,
        CASE
            WHEN COALESCE(cost_usd_nanos, 0) < 0 THEN 0
            ELSE COALESCE(cost_usd_nanos, 0)
        END AS cost_usd_nanos,
        COALESCE(
            SUM(
                CASE
                    WHEN COALESCE(cost_usd_nanos, 0) < 0 THEN 0
                    ELSE COALESCE(cost_usd_nanos, 0)
                END
            ) OVER (
                PARTITION BY user_id
                ORDER BY created_at, id
                ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
            ),
            0
        ) AS consumed_usd_after_nanos
    FROM api_calls
)
UPDATE api_calls
SET
    consumed_usd_before_nanos = running_calls.consumed_usd_after_nanos - running_calls.cost_usd_nanos,
    consumed_usd_after_nanos = running_calls.consumed_usd_after_nanos
FROM running_calls
WHERE api_calls.id = running_calls.id;

UPDATE users
SET consumed_usd_nanos = COALESCE(
    (
        SELECT SUM(
            CASE
                WHEN COALESCE(cost_usd_nanos, 0) < 0 THEN 0
                ELSE COALESCE(cost_usd_nanos, 0)
            END
        )
        FROM api_calls
        WHERE api_calls.user_id = users.id
    ),
    0
);
