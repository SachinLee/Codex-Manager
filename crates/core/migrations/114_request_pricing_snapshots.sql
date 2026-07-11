CREATE TABLE IF NOT EXISTS request_pricing_snapshots (
    request_log_id INTEGER PRIMARY KEY,
    billing_mode TEXT NOT NULL,
    context_band TEXT NOT NULL,
    long_context_threshold_tokens INTEGER,
    matched_rule_id TEXT,
    matched_pattern TEXT,
    price_source TEXT,
    match_quality TEXT,
    price_status TEXT NOT NULL,
    plain_input_cost_usd REAL,
    cached_input_cost_usd REAL,
    cache_write_cost_usd REAL,
    output_cost_usd REAL,
    total_cost_usd REAL,
    short_baseline_cost_usd REAL,
    long_context_uplift_usd REAL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (request_log_id) REFERENCES request_logs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_request_pricing_snapshots_context_band_created_at
    ON request_pricing_snapshots(context_band, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_request_pricing_snapshots_price_status_created_at
    ON request_pricing_snapshots(price_status, created_at DESC);
