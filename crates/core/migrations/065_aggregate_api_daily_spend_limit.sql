ALTER TABLE aggregate_apis ADD COLUMN daily_spend_limit_usd REAL;

UPDATE aggregate_apis
SET daily_spend_limit_usd = NULL
WHERE daily_spend_limit_usd IS NOT NULL AND daily_spend_limit_usd <= 0;
