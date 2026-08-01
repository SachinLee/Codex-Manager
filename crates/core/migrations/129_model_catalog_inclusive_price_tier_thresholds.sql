CREATE TEMP TABLE IF NOT EXISTS _inclusive_price_tier_candidates (
  model_id TEXT PRIMARY KEY
);

DELETE FROM _inclusive_price_tier_candidates;

INSERT INTO _inclusive_price_tier_candidates(model_id)
SELECT id
FROM models
WHERE origin = 'builtin'
  AND user_edited = 0
  AND lower(slug) IN ('gpt-5.4', 'gpt-5.5');

DELETE FROM model_price_tiers
WHERE min_input_tokens = 272000
  AND model_id IN (SELECT model_id FROM _inclusive_price_tier_candidates)
  AND EXISTS (
    SELECT 1
    FROM model_price_tiers existing
    WHERE existing.model_id = model_price_tiers.model_id
      AND existing.min_input_tokens = 272001
  );

UPDATE model_price_tiers
SET min_input_tokens = 272001
WHERE min_input_tokens = 272000
  AND model_id IN (SELECT model_id FROM _inclusive_price_tier_candidates);

INSERT INTO model_catalog_v2_meta(key, value)
VALUES('inclusive_price_tier_thresholds_revision', '2026-08-01')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
