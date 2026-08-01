CREATE TEMP TABLE IF NOT EXISTS _gpt56_price_reduction_candidates (
  model_id TEXT PRIMARY KEY
);

DELETE FROM _gpt56_price_reduction_candidates;

INSERT INTO _gpt56_price_reduction_candidates(model_id)
SELECT m.id
FROM models m
JOIN model_prices p ON p.model_id = m.id
JOIN model_price_tiers base ON base.model_id = m.id AND base.min_input_tokens = 0
JOIN model_price_tiers long_tier ON long_tier.model_id = m.id AND long_tier.min_input_tokens = 272000
WHERE m.origin = 'builtin'
  AND m.user_edited = 0
  AND p.price_status = 'official'
  AND p.price_source = 'https://developers.openai.com/api/docs/models/compare'
  AND lower(m.slug) IN ('gpt-5.6-sol', 'gpt-5.6-terra', 'gpt-5.6-luna')
  AND p.input_microusd_per_1m = CASE lower(m.slug)
    WHEN 'gpt-5.6-sol' THEN 5000000
    WHEN 'gpt-5.6-terra' THEN 2500000
    WHEN 'gpt-5.6-luna' THEN 1000000
  END
  AND p.cached_input_microusd_per_1m = CASE lower(m.slug)
    WHEN 'gpt-5.6-sol' THEN 500000
    WHEN 'gpt-5.6-terra' THEN 250000
    WHEN 'gpt-5.6-luna' THEN 100000
  END
  AND p.output_microusd_per_1m = CASE lower(m.slug)
    WHEN 'gpt-5.6-sol' THEN 30000000
    WHEN 'gpt-5.6-terra' THEN 15000000
    WHEN 'gpt-5.6-luna' THEN 6000000
  END
  AND base.input_microusd_per_1m = p.input_microusd_per_1m
  AND base.cached_input_microusd_per_1m = p.cached_input_microusd_per_1m
  AND base.output_microusd_per_1m = p.output_microusd_per_1m
  AND long_tier.input_microusd_per_1m = CASE lower(m.slug)
    WHEN 'gpt-5.6-sol' THEN 10000000
    WHEN 'gpt-5.6-terra' THEN 5000000
    WHEN 'gpt-5.6-luna' THEN 2000000
  END
  AND long_tier.cached_input_microusd_per_1m = CASE lower(m.slug)
    WHEN 'gpt-5.6-sol' THEN 1000000
    WHEN 'gpt-5.6-terra' THEN 500000
    WHEN 'gpt-5.6-luna' THEN 200000
  END
  AND long_tier.output_microusd_per_1m = CASE lower(m.slug)
    WHEN 'gpt-5.6-sol' THEN 45000000
    WHEN 'gpt-5.6-terra' THEN 22500000
    WHEN 'gpt-5.6-luna' THEN 9000000
  END;

UPDATE model_price_tiers
SET input_microusd_per_1m = CASE lower((SELECT slug FROM models WHERE id = model_price_tiers.model_id))
      WHEN 'gpt-5.6-sol' THEN CASE min_input_tokens WHEN 0 THEN 5000000 ELSE 10000000 END
      WHEN 'gpt-5.6-terra' THEN CASE min_input_tokens WHEN 0 THEN 2000000 ELSE 4000000 END
      WHEN 'gpt-5.6-luna' THEN CASE min_input_tokens WHEN 0 THEN 200000 ELSE 400000 END
    END,
    cached_input_microusd_per_1m = CASE lower((SELECT slug FROM models WHERE id = model_price_tiers.model_id))
      WHEN 'gpt-5.6-sol' THEN CASE min_input_tokens WHEN 0 THEN 500000 ELSE 1000000 END
      WHEN 'gpt-5.6-terra' THEN CASE min_input_tokens WHEN 0 THEN 200000 ELSE 400000 END
      WHEN 'gpt-5.6-luna' THEN CASE min_input_tokens WHEN 0 THEN 20000 ELSE 40000 END
    END,
    output_microusd_per_1m = CASE lower((SELECT slug FROM models WHERE id = model_price_tiers.model_id))
      WHEN 'gpt-5.6-sol' THEN CASE min_input_tokens WHEN 0 THEN 30000000 ELSE 45000000 END
      WHEN 'gpt-5.6-terra' THEN CASE min_input_tokens WHEN 0 THEN 12000000 ELSE 18000000 END
      WHEN 'gpt-5.6-luna' THEN CASE min_input_tokens WHEN 0 THEN 1200000 ELSE 1800000 END
    END
WHERE model_id IN (SELECT model_id FROM _gpt56_price_reduction_candidates)
  AND min_input_tokens IN (0, 272000);

UPDATE model_prices
SET input_microusd_per_1m = CASE lower((SELECT slug FROM models WHERE id = model_prices.model_id))
      WHEN 'gpt-5.6-sol' THEN 5000000
      WHEN 'gpt-5.6-terra' THEN 2000000
      WHEN 'gpt-5.6-luna' THEN 200000
    END,
    cached_input_microusd_per_1m = CASE lower((SELECT slug FROM models WHERE id = model_prices.model_id))
      WHEN 'gpt-5.6-sol' THEN 500000
      WHEN 'gpt-5.6-terra' THEN 200000
      WHEN 'gpt-5.6-luna' THEN 20000
    END,
    output_microusd_per_1m = CASE lower((SELECT slug FROM models WHERE id = model_prices.model_id))
      WHEN 'gpt-5.6-sol' THEN 30000000
      WHEN 'gpt-5.6-terra' THEN 12000000
      WHEN 'gpt-5.6-luna' THEN 1200000
    END,
    updated_at = CAST(strftime('%s', 'now') AS INTEGER)
WHERE model_id IN (SELECT model_id FROM _gpt56_price_reduction_candidates);

UPDATE models
SET builtin_revision = MAX(COALESCE(builtin_revision, 0), 6),
    updated_at = CAST(strftime('%s', 'now') AS INTEGER)
WHERE id IN (SELECT model_id FROM _gpt56_price_reduction_candidates);

INSERT INTO model_catalog_v2_meta(key, value)
VALUES('gpt56_pricing_revision', '2026-07-31-price-reduction')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
