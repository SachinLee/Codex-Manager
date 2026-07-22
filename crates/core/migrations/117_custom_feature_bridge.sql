-- This bridge intentionally follows main's 112-116 model-catalog migrations.
-- Existing installations may already have the older custom migrations recorded;
-- the storage-level compatibility pass adds any missing columns idempotently.
CREATE TABLE IF NOT EXISTS request_pricing_snapshots (
    request_log_id INTEGER PRIMARY KEY,
    billing_mode TEXT NOT NULL,
    context_band TEXT NOT NULL,
    long_context_threshold_tokens INTEGER,
    long_context_threshold_inclusive INTEGER,
    matched_rule_id TEXT,
    matched_pattern TEXT,
    price_source TEXT,
    match_quality TEXT,
    price_status TEXT NOT NULL,
    cost_source TEXT,
    provider_cost_usd_ticks INTEGER,
    provider_cost_usd REAL,
    local_estimated_cost_usd REAL,
    pricing_variance_usd REAL,
    plain_input_cost_usd REAL,
    cached_input_cost_usd REAL,
    cache_write_cost_usd REAL,
    output_cost_usd REAL,
    total_cost_usd REAL,
    short_baseline_cost_usd REAL,
    long_context_uplift_usd REAL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_request_pricing_snapshots_context_band_created_at
    ON request_pricing_snapshots(context_band, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_request_pricing_snapshots_price_status_created_at
    ON request_pricing_snapshots(price_status, created_at DESC);

-- The preceding storage schema closure supplies columns that were skipped when
-- a database recorded the custom 113 migration.  Keep historical token rows
-- readable and make cache-write aggregation deterministic without changing
-- user-edited model prices.
UPDATE request_token_stats
SET cache_write_input_tokens = 0
WHERE cache_write_input_tokens IS NULL;

UPDATE request_token_stat_rollups
SET cache_write_input_tokens = 0
WHERE cache_write_input_tokens IS NULL;

UPDATE request_token_daily_rollups
SET cache_write_input_tokens = 0
WHERE cache_write_input_tokens IS NULL;
