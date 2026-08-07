from pathlib import Path
path = Path(r"crates/core/src/storage/request_token_stats.rs")
text = path.read_text(encoding="utf-8")

old_insert = """    pub fn insert_request_token_stat(&self, stat: &RequestTokenStat) -> Result<()> {
        self.conn.execute(
            \"INSERT INTO request_token_stats (
                request_log_id, key_id, account_id, model, actual_source_kind, actual_source_id,
                input_tokens, cached_input_tokens, output_tokens, total_tokens, reasoning_output_tokens,
                estimated_cost_usd, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)\",
            (
                stat.request_log_id,
                &stat.key_id,
                &stat.account_id,
                &stat.model,
                &stat.actual_source_kind,
                &stat.actual_source_id,
                stat.input_tokens,
                stat.cached_input_tokens,
                stat.output_tokens,
                stat.total_tokens,
                stat.reasoning_output_tokens,
                stat.estimated_cost_usd,
                stat.created_at,
            ),
        )?;
        Ok(())
    }"""

new_insert = """    pub fn insert_request_token_stat(&self, stat: &RequestTokenStat) -> Result<()> {
        self.conn.execute(
            \"INSERT INTO request_token_stats (
                request_log_id, key_id, account_id, aggregate_api_id, aggregate_api_supplier_name, aggregate_api_url,
                model, actual_source_kind, actual_source_id,
                input_tokens, cached_input_tokens, cache_write_input_tokens, output_tokens, total_tokens,
                reasoning_output_tokens, estimated_cost_usd, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)\",
            (
                stat.request_log_id,
                &stat.key_id,
                &stat.account_id,
                &stat.aggregate_api_id,
                &stat.aggregate_api_supplier_name,
                &stat.aggregate_api_url,
                &stat.model,
                &stat.actual_source_kind,
                &stat.actual_source_id,
                stat.input_tokens,
                stat.cached_input_tokens,
                stat.cache_write_input_tokens,
                stat.output_tokens,
                stat.total_tokens,
                stat.reasoning_output_tokens,
                stat.estimated_cost_usd,
                stat.created_at,
            ),
        )?;
        Ok(())
    }"""

if old_insert not in text:
    raise SystemExit("insert block not found")
text = text.replace(old_insert, new_insert, 1)

old_hourly_cols = """                \"INSERT INTO request_token_stat_hourly_rollups (
                    bucket_start, bucket_end, key_id, account_id, model, actual_source_kind, actual_source_id,
                    owner_user_id, input_tokens, cached_input_tokens, output_tokens, total_tokens,
                    reasoning_output_tokens, estimated_cost_usd, request_count, success_count,
                    error_count, updated_at
                 )
                 SELECT
                    CAST(t.created_at / {HOUR_SECONDS} AS INTEGER) * {HOUR_SECONDS},
                    CAST(t.created_at / {HOUR_SECONDS} AS INTEGER) * {HOUR_SECONDS} + {HOUR_SECONDS},
                    COALESCE(NULLIF(TRIM(t.key_id), ''), ''),
                    COALESCE(NULLIF(TRIM(t.account_id), ''), ''),
                    COALESCE(NULLIF(TRIM(t.model), ''), ''),
                    COALESCE(NULLIF(TRIM(t.actual_source_kind), ''), ''),
                    COALESCE(NULLIF(TRIM(t.actual_source_id), ''), ''),
                    COALESCE({USER_OWNER_EXPR}, ''),
                    IFNULL(SUM(CASE WHEN t.input_tokens > 0 THEN t.input_tokens ELSE 0 END), 0),
                    IFNULL(SUM(CASE WHEN t.cached_input_tokens > 0 THEN t.cached_input_tokens ELSE 0 END), 0),
                    IFNULL(SUM(CASE WHEN t.output_tokens > 0 THEN t.output_tokens ELSE 0 END), 0),
                    IFNULL(SUM({token_total}), 0),
                    IFNULL(SUM(CASE WHEN t.reasoning_output_tokens > 0 THEN t.reasoning_output_tokens ELSE 0 END), 0),
                    IFNULL(SUM(CASE WHEN t.estimated_cost_usd > 0 THEN t.estimated_cost_usd ELSE 0 END), 0.0),
                    COUNT(DISTINCT t.request_log_id),
                    COUNT(DISTINCT CASE WHEN r.status_code >= 200 AND r.status_code <= 299 THEN t.request_log_id END),
                    COUNT(DISTINCT CASE WHEN IFNULL(r.status_code, 0) >= 400 OR TRIM(IFNULL(r.error, '')) <> '' THEN t.request_log_id END),
                    ?2
                 FROM request_token_stats t
                 LEFT JOIN request_logs r ON r.id = t.request_log_id
                 {USER_OWNER_JOINS}
                 WHERE t.created_at < ?1
                 GROUP BY
                    CAST(t.created_at / {HOUR_SECONDS} AS INTEGER) * {HOUR_SECONDS},
                    COALESCE(NULLIF(TRIM(t.key_id), ''), ''),
                    COALESCE(NULLIF(TRIM(t.account_id), ''), ''),
                    COALESCE(NULLIF(TRIM(t.model), ''), ''),
                    COALESCE(NULLIF(TRIM(t.actual_source_kind), ''), ''),
                    COALESCE(NULLIF(TRIM(t.actual_source_id), ''), ''),
                    COALESCE({USER_OWNER_EXPR}, '')
                 ON CONFLICT(bucket_start, key_id, account_id, model, actual_source_kind, actual_source_id, owner_user_id)
                 DO UPDATE SET
                    bucket_end = CASE
                        WHEN request_token_stat_hourly_rollups.bucket_end > excluded.bucket_end
                            THEN request_token_stat_hourly_rollups.bucket_end
                        ELSE excluded.bucket_end
                    END,
                    input_tokens = request_token_stat_hourly_rollups.input_tokens + excluded.input_tokens,
                    cached_input_tokens = request_token_stat_hourly_rollups.cached_input_tokens + excluded.cached_input_tokens,
                    output_tokens = request_token_stat_hourly_rollups.output_tokens + excluded.output_tokens,
                    total_tokens = request_token_stat_hourly_rollups.total_tokens + excluded.total_tokens,
                    reasoning_output_tokens = request_token_stat_hourly_rollups.reasoning_output_tokens + excluded.reasoning_output_tokens,
                    estimated_cost_usd = request_token_stat_hourly_rollups.estimated_cost_usd + excluded.estimated_cost_usd,
                    request_count = request_token_stat_hourly_rollups.request_count + excluded.request_count,
                    success_count = request_token_stat_hourly_rollups.success_count + excluded.success_count,
                    error_count = request_token_stat_hourly_rollups.error_count + excluded.error_count,
                    updated_at = excluded.updated_at","""

