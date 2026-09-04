CREATE TABLE IF NOT EXISTS aggregate_api_bindings (
    platform_key_hash TEXT NOT NULL,
    protocol_type TEXT NOT NULL,
    model TEXT NOT NULL,
    cache_affinity_route_id_hash TEXT NOT NULL,
    bound_aggregate_api_id TEXT NOT NULL,
    bound_at INTEGER NOT NULL,
    reason TEXT,
    PRIMARY KEY (platform_key_hash, protocol_type, model, cache_affinity_route_id_hash)
);

CREATE INDEX IF NOT EXISTS idx_aggregate_api_bindings_cleanup
    ON aggregate_api_bindings(bound_at DESC);
