-- 137_aggregate_api_daily_spend.sql
-- Additive durable per-API/per-local-day spend budget used by Aggregate API
-- daily enforcement. Existing charge snapshots, request logs, guard events,
-- and rollups are never rewritten.

CREATE TABLE IF NOT EXISTS aggregate_api_daily_spend_buckets (
  aggregate_api_id TEXT NOT NULL REFERENCES aggregate_apis(id) ON DELETE CASCADE,
  day_start_ts INTEGER NOT NULL,
  opening_spend_microusd INTEGER NOT NULL,
  settled_spend_microusd INTEGER NOT NULL DEFAULT 0,
  reserved_spend_microusd INTEGER NOT NULL DEFAULT 0,
  held_spend_microusd INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (aggregate_api_id, day_start_ts)
);

CREATE TABLE IF NOT EXISTS aggregate_api_daily_spend_attempts (
  id TEXT PRIMARY KEY,
  aggregate_api_id TEXT NOT NULL REFERENCES aggregate_apis(id) ON DELETE CASCADE,
  day_start_ts INTEGER NOT NULL,
  trace_id TEXT,
  attempt_kind TEXT NOT NULL CHECK (attempt_kind IN (
    'initial','transport_retry','capacity_retry','guard_retry','continuation_recovery'
  )),
  state TEXT NOT NULL CHECK (state IN ('reserved','settled','released','held')),
  pricing_state TEXT NOT NULL CHECK (pricing_state IN (
    'quoted','unbounded_output','unpriced_model','provider_reported'
  )),
  reserved_microusd INTEGER NOT NULL DEFAULT 0,
  settled_microusd INTEGER,
  request_log_id INTEGER,
  created_at INTEGER NOT NULL,
  resolved_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_aggregate_api_daily_spend_attempts_api_day_state
  ON aggregate_api_daily_spend_attempts(aggregate_api_id, day_start_ts, state);
CREATE INDEX IF NOT EXISTS idx_aggregate_api_daily_spend_attempts_resolved_at
  ON aggregate_api_daily_spend_attempts(resolved_at);