new_hourly_cols = """                \"INSERT INTO request_token_stat_hourly_rollups (
                    bucket_start, bucket_end, key_id, account_id, model, actual_source_kind, actual_source_id,
                    owner_user_id, input_tokens, cached_input_tokens, cache_write_input_tokens, output_tokens, total_tokens,
                    reasoning_output_tokens, estimated_cost_usd, request_count, success_count,
                    error_count, updated_at
                 )
                 SELECT
                    CAST(t.created_at / {HOUR_SECONDS} AS INTEGER) * {HOUR_SECONDS},
                    CAST(t.created_at / {HOUR_SECONDS} AS INTEGER) * {HOUR_SECONDS} + {HOUR_SECONDS},
                    COALESCE(NULLIF(TRIM(t.key_id), ''), ''),
                    COALESCE(NULLIF(TRIM(t.account_id), ''), ''),
                    COALESCE(NULLIF(TRIM(t.model), ''), ''),
                    COALESCE(NULLIF(TRIM(t.actual_source_kind), ''), ''),
                    COALESCE(NULLIF(TRIM(t.actual_source_id), ''), ''),
                    COALESCE({USER_OWNER_EXPR}, ''),
                    IFNULL(SUM(CASE WHEN t.input_tokens > 0 THEN t.input_tokens ELSE 0 END), 0),
                    IFNULL(SUM(CASE WHEN t.cached_input_tokens > 0 THEN t.cached_input_tokens ELSE 0 END), 0),
                    IFNULL(SUM(CASE WHEN t.cache_write_input_tokens > 0 THEN t.cache_write_input_tokens ELSE 0 END), 0),
                    IFNULL(SUM(CASE WHEN t.output_tokens > 0 THEN t.output_tokens ELSE 0 END), 0),
                    IFNULL(SUM({token_total}), 0),
                    IFNULL(SUM(CASE WHEN t.reasoning_output_tokens > 0 THEN t.reasoning_output_tokens ELSE 0 END), 0),
                    IFNULL(SUM(CASE WHEN t.estimated_cost_usd > 0 THEN t.estimated_cost_usd ELSE 0 END), 0.0),
                    COUNT(DISTINCT t.request_log_id),
                    COUNT(DISTINCT CASE WHEN r.status_code >= 200 AND r.status_code <= 299 THEN t.request_log_id END),
                    COUNT(DISTINCT CASE WHEN IFNULL(r.status_code, 0) >= 400 OR TRIM(IFNULL(r.error, '')) <> '' THEN t.request_log_id END),
                    ?2
                 FROM request_token_stats t
                 LEFT JOIN request_logs r ON r.id = t.request_log_id
                 {USER_OWNER_JOINS}
                 WHERE t.created_at < ?1
                 GROUP BY
                    CAST(t.created_at / {HOUR_SECONDS} AS INTEGER) * {HOUR_SECONDS},
                    COALESCE(NULLIF(TRIM(t.key_id), ''), ''),
                    COALESCE(NULLIF(TRIM(t.account_id), ''), ''),
                    COALESCE(NULLIF(TRIM(t.model), ''), ''),
                    COALESCE(NULLIF(TRIM(t.actual_source_kind), ''), ''),
                    COALESCE(NULLIF(TRIM(t.actual_source_id), ''), ''),
                    COALESCE({USER_OWNER_EXPR}, '')
                 ON CONFLICT(bucket_start, key_id, account_id, model, actual_source_kind, actual_source_id, owner_user_id)
                 DO UPDATE SET
                    bucket_end = CASE
                        WHEN request_token_stat_hourly_rollups.bucket_end > excluded.bucket_end
                            THEN request_token_stat_hourly_rollups.bucket_end
                        ELSE excluded.bucket_end
                    END,
                    input_tokens = request_token_stat_hourly_rollups.input_tokens + excluded.input_tokens,
                    cached_input_tokens = request_token_stat_hourly_rollups.cached_input_tokens + excluded.cached_input_tokens,
                    cache_write_input_tokens = request_token_stat_hourly_rollups.cache_write_input_tokens + excluded.cache_write_input_tokens,
                    output_tokens = request_token_stat_hourly_rollups.output_tokens + excluded.output_tokens,
                    total_tokens = request_token_stat_hourly_rollups.total_tokens + excluded.total_tokens,
                    reasoning_output_tokens = request_token_stat_hourly_rollups.reasoning_output_tokens + excluded.reasoning_output_tokens,
                    estimated_cost_usd = request_token_stat_hourly_rollups.estimated_cost_usd + excluded.estimated_cost_usd,
                    request_count = request_token_stat_hourly_rollups.request_count + excluded.request_count,
                    success_count = request_token_stat_hourly_rollups.success_count + excluded.success_count,
                    error_count = request_token_stat_hourly_rollups.error_count + excluded.error_count,
                    updated_at = excluded.updated_at","""

