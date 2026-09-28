-- Providers and consumers remain durable records. Deleting either resource only
-- hides it from management, authentication, and scheduling surfaces.
ALTER TABLE consumers
ADD COLUMN is_deleted INTEGER NOT NULL DEFAULT 0 CHECK(is_deleted IN (0,1));

-- Revoked Consumers were already unavailable. Preserve that state rather than
-- allowing an old credential to become usable after this migration.
UPDATE consumers SET is_deleted=1 WHERE revoked_at IS NOT NULL;
ALTER TABLE consumers DROP COLUMN revoked_at;
CREATE INDEX IF NOT EXISTS consumers_visible_user_idx
ON consumers(user_id,is_system,is_deleted,created_at DESC);

ALTER TABLE providers
ADD COLUMN is_deleted INTEGER NOT NULL DEFAULT 0 CHECK(is_deleted IN (0,1));
CREATE INDEX IF NOT EXISTS providers_visible_created_idx
ON providers(is_deleted,created_at DESC,id);
CREATE INDEX IF NOT EXISTS providers_visible_owner_idx
ON providers(owner_id,is_deleted,created_at DESC);
