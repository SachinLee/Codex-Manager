ALTER TABLE aggregate_apis ADD COLUMN cost_multiplier REAL NOT NULL DEFAULT 1.0;

UPDATE aggregate_apis
SET cost_multiplier = 1.0
WHERE cost_multiplier IS NULL OR cost_multiplier <= 0;