if old_hourly_cols not in text:
    raise SystemExit("hourly rollup block not found")
text = text.replace(old_hourly_cols, new_hourly_cols, 1)

start = text.find("    pub(super) fn ensure_request_token_stats_table(&self) -> Result<()> {")
if start < 0:
    raise SystemExit("ensure start not found")
end = text.find("    pub fn summarize_request_token_stats_by_account_between(", start)
if end < 0:
    raise SystemExit("ensure end not found")

new_ensure = r'''    pub(super) fn ensure_request_token_stats_table(&self) -> Result<()> {
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
                actual_source_kind TEXT,
                actual_source_id TEXT,
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
            "CREATE INDEX IF NOT EXISTS idx_request_token_stats_model_created_at
             ON request_token_stats(model, created_at DESC)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_stats_key_model_created_at
             ON request_token_stats(key_id, model, created_at DESC)",
            [],
        )?;
        self.conn.execute(
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
        if self.has_column("request_logs", "actual_source_kind")?
            && self.has_column("request_logs", "actual_source_id")?
        {
            self.conn.execute(
                "UPDATE request_token_stats
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
                 WHERE (actual_source_kind IS NULL OR TRIM(actual_source_kind) = '')
                   AND (actual_source_id IS NULL OR TRIM(actual_source_id) = '')
                   AND request_log_id IS NOT NULL
                   AND EXISTS (
                        SELECT 1
                        FROM request_logs
                        WHERE request_logs.id = request_token_stats.request_log_id
                          AND (
                            request_logs.actual_source_kind IS NOT NULL
                            OR request_logs.actual_source_id IS NOT NULL
                          )
                   )",
                [],
            )?;
        }
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_stats_actual_source_created_at
             ON request_token_stats(actual_source_kind, actual_source_id, created_at DESC)",
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
            "cache_write_input_tokens",
            "INTEGER NOT NULL DEFAULT 0",
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
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS request_token_stat_hourly_rollups (
                bucket_start INTEGER NOT NULL,
                bucket_end INTEGER NOT NULL,
                key_id TEXT NOT NULL DEFAULT '',
                account_id TEXT NOT NULL DEFAULT '',
                model TEXT NOT NULL DEFAULT '',
                actual_source_kind TEXT NOT NULL DEFAULT '',
                actual_source_id TEXT NOT NULL DEFAULT '',
                owner_user_id TEXT NOT NULL DEFAULT '',
                input_tokens INTEGER NOT NULL DEFAULT 0,
                cached_input_tokens INTEGER NOT NULL DEFAULT 0,
                cache_write_input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                reasoning_output_tokens INTEGER NOT NULL DEFAULT 0,
                estimated_cost_usd REAL NOT NULL DEFAULT 0.0,
                request_count INTEGER NOT NULL DEFAULT 0,
                success_count INTEGER NOT NULL DEFAULT 0,
                error_count INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY(bucket_start, key_id, account_id, model, actual_source_kind, actual_source_id, owner_user_id)
             )",
            [],
        )?;
        self.ensure_column("request_token_stat_hourly_rollups", "bucket_end", "INTEGER")?;
        self.ensure_column(
            "request_token_stat_hourly_rollups",
            "cache_write_input_tokens",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        self.conn.execute(
            "UPDATE request_token_stat_hourly_rollups
             SET bucket_end = bucket_start + 3600
             WHERE bucket_end IS NULL OR bucket_end <= bucket_start",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_stat_hourly_rollups_bucket_start
             ON request_token_stat_hourly_rollups(bucket_start)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_stat_hourly_rollups_key_bucket
             ON request_token_stat_hourly_rollups(key_id, bucket_start)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_stat_hourly_rollups_owner_bucket
             ON request_token_stat_hourly_rollups(owner_user_id, bucket_start)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_stat_hourly_rollups_source_bucket
             ON request_token_stat_hourly_rollups(actual_source_kind, actual_source_id, bucket_start)",
            [],
        )?;
        // Legacy custom daily rollups remain for databases that still hold compacted
        // rows; the bridge migration normalizes cache-write values against them.
        self.ensure_request_token_daily_rollups_table()?;
        if self.has_column("request_logs", "input_tokens")? {
            let actual_source_kind_expr =
                if self.has_column("request_logs", "actual_source_kind")? {
                    "actual_source_kind"
                } else {
                    "NULL"
                };
            let actual_source_id_expr = if self.has_column("request_logs", "actual_source_id")? {
                "actual_source_id"
            } else {
                "NULL"
            };
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
            let cache_write_expr = if self.has_column("request_logs", "cache_write_input_tokens")?
            {
                "cache_write_input_tokens"
            } else {
                "NULL"
            };
            let backfill_sql = format!(
                "INSERT OR IGNORE INTO request_token_stats (
                    request_log_id, key_id, account_id, aggregate_api_id, aggregate_api_supplier_name,
                    aggregate_api_url, model, actual_source_kind, actual_source_id,
                    input_tokens, cached_input_tokens, cache_write_input_tokens, output_tokens,
                    total_tokens, reasoning_output_tokens, estimated_cost_usd, created_at
                 )
                 SELECT
                    id, key_id, account_id, {aggregate_api_id_expr}, {aggregate_api_supplier_name_expr},
                    {aggregate_api_url_expr}, model, {actual_source_kind_expr}, {actual_source_id_expr},
                    input_tokens, cached_input_tokens, {cache_write_expr}, output_tokens, NULL,
                    reasoning_output_tokens, estimated_cost_usd, created_at
                 FROM request_logs
                 WHERE input_tokens IS NOT NULL
                    OR cached_input_tokens IS NOT NULL
                    OR output_tokens IS NOT NULL
                    OR reasoning_output_tokens IS NOT NULL
                    OR estimated_cost_usd IS NOT NULL"
            );
            self.conn.execute(&backfill_sql, [])?;
        }
        self.backfill_request_token_stats_aggregate_api_context()?;
        Ok(())
    }

    pub(super) fn ensure_request_token_daily_rollups_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS request_token_daily_rollups (
                day_start_ts INTEGER NOT NULL,
                source_kind TEXT NOT NULL DEFAULT 'global',
                source_id TEXT NOT NULL DEFAULT '',
                input_tokens INTEGER NOT NULL DEFAULT 0,
                cached_input_tokens INTEGER NOT NULL DEFAULT 0,
                cache_write_input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                reasoning_output_tokens INTEGER NOT NULL DEFAULT 0,
                estimated_cost_usd REAL NOT NULL DEFAULT 0.0,
                request_count INTEGER NOT NULL DEFAULT 0,
                success_count INTEGER NOT NULL DEFAULT 0,
                error_count INTEGER NOT NULL DEFAULT 0,
                max_duration_ms INTEGER,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (day_start_ts, source_kind, source_id)
            )",
            [],
        )?;
        self.ensure_column(
            "request_token_daily_rollups",
            "cache_write_input_tokens",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_daily_rollups_source_day
             ON request_token_daily_rollups(source_kind, source_id, day_start_ts)",
            [],
        )?;
        Ok(())
    }

'''

text = text[:start] + new_ensure + text[end:]
path.write_text(text, encoding="utf-8")
print("patched ok")
print("daily ensure:", "fn ensure_request_token_daily_rollups_table" in text)
print("insert has cache_write:", "cache_write_input_tokens, output_tokens, total_tokens" in text)
print("hourly has cache_write sum:", "t.cache_write_input_tokens > 0 THEN t.cache_write_input_tokens" in text)
