-- Aggregate API upstream protocol label for request logs (bounded observability).
-- NULL preserves legacy rows. Explicit values: responses | chat_completions | anthropic_messages
ALTER TABLE request_logs ADD COLUMN upstream_protocol TEXT;
