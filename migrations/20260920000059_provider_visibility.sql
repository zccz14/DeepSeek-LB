-- A provider is either public (any Consumer may route to it) or private (only Consumers
-- owned by the provider's owner may route to it). New providers start private.
ALTER TABLE providers
ADD COLUMN visibility TEXT NOT NULL DEFAULT 'private' CHECK(visibility IN ('public','private'));

-- Providers registered before this migration were pooled globally for every Consumer.
-- Keeping them public preserves that reach; owners can switch them to private explicitly.
UPDATE providers SET visibility='public';
