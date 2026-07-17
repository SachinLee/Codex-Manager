ALTER TABLE model_price_rules
    ADD COLUMN long_context_threshold_inclusive INTEGER NOT NULL DEFAULT 0;

ALTER TABLE request_pricing_snapshots
    ADD COLUMN long_context_threshold_inclusive INTEGER;
ALTER TABLE request_pricing_snapshots
    ADD COLUMN cost_source TEXT;
ALTER TABLE request_pricing_snapshots
    ADD COLUMN provider_cost_usd_ticks INTEGER;
ALTER TABLE request_pricing_snapshots
    ADD COLUMN provider_cost_usd REAL;
ALTER TABLE request_pricing_snapshots
    ADD COLUMN local_estimated_cost_usd REAL;
ALTER TABLE request_pricing_snapshots
    ADD COLUMN pricing_variance_usd REAL;
