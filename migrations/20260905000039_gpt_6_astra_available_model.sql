-- COMPATIBILITY: Existing installations may have administrator-edited
-- allowlists. Update only the exact former default; this migration is applied
-- once, so future releases must preserve its recorded checksum.
UPDATE app_meta
SET
    value = '["gpt-6-astra","gpt-5.6-sol","gpt-5.6-terra","gpt-5.6-luna","gpt-5.4","gpt-5.3-codex","gpt-5.4-mini","gpt-4o-transcribe","gpt-image-1","gpt-image-1.5","gpt-image-2"]',
    updated_at = unixepoch()
WHERE key = 'available_model_ids'
  AND value = '["gpt-5.6-sol","gpt-5.6-terra","gpt-5.6-luna","gpt-5.4","gpt-5.3-codex","gpt-5.4-mini","gpt-4o-transcribe","gpt-image-1","gpt-image-1.5","gpt-image-2"]';
