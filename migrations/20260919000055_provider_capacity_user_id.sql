ALTER TABLE provider_capacity_snapshots ADD COLUMN upstream_user_id TEXT;

-- Existing history has no upstream identity, so it cannot be corrected safely.
DELETE FROM provider_capacity_history;
