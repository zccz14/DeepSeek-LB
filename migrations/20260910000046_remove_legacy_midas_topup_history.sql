DROP TABLE midas_topup_baselines;

DELETE FROM app_meta
WHERE key IN ('midas_agreement_id', 'midas_agreement_api_key');
