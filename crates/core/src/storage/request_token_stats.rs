use rusqlite::{params, Result};
use serde_json::Value as JsonValue;

use super::{
    AccountDailyUsageSummary, AggregateApiDailyUsageSummary, ApiKeyTokenUsageSummary,
    RequestLogTodaySummary, RequestTokenStat, Storage,
};

impl Storage {
    /// 函数 `insert_request_token_stat`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - stat: 参数 stat
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn insert_request_token_stat(&self, stat: &RequestTokenStat) -> Result<()> {
        self.conn.execute(
            "INSERT INTO request_token_stats (
                request_log_id, key_id, account_id, aggregate_api_id, aggregate_api_supplier_name, aggregate_api_url, model,
                input_tokens, cached_input_tokens, output_tokens, total_tokens, reasoning_output_tokens,
                estimated_cost_usd, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            (
                stat.request_log_id,
                &stat.key_id,
                &stat.account_id,
                &stat.aggregate_api_id,
                &stat.aggregate_api_supplier_name,
                &stat.aggregate_api_url,
                &stat.model,
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
    }

    /// 函数 `summarize_request_token_stats_between`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - start_ts: 参数 start_ts
    /// - end_ts: 参数 end_ts
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn summarize_request_token_stats_between(
        &self,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<RequestLogTodaySummary> {
        let mut stmt = self.conn.prepare(
            "SELECT
                IFNULL(SUM(input_tokens), 0),
                IFNULL(SUM(cached_input_tokens), 0),
                IFNULL(SUM(output_tokens), 0),
                IFNULL(SUM(reasoning_output_tokens), 0),
                IFNULL(SUM(estimated_cost_usd), 0.0)
             FROM request_token_stats
             WHERE created_at >= ?1 AND created_at < ?2",
        )?;
        let mut rows = stmt.query((start_ts, end_ts))?;
        if let Some(row) = rows.next()? {
            return Ok(RequestLogTodaySummary {
                input_tokens: row.get(0)?,
                cached_input_tokens: row.get(1)?,
                output_tokens: row.get(2)?,
                reasoning_output_tokens: row.get(3)?,
                estimated_cost_usd: row.get(4)?,
            });
        }
        Ok(RequestLogTodaySummary {
            input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            estimated_cost_usd: 0.0,
        })
    }

    /// 函数 `summarize_request_token_stats_by_key`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn summarize_request_token_stats_by_key(&self) -> Result<Vec<ApiKeyTokenUsageSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                key_id,
                IFNULL(
                    SUM(
                        CASE
                            WHEN total_tokens IS NOT NULL THEN
                                CASE WHEN total_tokens > 0 THEN total_tokens ELSE 0 END
                            ELSE
                                CASE
                                    WHEN IFNULL(input_tokens, 0) - IFNULL(cached_input_tokens, 0) + IFNULL(output_tokens, 0) > 0
                                        THEN IFNULL(input_tokens, 0) - IFNULL(cached_input_tokens, 0) + IFNULL(output_tokens, 0)
                                    ELSE 0
                                END
                        END
                    ),
                    0
                ) AS total_tokens,
                IFNULL(SUM(estimated_cost_usd), 0.0) AS estimated_cost_usd
             FROM request_token_stats
             WHERE key_id IS NOT NULL AND TRIM(key_id) <> ''
             GROUP BY key_id
             ORDER BY total_tokens DESC, key_id ASC",
        )?;
        let mut rows = stmt.query([])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(ApiKeyTokenUsageSummary {
                key_id: row.get(0)?,
                total_tokens: row.get(1)?,
                estimated_cost_usd: row.get(2)?,
            });
        }
        Ok(items)
    }

