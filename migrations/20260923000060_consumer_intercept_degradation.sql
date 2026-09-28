ALTER TABLE consumers
ADD COLUMN intercept_degradation INTEGER NOT NULL DEFAULT 0 CHECK(intercept_degradation IN (0,1));
