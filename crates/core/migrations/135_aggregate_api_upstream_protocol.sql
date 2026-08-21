-- Aggregate API upstream protocol declaration.
-- Nullable: NULL preserves legacy client-dependent behavior (compatible -> Responses for
-- Responses clients / Messages for Anthropic clients, codex -> Responses).
-- Explicit values: 'responses' | 'chat_completions' for codex/compatible candidates only.
ALTER TABLE aggregate_apis ADD COLUMN upstream_protocol TEXT;
