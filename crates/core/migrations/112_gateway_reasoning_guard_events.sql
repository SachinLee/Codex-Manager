CREATE TABLE IF NOT EXISTS gateway_reasoning_guard_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    trace_id TEXT,
    request_log_id INTEGER,
    mode TEXT NOT NULL,
    action TEXT NOT NULL,
    target_token INTEGER,
    source_kind TEXT,
    source_id TEXT,
    supplier_name TEXT,
    upstream_model TEXT,
    request_path TEXT,
    attempt_index INTEGER NOT NULL DEFAULT 0,
    final_status_code INTEGER,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_reasoning_guard_events_created_at
    ON gateway_reasoning_guard_events(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_reasoning_guard_events_source_created_at
    ON gateway_reasoning_guard_events(source_kind, source_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_reasoning_guard_events_trace_id
    ON gateway_reasoning_guard_events(trace_id);
