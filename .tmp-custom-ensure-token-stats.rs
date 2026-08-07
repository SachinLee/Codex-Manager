    pub(super) fn ensure_request_token_stats_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS request_token_stats (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                request_log_id INTEGER NOT NULL,
                key_id TEXT,
                account_id TEXT,
                aggregate_api_id TEXT,
                aggregate_api_supplier_name TEXT,
                aggregate_api_url TEXT,
                model TEXT,
                input_tokens INTEGER,
                cached_input_tokens INTEGER,
                cache_write_input_tokens INTEGER,
                output_tokens INTEGER,
                total_tokens INTEGER,
                reasoning_output_tokens INTEGER,
                estimated_cost_usd REAL,
                created_at INTEGER NOT NULL
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_request_token_stats_request_log_id
             ON request_token_stats(request_log_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_stats_created_at
             ON request_token_stats(created_at DESC)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_stats_account_id_created_at
             ON request_token_stats(account_id, created_at DESC)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_stats_key_id_created_at
             ON request_token_stats(key_id, created_at DESC)",
            [],
        )?;
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS request_token_stat_rollups (
                key_id TEXT NOT NULL DEFAULT '',
                account_id TEXT NOT NULL DEFAULT '',
                model TEXT NOT NULL DEFAULT '',
                input_tokens INTEGER NOT NULL DEFAULT 0,
                cached_input_tokens INTEGER NOT NULL DEFAULT 0,
                cache_write_input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                reasoning_output_tokens INTEGER NOT NULL DEFAULT 0,
                estimated_cost_usd REAL NOT NULL DEFAULT 0.0,
                source_rows INTEGER NOT NULL DEFAULT 0,
                success_count INTEGER NOT NULL DEFAULT 0,
                error_count INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (key_id, account_id, model)
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_stat_rollups_key_id
             ON request_token_stat_rollups(key_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_stat_rollups_model
             ON request_token_stat_rollups(model)",
            [],
        )?;
        self.ensure_column(
            "request_token_stat_rollups",
            "success_count",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        self.ensure_column(
            "request_token_stat_rollups",
            "error_count",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        self.ensure_column("request_token_stats", "total_tokens", "INTEGER")?;
        self.ensure_column("request_token_stats", "cache_write_input_tokens", "INTEGER")?;
        self.ensure_column(
            "request_token_stat_rollups",
            "cache_write_input_tokens",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        self.ensure_column("request_token_stats", "aggregate_api_id", "TEXT")?;
        self.ensure_column("request_token_stats", "aggregate_api_supplier_name", "TEXT")?;
        self.ensure_column("request_token_stats", "aggregate_api_url", "TEXT")?;
        // 中文注释：087 回填依赖这两列；历史库可能未经过 087 的 ALTER，这里兜底补齐，
        // 兼容 apply_sql_or_compat_migration 在 "duplicate column name" 时走到此 fallback 的场景。
        self.ensure_column("request_token_stats", "actual_source_kind", "TEXT")?;
        self.ensure_column("request_token_stats", "actual_source_id", "TEXT")?;
        self.ensure_request_token_daily_rollups_table()?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_stats_aggregate_api_id_created_at
             ON request_token_stats(aggregate_api_id, created_at DESC)",
            [],
        )?;

        if self.has_column("request_logs", "input_tokens")? {
            let aggregate_api_id_expr =
                if self.has_column("request_logs", "initial_aggregate_api_id")? {
                    "initial_aggregate_api_id"
                } else {
                    "NULL"
                };
            let aggregate_api_supplier_name_expr =
                if self.has_column("request_logs", "aggregate_api_supplier_name")? {
                    "aggregate_api_supplier_name"
                } else {
                    "NULL"
                };
            let aggregate_api_url_expr = if self.has_column("request_logs", "aggregate_api_url")? {
                "aggregate_api_url"
            } else {
                "NULL"
            };
            // 中文注释：迁移历史 request_logs 里的 token 字段，避免升级后今日统计突然归零。
            let backfill_sql = format!(
                "INSERT OR IGNORE INTO request_token_stats (
                    request_log_id, key_id, account_id, aggregate_api_id, aggregate_api_supplier_name, aggregate_api_url, model,
                    input_tokens, cached_input_tokens, output_tokens, total_tokens, reasoning_output_tokens,
                    estimated_cost_usd, created_at
                 )
                 SELECT
                    id, key_id, account_id, {aggregate_api_id_expr}, {aggregate_api_supplier_name_expr}, {aggregate_api_url_expr}, model,
                    input_tokens, cached_input_tokens, output_tokens, NULL, reasoning_output_tokens,
                    estimated_cost_usd, created_at
                 FROM request_logs
                 WHERE input_tokens IS NOT NULL
                    OR cached_input_tokens IS NOT NULL
                    OR output_tokens IS NOT NULL
                    OR reasoning_output_tokens IS NOT NULL
                    OR estimated_cost_usd IS NOT NULL"
            );
            self.conn.execute(backfill_sql.as_str(), [])?;
        }
        self.backfill_request_token_stats_aggregate_api_context()?;
        Ok(())
    }

