from pathlib import Path
path = Path(r"crates/core/src/storage/request_token_stats.rs")
text = path.read_text(encoding="utf-8")
old = '''        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_stats_account_model_created_at
             ON request_token_stats(account_id, model, created_at DESC)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_stats_aggregate_api_id_created_at
             ON request_token_stats(aggregate_api_id, created_at DESC)",
            [],
        )?;
        self.ensure_column("request_token_stats", "total_tokens", "INTEGER")?;
        self.ensure_column("request_token_stats", "cache_write_input_tokens", "INTEGER")?;
        self.ensure_column("request_token_stats", "aggregate_api_id", "TEXT")?;
        self.ensure_column("request_token_stats", "aggregate_api_supplier_name", "TEXT")?;
        self.ensure_column("request_token_stats", "aggregate_api_url", "TEXT")?;
        self.ensure_column("request_token_stats", "actual_source_kind", "TEXT")?;
        self.ensure_column("request_token_stats", "actual_source_id", "TEXT")?;
'''
new = '''        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_stats_account_model_created_at
             ON request_token_stats(account_id, model, created_at DESC)",
            [],
        )?;
        self.ensure_column("request_token_stats", "total_tokens", "INTEGER")?;
        self.ensure_column("request_token_stats", "cache_write_input_tokens", "INTEGER")?;
        self.ensure_column("request_token_stats", "aggregate_api_id", "TEXT")?;
        self.ensure_column("request_token_stats", "aggregate_api_supplier_name", "TEXT")?;
        self.ensure_column("request_token_stats", "aggregate_api_url", "TEXT")?;
        self.ensure_column("request_token_stats", "actual_source_kind", "TEXT")?;
        self.ensure_column("request_token_stats", "actual_source_id", "TEXT")?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_stats_aggregate_api_id_created_at
             ON request_token_stats(aggregate_api_id, created_at DESC)",
            [],
        )?;
'''
if old not in text:
    raise SystemExit('block not found')
path.write_text(text.replace(old, new, 1), encoding='utf-8')
print('fixed index order')
