CREATE TABLE IF NOT EXISTS aggregate_api_health_configs (
  aggregate_api_id TEXT PRIMARY KEY REFERENCES aggregate_apis(id) ON DELETE CASCADE,
  enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
  probe_interval_secs INTEGER NOT NULL DEFAULT 900 CHECK (probe_interval_secs BETWEEN 60 AND 86400),
  probe_timeout_ms INTEGER NOT NULL DEFAULT 30000 CHECK (probe_timeout_ms BETWEEN 1000 AND 60000),
  probe_model TEXT,
  last_scheduled_at INTEGER,
  next_probe_at INTEGER,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS aggregate_api_health_states (
  aggregate_api_id TEXT NOT NULL REFERENCES aggregate_apis(id) ON DELETE CASCADE,
  upstream_model TEXT NOT NULL DEFAULT '',
  protocol TEXT NOT NULL DEFAULT '',
  state TEXT NOT NULL CHECK (state IN ('unknown', 'healthy', 'degraded', 'unhealthy', 'cooldown', 'recovering')),
  consecutive_failures INTEGER NOT NULL DEFAULT 0,
  consecutive_successes INTEGER NOT NULL DEFAULT 0,
  failure_threshold INTEGER NOT NULL DEFAULT 5,
  cooldown_until INTEGER,
  half_open_at INTEGER,
  last_observed_at INTEGER,
  last_probe_at INTEGER,
  last_success_at INTEGER,
  last_failure_at INTEGER,
  last_latency_ms INTEGER,
  last_http_status INTEGER,
  last_error_category TEXT,
  last_error_reason TEXT,
  last_observation_source TEXT,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (aggregate_api_id, upstream_model, protocol)
);

CREATE TABLE IF NOT EXISTS aggregate_api_health_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  aggregate_api_id TEXT NOT NULL REFERENCES aggregate_apis(id) ON DELETE CASCADE,
  upstream_model TEXT,
  protocol TEXT,
  trigger TEXT NOT NULL CHECK (trigger IN ('passive', 'scheduled_probe', 'manual_probe', 'half_open')),
  outcome TEXT NOT NULL CHECK (outcome IN ('success', 'failure', 'ignored')),
  state_before TEXT NOT NULL,
  state_after TEXT NOT NULL,
  error_category TEXT,
  http_status INTEGER,
  latency_ms INTEGER,
  reason TEXT,
  observed_at INTEGER NOT NULL,
  cooldown_until INTEGER
);

CREATE INDEX IF NOT EXISTS idx_aggregate_api_health_events_source_observed
  ON aggregate_api_health_events(aggregate_api_id, observed_at DESC);
CREATE INDEX IF NOT EXISTS idx_aggregate_api_health_events_model_observed
  ON aggregate_api_health_events(aggregate_api_id, upstream_model, observed_at DESC);
