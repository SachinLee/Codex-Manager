CREATE TABLE IF NOT EXISTS request_token_daily_rollups (
  day_start_ts INTEGER NOT NULL,
  source_kind TEXT NOT NULL DEFAULT 'global',
  source_id TEXT NOT NULL DEFAULT '',
  input_tokens INTEGER NOT NULL DEFAULT 0,
  cached_input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  total_tokens INTEGER NOT NULL DEFAULT 0,
  reasoning_output_tokens INTEGER NOT NULL DEFAULT 0,
  estimated_cost_usd REAL NOT NULL DEFAULT 0.0,
  request_count INTEGER NOT NULL DEFAULT 0,
  success_count INTEGER NOT NULL DEFAULT 0,
  error_count INTEGER NOT NULL DEFAULT 0,
  max_duration_ms INTEGER,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (day_start_ts, source_kind, source_id)
);

CREATE INDEX IF NOT EXISTS idx_request_token_daily_rollups_source_day
ON request_token_daily_rollups(source_kind, source_id, day_start_ts);
