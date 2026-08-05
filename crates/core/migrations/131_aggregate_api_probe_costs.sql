CREATE TABLE IF NOT EXISTS aggregate_api_probe_costs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  aggregate_api_id TEXT NOT NULL REFERENCES aggregate_apis(id) ON DELETE CASCADE,
  upstream_model TEXT NOT NULL DEFAULT '',
  trigger TEXT NOT NULL CHECK (trigger IN ('scheduled_probe', 'manual_probe', 'half_open')),
  outcome TEXT NOT NULL CHECK (outcome IN ('success', 'failure')),
  estimated_input_tokens INTEGER NOT NULL,
  estimated_output_tokens INTEGER NOT NULL,
  pricing_model TEXT NULL,
  price_source TEXT NULL,
  input_microusd_per_1m INTEGER NULL,
  output_microusd_per_1m INTEGER NULL,
  rate_multiplier_millis INTEGER NULL,
  estimated_cost_microusd INTEGER NULL,
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_aggregate_api_probe_costs_api_time
  ON aggregate_api_probe_costs(aggregate_api_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_aggregate_api_probe_costs_time
  ON aggregate_api_probe_costs(created_at DESC);
