-- request_token_stats 需要先具备 actual_source_kind / actual_source_id 两列，
-- 才能从 request_logs 回填实际来源。历史库可能缺这两列（早期仅 request_logs 有）。
-- 若列已存在，ALTER 会以 "duplicate column name" 失败，由调用方的 compat fallback 兜底。
ALTER TABLE request_token_stats ADD COLUMN actual_source_kind TEXT;
ALTER TABLE request_token_stats ADD COLUMN actual_source_id TEXT;

UPDATE request_token_stats
SET
  actual_source_kind = (
    SELECT request_logs.actual_source_kind
    FROM request_logs
    WHERE request_logs.id = request_token_stats.request_log_id
  ),
  actual_source_id = (
    SELECT request_logs.actual_source_id
    FROM request_logs
    WHERE request_logs.id = request_token_stats.request_log_id
  )
WHERE request_log_id IS NOT NULL
  AND EXISTS (
    SELECT 1
    FROM request_logs
    WHERE request_logs.id = request_token_stats.request_log_id
      AND (
        request_logs.actual_source_kind IS NOT NULL
        OR request_logs.actual_source_id IS NOT NULL
      )
  );

CREATE INDEX IF NOT EXISTS idx_request_token_stats_actual_source_created_at
  ON request_token_stats(actual_source_kind, actual_source_id, created_at DESC);
