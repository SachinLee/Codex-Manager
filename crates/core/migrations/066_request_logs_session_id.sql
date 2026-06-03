ALTER TABLE request_logs ADD COLUMN session_id TEXT;

CREATE INDEX IF NOT EXISTS idx_request_logs_session_id_created_at
  ON request_logs(session_id, created_at DESC);
