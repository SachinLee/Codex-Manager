CREATE TABLE IF NOT EXISTS gateway_capability_overrides (
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    upstream_model_pattern TEXT NOT NULL DEFAULT '*',
    protocol TEXT NOT NULL DEFAULT '*',
    capability_key TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('supported', 'unsupported')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (
        source_kind, source_id, upstream_model_pattern, protocol, capability_key
    )
);

CREATE TABLE IF NOT EXISTS gateway_capability_observations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    upstream_model_pattern TEXT NOT NULL DEFAULT '*',
    protocol TEXT NOT NULL DEFAULT '*',
    capability_key TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('supported', 'unsupported')),
    observation_source TEXT NOT NULL CHECK (observation_source IN ('runtime', 'probe')),
    confidence TEXT NOT NULL CHECK (confidence IN ('high', 'medium', 'low')),
    evidence_code TEXT NOT NULL,
    first_observed_at INTEGER NOT NULL,
    last_observed_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    occurrence_count INTEGER NOT NULL DEFAULT 1,
    UNIQUE (
        source_kind, source_id, upstream_model_pattern, protocol, capability_key,
        state, observation_source, evidence_code
    )
);

CREATE INDEX IF NOT EXISTS idx_gateway_capability_observation_resolve
ON gateway_capability_observations (
    source_kind, source_id, capability_key, expires_at DESC,
    upstream_model_pattern, protocol
);

CREATE INDEX IF NOT EXISTS idx_gateway_capability_observation_expiry
ON gateway_capability_observations (expires_at);

CREATE TABLE IF NOT EXISTS gateway_upstream_attempt_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    trace_id TEXT NOT NULL,
    request_log_id INTEGER,
    attempt_index INTEGER NOT NULL,
    phase TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    supplier_name TEXT,
    upstream_model TEXT,
    protocol TEXT NOT NULL,
    request_path TEXT NOT NULL,
    contract_signature TEXT NOT NULL,
    capability_decisions_json TEXT NOT NULL DEFAULT '[]',
    transform_codes_json TEXT NOT NULL DEFAULT '[]',
    error_class TEXT,
    error_code TEXT,
    http_status INTEGER,
    duration_ms INTEGER,
    outcome TEXT NOT NULL,
    delivery_started INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_gateway_upstream_attempt_events_trace
ON gateway_upstream_attempt_events (trace_id, attempt_index, id);

CREATE INDEX IF NOT EXISTS idx_gateway_upstream_attempt_events_source_created
ON gateway_upstream_attempt_events (source_kind, source_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_gateway_upstream_attempt_events_created
ON gateway_upstream_attempt_events (created_at DESC);