    pub fn summarize_request_token_stats_by_account_between(
        &self,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<AccountDailyUsageSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                account_id,
                COUNT(1) AS request_count,
                IFNULL(SUM(CASE WHEN IFNULL(input_tokens, 0) > 0 THEN input_tokens ELSE 0 END), 0) AS input_tokens,
                IFNULL(SUM(CASE
                    WHEN IFNULL(cached_input_tokens, 0) < 0 THEN 0
                    WHEN IFNULL(input_tokens, 0) > 0 AND IFNULL(cached_input_tokens, 0) > input_tokens THEN input_tokens
                    ELSE IFNULL(cached_input_tokens, 0)
                END), 0) AS cached_input_tokens,
                IFNULL(SUM(CASE WHEN IFNULL(output_tokens, 0) > 0 THEN output_tokens ELSE 0 END), 0) AS output_tokens,
                IFNULL(SUM(
                    CASE
                        WHEN total_tokens IS NOT NULL THEN
                            CASE WHEN total_tokens > 0 THEN total_tokens ELSE 0 END
                        ELSE
                            CASE
                                WHEN IFNULL(input_tokens, 0) - IFNULL(cached_input_tokens, 0) + IFNULL(output_tokens, 0) > 0
                                    THEN IFNULL(input_tokens, 0) - IFNULL(cached_input_tokens, 0) + IFNULL(output_tokens, 0)
                                ELSE 0
                            END
                    END
                ), 0) AS total_tokens,
                IFNULL(SUM(CASE WHEN IFNULL(reasoning_output_tokens, 0) > 0 THEN reasoning_output_tokens ELSE 0 END), 0) AS reasoning_output_tokens,
                IFNULL(SUM(estimated_cost_usd), 0.0) AS estimated_cost_usd
             FROM request_token_stats
             WHERE created_at >= ?1
                AND created_at < ?2
                AND account_id IS NOT NULL
                AND TRIM(account_id) <> ''
             GROUP BY account_id
             ORDER BY estimated_cost_usd DESC, total_tokens DESC, account_id ASC",
        )?;
        let mut rows = stmt.query((start_ts, end_ts))?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            let input_tokens = row.get::<_, i64>(2)?;
            let cached_input_tokens = row.get::<_, i64>(3)?;
            let billable_input_tokens = input_tokens.saturating_sub(cached_input_tokens);
            items.push(AccountDailyUsageSummary {
                account_id: row.get(0)?,
                request_count: row.get(1)?,
                input_tokens,
                cached_input_tokens,
                billable_input_tokens,
                output_tokens: row.get(4)?,
                total_tokens: row.get(5)?,
                reasoning_output_tokens: row.get(6)?,
                estimated_cost_usd: row.get(7)?,
                cache_hit_rate: cache_hit_rate(input_tokens, cached_input_tokens),
            });
        }
        Ok(items)
    }

    pub fn summarize_request_token_stats_by_aggregate_api_between(
        &self,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<AggregateApiDailyUsageSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                aggregate_api_id,
                MAX(aggregate_api_supplier_name) AS aggregate_api_supplier_name,
                MAX(aggregate_api_url) AS aggregate_api_url,
                COUNT(1) AS request_count,
                IFNULL(SUM(CASE WHEN IFNULL(input_tokens, 0) > 0 THEN input_tokens ELSE 0 END), 0) AS input_tokens,
                IFNULL(SUM(CASE
                    WHEN IFNULL(cached_input_tokens, 0) < 0 THEN 0
                    WHEN IFNULL(input_tokens, 0) > 0 AND IFNULL(cached_input_tokens, 0) > input_tokens THEN input_tokens
                    ELSE IFNULL(cached_input_tokens, 0)
                END), 0) AS cached_input_tokens,
                IFNULL(SUM(CASE WHEN IFNULL(output_tokens, 0) > 0 THEN output_tokens ELSE 0 END), 0) AS output_tokens,
                IFNULL(SUM(
                    CASE
                        WHEN total_tokens IS NOT NULL THEN
                            CASE WHEN total_tokens > 0 THEN total_tokens ELSE 0 END
                        ELSE
                            CASE
                                WHEN IFNULL(input_tokens, 0) - IFNULL(cached_input_tokens, 0) + IFNULL(output_tokens, 0) > 0
                                    THEN IFNULL(input_tokens, 0) - IFNULL(cached_input_tokens, 0) + IFNULL(output_tokens, 0)
                                ELSE 0
                            END
                    END
                ), 0) AS total_tokens,
                IFNULL(SUM(CASE WHEN IFNULL(reasoning_output_tokens, 0) > 0 THEN reasoning_output_tokens ELSE 0 END), 0) AS reasoning_output_tokens,
                IFNULL(SUM(estimated_cost_usd), 0.0) AS estimated_cost_usd
             FROM request_token_stats
             WHERE created_at >= ?1
                AND created_at < ?2
                AND aggregate_api_id IS NOT NULL
                AND TRIM(aggregate_api_id) <> ''
             GROUP BY aggregate_api_id
             ORDER BY estimated_cost_usd DESC, total_tokens DESC, aggregate_api_id ASC",
        )?;
        let mut rows = stmt.query((start_ts, end_ts))?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            let input_tokens = row.get::<_, i64>(4)?;
            let cached_input_tokens = row.get::<_, i64>(5)?;
            let billable_input_tokens = input_tokens.saturating_sub(cached_input_tokens);
            items.push(AggregateApiDailyUsageSummary {
                aggregate_api_id: row.get(0)?,
                aggregate_api_supplier_name: row.get(1)?,
                aggregate_api_url: row.get(2)?,
                request_count: row.get(3)?,
                input_tokens,
                cached_input_tokens,
                billable_input_tokens,
                output_tokens: row.get(6)?,
                total_tokens: row.get(7)?,
                reasoning_output_tokens: row.get(8)?,
                estimated_cost_usd: row.get(9)?,
                cache_hit_rate: cache_hit_rate(input_tokens, cached_input_tokens),
            });
        }
        Ok(items)
    }

    pub fn aggregate_api_estimated_cost_between(
        &self,
        aggregate_api_id: &str,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<f64> {
        self.conn.query_row(
            "SELECT IFNULL(SUM(estimated_cost_usd), 0.0)
             FROM request_token_stats
             WHERE aggregate_api_id = ?1
                AND created_at >= ?2
                AND created_at < ?3",
            (aggregate_api_id, start_ts, end_ts),
            |row| row.get(0),
        )
    }

    /// 函数 `ensure_request_token_stats_table`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - super: 参数 super
    ///
    /// # 返回
    /// 返回函数执行结果
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
        self.ensure_column("request_token_stats", "total_tokens", "INTEGER")?;
        self.ensure_column("request_token_stats", "aggregate_api_id", "TEXT")?;
        self.ensure_column("request_token_stats", "aggregate_api_supplier_name", "TEXT")?;
        self.ensure_column("request_token_stats", "aggregate_api_url", "TEXT")?;
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

    fn backfill_request_token_stats_aggregate_api_context(&self) -> Result<()> {
        if !self.has_table("request_logs")?
            || !self.has_column("request_token_stats", "aggregate_api_id")?
        {
            return Ok(());
        }

        let has_initial = self.has_column("request_logs", "initial_aggregate_api_id")?;
        let has_attempted = self.has_column("request_logs", "attempted_aggregate_api_ids_json")?;
        let has_supplier = self.has_column("request_logs", "aggregate_api_supplier_name")?;
        let has_url = self.has_column("request_logs", "aggregate_api_url")?;
        if !has_initial && !has_attempted && !has_supplier && !has_url {
            return Ok(());
        }

        let initial_select = if has_initial {
            "r.initial_aggregate_api_id"
        } else {
            "NULL"
        };
        let attempted_select = if has_attempted {
            "r.attempted_aggregate_api_ids_json"
        } else {
            "NULL"
        };
        let supplier_select = if has_supplier {
            "r.aggregate_api_supplier_name"
        } else {
            "NULL"
        };
        let url_select = if has_url {
            "r.aggregate_api_url"
        } else {
            "NULL"
        };

        let mut stmt = self.conn.prepare(&format!(
            "SELECT
                t.request_log_id,
                t.aggregate_api_id,
                t.aggregate_api_supplier_name,
                t.aggregate_api_url,
                {initial_select},
                {attempted_select},
                {supplier_select},
                {url_select}
             FROM request_token_stats t
             JOIN request_logs r ON r.id = t.request_log_id
             WHERE ({initial_select} IS NOT NULL AND TRIM({initial_select}) <> '')
                OR ({attempted_select} IS NOT NULL AND TRIM({attempted_select}) <> '')
                OR ({supplier_select} IS NOT NULL AND TRIM({supplier_select}) <> '')
                OR ({url_select} IS NOT NULL AND TRIM({url_select}) <> '')"
        ))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            })?
            .collect::<Result<Vec<_>>>()?;
        drop(stmt);

        for (
            request_log_id,
            current_api_id,
            current_supplier_name,
            current_url,
            initial_api_id,
            attempted_api_ids_json,
            log_supplier_name,
            log_url,
        ) in rows
        {
            let final_api_id = final_attempted_aggregate_api_id(
                attempted_api_ids_json.as_deref(),
                initial_api_id.as_deref(),
            );
            let next_api_id = final_api_id.or_else(|| non_blank_owned(current_api_id.clone()));
            let next_supplier_name =
                non_blank_owned(log_supplier_name).or_else(|| current_supplier_name.clone());
            let next_url = non_blank_owned(log_url).or_else(|| current_url.clone());

            if next_api_id == current_api_id
                && next_supplier_name == current_supplier_name
                && next_url == current_url
            {
                continue;
            }

            self.conn.execute(
                "UPDATE request_token_stats
                 SET aggregate_api_id = ?2,
                     aggregate_api_supplier_name = ?3,
                     aggregate_api_url = ?4
                 WHERE request_log_id = ?1",
                params![request_log_id, next_api_id, next_supplier_name, next_url],
            )?;
        }
        Ok(())
    }
}

fn cache_hit_rate(input_tokens: i64, cached_input_tokens: i64) -> f64 {
    if input_tokens <= 0 {
        return 0.0;
    }
    let cached = cached_input_tokens.clamp(0, input_tokens);
    cached as f64 / input_tokens as f64
}

fn final_attempted_aggregate_api_id(
    attempted_api_ids_json: Option<&str>,
    initial_api_id: Option<&str>,
) -> Option<String> {
    attempted_api_ids_json
        .and_then(|value| serde_json::from_str::<JsonValue>(value).ok())
        .and_then(|value| match value {
            JsonValue::Array(items) => items.into_iter().rev().find_map(|item| match item {
                JsonValue::String(value) => non_blank_str(value.as_str()),
                _ => None,
            }),
            _ => None,
        })
        .or_else(|| initial_api_id.and_then(non_blank_str))
}

fn non_blank_owned(value: Option<String>) -> Option<String> {
    value.and_then(|value| non_blank_str(value.as_str()))
}

fn non_blank_str(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
