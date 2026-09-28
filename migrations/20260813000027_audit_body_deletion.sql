ALTER TABLE request_archives
ADD COLUMN bodies_deleted INTEGER NOT NULL DEFAULT 0 CHECK(bodies_deleted IN (0,1));
