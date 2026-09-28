-- Global providers are owned by root or administrators. Tenant users may use
-- those providers, their own providers, and providers explicitly granted to them.
ALTER TABLE users DROP COLUMN provider_access;

-- Existing users retain uninterrupted service during this policy rollout.
-- Future users default to prepaid-only access.
ALTER TABLE users
ADD COLUMN allow_debt INTEGER NOT NULL DEFAULT 0 CHECK(allow_debt IN (0,1));

UPDATE users SET allow_debt=1;
