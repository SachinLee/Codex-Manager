-- Older gateway versions created charge snapshots for every upstream response,
-- even though only 2xx responses (and client-aborted 499 deliveries) are billable.
DELETE FROM request_pricing_snapshots
WHERE request_log_id IN (
    SELECT id
    FROM request_logs
    WHERE status_code NOT BETWEEN 200 AND 299 AND status_code <> 499
);

DELETE FROM request_charge_snapshots
WHERE request_log_id IN (
    SELECT id
    FROM request_logs
    WHERE status_code NOT BETWEEN 200 AND 299 AND status_code <> 499
);

UPDATE request_token_stats
SET estimated_cost_usd = NULL
WHERE request_log_id IN (
    SELECT id
    FROM request_logs
    WHERE status_code NOT BETWEEN 200 AND 299 AND status_code <> 499
);
