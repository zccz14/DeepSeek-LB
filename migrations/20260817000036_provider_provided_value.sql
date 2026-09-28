ALTER TABLE users
ADD COLUMN provided_usd_nanos INTEGER NOT NULL DEFAULT 0 CHECK(provided_usd_nanos >= 0);

ALTER TABLE providers
ADD COLUMN official_provided_usd_nanos INTEGER NOT NULL DEFAULT 0 CHECK(official_provided_usd_nanos >= 0);

ALTER TABLE providers
ADD COLUMN actual_provided_usd_nanos INTEGER NOT NULL DEFAULT 0 CHECK(actual_provided_usd_nanos >= 0);

ALTER TABLE api_calls
ADD COLUMN official_provided_usd_before_nanos INTEGER NOT NULL DEFAULT 0 CHECK(official_provided_usd_before_nanos >= 0);

ALTER TABLE api_calls
ADD COLUMN official_provided_usd_after_nanos INTEGER NOT NULL DEFAULT 0 CHECK(official_provided_usd_after_nanos >= official_provided_usd_before_nanos);

ALTER TABLE api_calls
ADD COLUMN actual_provided_usd_before_nanos INTEGER NOT NULL DEFAULT 0 CHECK(actual_provided_usd_before_nanos >= 0);

ALTER TABLE api_calls
ADD COLUMN actual_provided_usd_after_nanos INTEGER NOT NULL DEFAULT 0 CHECK(actual_provided_usd_after_nanos >= actual_provided_usd_before_nanos);

WITH running_calls AS (
    SELECT
        id,
        MAX(official_cost_usd_nanos, 0) AS official_cost_usd_nanos,
        MAX(actual_cost_usd_nanos, 0) AS actual_cost_usd_nanos,
        COALESCE(
            SUM(MAX(official_cost_usd_nanos, 0)) OVER (
                PARTITION BY provider_id
                ORDER BY created_at, id
                ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
            ),
            0
        ) AS official_provided_usd_after_nanos,
        COALESCE(
            SUM(MAX(actual_cost_usd_nanos, 0)) OVER (
                PARTITION BY provider_id
                ORDER BY created_at, id
                ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
            ),
            0
        ) AS actual_provided_usd_after_nanos
    FROM api_calls
    WHERE provider_id IS NOT NULL
)
UPDATE api_calls
SET
    official_provided_usd_before_nanos = running_calls.official_provided_usd_after_nanos - running_calls.official_cost_usd_nanos,
    official_provided_usd_after_nanos = running_calls.official_provided_usd_after_nanos,
    actual_provided_usd_before_nanos = running_calls.actual_provided_usd_after_nanos - running_calls.actual_cost_usd_nanos,
    actual_provided_usd_after_nanos = running_calls.actual_provided_usd_after_nanos
FROM running_calls
WHERE api_calls.id = running_calls.id;

UPDATE providers
SET
    official_provided_usd_nanos = COALESCE((
        SELECT MAX(official_provided_usd_after_nanos)
        FROM api_calls
        WHERE api_calls.provider_id = providers.id
    ), 0),
    actual_provided_usd_nanos = COALESCE((
        SELECT MAX(actual_provided_usd_after_nanos)
        FROM api_calls
        WHERE api_calls.provider_id = providers.id
    ), 0);

UPDATE users
SET provided_usd_nanos = COALESCE((
    SELECT SUM(actual_provided_usd_nanos)
    FROM providers
    WHERE providers.owner_id = users.id
), 0);
