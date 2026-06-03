ALTER TABLE request_logs ADD COLUMN conversation_anchor TEXT;

CREATE INDEX IF NOT EXISTS idx_request_logs_conversation_anchor_created_at
    ON request_logs(conversation_anchor, created_at DESC);
