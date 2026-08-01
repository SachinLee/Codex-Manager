ALTER TABLE request_token_stat_rollups
  ADD COLUMN hourly_covered_through INTEGER NOT NULL DEFAULT 0;
