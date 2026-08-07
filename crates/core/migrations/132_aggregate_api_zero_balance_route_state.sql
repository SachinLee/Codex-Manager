CREATE TABLE IF NOT EXISTS aggregate_api_zero_balance_route_states (
  aggregate_api_id TEXT PRIMARY KEY REFERENCES aggregate_apis(id) ON DELETE CASCADE,
  state TEXT NOT NULL CHECK (state IN ('zero_balance_blocked', 'manually_released')),
  observed_at INTEGER NOT NULL,
  released_at INTEGER,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_aggregate_api_zero_balance_route_states_state
  ON aggregate_api_zero_balance_route_states(state, aggregate_api_id);
