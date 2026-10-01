-- COMPATIBILITY: Existing installations may have administrator-edited limits.
-- Update only the exact former default (2 MiB) to the new default 48 MiB
-- (50331648 bytes); this migration is applied once, so future releases must
-- preserve its recorded checksum.
UPDATE app_meta
SET value = '50331648', updated_at = unixepoch()
WHERE key = 'response_body_limit' AND value = '2097152';
