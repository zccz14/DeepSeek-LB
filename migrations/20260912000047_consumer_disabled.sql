ALTER TABLE consumers
ADD COLUMN is_disabled INTEGER NOT NULL DEFAULT 0 CHECK(is_disabled IN (0,1));
