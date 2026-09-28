INSERT OR IGNORE INTO app_meta(key,value,updated_at) VALUES
    ('inject_image_generation','false',unixepoch()),
    ('inject_web_search','false',unixepoch());

ALTER TABLE consumers
ADD COLUMN inject_image_generation INTEGER NOT NULL DEFAULT 0 CHECK(inject_image_generation IN (0,1));

ALTER TABLE consumers
ADD COLUMN inject_web_search INTEGER NOT NULL DEFAULT 0 CHECK(inject_web_search IN (0,1));
