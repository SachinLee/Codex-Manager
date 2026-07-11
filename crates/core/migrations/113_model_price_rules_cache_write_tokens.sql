ALTER TABLE model_price_rules ADD COLUMN cache_write_price_per_1m REAL;
ALTER TABLE model_price_rules ADD COLUMN long_context_cache_write_price_per_1m REAL;

ALTER TABLE request_token_stats ADD COLUMN cache_write_input_tokens INTEGER;
ALTER TABLE request_token_stat_rollups ADD COLUMN cache_write_input_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE request_token_daily_rollups ADD COLUMN cache_write_input_tokens INTEGER NOT NULL DEFAULT 0;
