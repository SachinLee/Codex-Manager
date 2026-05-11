ALTER TABLE request_token_stats ADD COLUMN aggregate_api_id TEXT;
ALTER TABLE request_token_stats ADD COLUMN aggregate_api_supplier_name TEXT;
ALTER TABLE request_token_stats ADD COLUMN aggregate_api_url TEXT;

CREATE INDEX IF NOT EXISTS idx_request_token_stats_aggregate_api_id_created_at
ON request_token_stats(aggregate_api_id, created_at DESC);
