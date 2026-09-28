-- Routing permission is independent of the immutable OAuth/upstream identity.
ALTER TABLE providers ADD COLUMN allow_other_originator INTEGER NOT NULL DEFAULT 0
    CHECK (allow_other_originator IN (0, 1));
ALTER TABLE api_calls ADD COLUMN downstream_originator TEXT;
ALTER TABLE api_calls ADD COLUMN upstream_originator TEXT;
ALTER TABLE api_calls ADD COLUMN originator_fallback_reason TEXT;
