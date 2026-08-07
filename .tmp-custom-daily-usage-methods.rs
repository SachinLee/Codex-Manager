             WHERE s.key_id IS NOT NULL AND TRIM(s.key_id) <> ''{key_filter_clause}
             GROUP BY s.key_id
             ORDER BY total_tokens DESC, s.key_id ASC",
            token_total = token_total_sql_expr(),
        ))?;
        let mut rows = stmt.query([])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(map_api_key_token_usage_summary(row)?);
        }
        Ok(items)
    }

    pub fn summarize_request_token_stats_by_account_between(
        &self,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<AccountDailyUsageSummary>> {
        let mut stmt = self.conn.prepare(
            "WITH all_stats AS (
                SELECT account_id, input_tokens, cached_input_tokens, cache_write_input_tokens,
                       output_tokens, total_tokens, reasoning_output_tokens, estimated_cost_usd,
                       1 AS request_count
                  FROM request_token_stats
                 WHERE created_at >= ?1 AND created_at < ?2
                UNION ALL
                SELECT NULLIF(TRIM(account_id), ''), input_tokens, cached_input_tokens,
                       cache_write_input_tokens, output_tokens, total_tokens,
                       reasoning_output_tokens, estimated_cost_usd, request_count
                  FROM request_token_stat_hourly_rollups
                 WHERE bucket_start >= ?3 AND bucket_end <= ?4
             )
             SELECT
                account_id,
                IFNULL(SUM(IFNULL(request_count, 0)), 0) AS request_count,
                IFNULL(SUM(CASE WHEN IFNULL(input_tokens, 0) > 0 THEN input_tokens ELSE 0 END), 0) AS input_tokens,
                IFNULL(SUM(CASE
                    WHEN IFNULL(cached_input_tokens, 0) < 0 THEN 0
                    WHEN IFNULL(input_tokens, 0) > 0 AND IFNULL(cached_input_tokens, 0) > input_tokens THEN input_tokens
                    ELSE IFNULL(cached_input_tokens, 0)
                END), 0) AS cached_input_tokens,
                IFNULL(SUM(CASE
                    WHEN IFNULL(input_tokens, 0) <= 0
                        OR IFNULL(cache_write_input_tokens, 0) <= 0 THEN 0
                    WHEN IFNULL(cache_write_input_tokens, 0) > MAX(IFNULL(input_tokens, 0), 0) - CASE
                        WHEN IFNULL(cached_input_tokens, 0) < 0 THEN 0
                        WHEN IFNULL(cached_input_tokens, 0) > IFNULL(input_tokens, 0)
                            THEN MAX(IFNULL(input_tokens, 0), 0)
                        ELSE IFNULL(cached_input_tokens, 0)
                    END THEN MAX(IFNULL(input_tokens, 0), 0) - CASE
                        WHEN IFNULL(cached_input_tokens, 0) < 0 THEN 0
                        WHEN IFNULL(cached_input_tokens, 0) > IFNULL(input_tokens, 0)
                            THEN MAX(IFNULL(input_tokens, 0), 0)
                        ELSE IFNULL(cached_input_tokens, 0)
                    END
                    ELSE IFNULL(cache_write_input_tokens, 0)
                END), 0) AS cache_write_input_tokens,
                IFNULL(SUM(CASE WHEN IFNULL(output_tokens, 0) > 0 THEN output_tokens ELSE 0 END), 0) AS output_tokens,
                IFNULL(SUM(
                    CASE
                        WHEN total_tokens IS NOT NULL THEN
                            CASE WHEN total_tokens > 0 THEN total_tokens ELSE 0 END
                        ELSE
                            CASE
                                WHEN IFNULL(input_tokens, 0) + IFNULL(output_tokens, 0) > 0
                                    THEN IFNULL(input_tokens, 0) + IFNULL(output_tokens, 0)
                                ELSE 0
                            END
                    END
                ), 0) AS total_tokens,
                IFNULL(SUM(CASE WHEN IFNULL(reasoning_output_tokens, 0) > 0 THEN reasoning_output_tokens ELSE 0 END), 0) AS reasoning_output_tokens,
                IFNULL(SUM(estimated_cost_usd), 0.0) AS estimated_cost_usd
             FROM all_stats
             WHERE account_id IS NOT NULL
                AND TRIM(account_id) <> ''
             GROUP BY account_id
             ORDER BY estimated_cost_usd DESC, total_tokens DESC, account_id ASC",
        )?;
        let mut rows = stmt.query((start_ts, end_ts, start_ts, end_ts))?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            let input_tokens = row.get::<_, i64>(2)?;
            let cached_input_tokens = row.get::<_, i64>(3)?;
            let cache_write_input_tokens = row.get::<_, i64>(4)?;
            let billable_input_tokens = input_tokens
                .saturating_sub(cached_input_tokens)
                .saturating_sub(cache_write_input_tokens);
            items.push(AccountDailyUsageSummary {
                account_id: row.get(0)?,
                request_count: row.get(1)?,
                input_tokens,
                cached_input_tokens,
                cache_write_input_tokens,
                billable_input_tokens,
                output_tokens: row.get(5)?,
                total_tokens: row.get(6)?,
                reasoning_output_tokens: row.get(7)?,
                estimated_cost_usd: row.get(8)?,
                cache_hit_rate: cache_hit_rate(input_tokens, cached_input_tokens),
            });
        }
        Ok(items)
    }

    pub fn summarize_request_token_stats_by_model(
        &self,
        start_ts: Option<i64>,
        end_ts: Option<i64>,
    ) -> Result<Vec<TokenUsageSummary>> {
        self.summarize_request_token_stats_by_model_filtered(start_ts, end_ts, None)
    }

    pub fn summarize_request_token_stats_by_aggregate_api_between(
        &self,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<AggregateApiDailyUsageSummary>> {
        self.ensure_gateway_reasoning_guard_events_table()?;
        let sql = format!(
            "WITH all_stats AS (
                SELECT aggregate_api_id, aggregate_api_supplier_name, aggregate_api_url,
                       input_tokens, cached_input_tokens, cache_write_input_tokens, output_tokens,
                       total_tokens, reasoning_output_tokens, estimated_cost_usd, 1 AS request_count
                  FROM request_token_stats
                 WHERE created_at >= ?1 AND created_at < ?2
                UNION ALL
                SELECT CASE WHEN actual_source_kind = 'aggregate_api'
                            THEN NULLIF(TRIM(actual_source_id), '') ELSE NULL END,
                       NULL, NULL, input_tokens, cached_input_tokens, cache_write_input_tokens,
                       output_tokens, total_tokens, reasoning_output_tokens, estimated_cost_usd,
                       request_count
                  FROM request_token_stat_hourly_rollups
                 WHERE bucket_start >= ?3 AND bucket_end <= ?4
             ),
             base_rollup AS (
                SELECT
                    aggregate_api_id,
                    MAX(aggregate_api_supplier_name) AS aggregate_api_supplier_name,
                    MAX(aggregate_api_url) AS aggregate_api_url,
                    IFNULL(SUM(IFNULL(request_count, 0)), 0) AS request_count,
                    IFNULL(SUM(CASE WHEN IFNULL(input_tokens, 0) > 0 THEN input_tokens ELSE 0 END), 0) AS input_tokens,
                    IFNULL(SUM(CASE
                        WHEN IFNULL(cached_input_tokens, 0) < 0 THEN 0
                        WHEN IFNULL(input_tokens, 0) > 0 AND IFNULL(cached_input_tokens, 0) > input_tokens THEN input_tokens
                        ELSE IFNULL(cached_input_tokens, 0)
                    END), 0) AS cached_input_tokens,
                    IFNULL(SUM(CASE
                        WHEN IFNULL(input_tokens, 0) <= 0
                            OR IFNULL(cache_write_input_tokens, 0) <= 0 THEN 0
                        WHEN IFNULL(cache_write_input_tokens, 0) > MAX(IFNULL(input_tokens, 0), 0) - CASE
                            WHEN IFNULL(cached_input_tokens, 0) < 0 THEN 0
                            WHEN IFNULL(cached_input_tokens, 0) > IFNULL(input_tokens, 0)
                                THEN MAX(IFNULL(input_tokens, 0), 0)
                            ELSE IFNULL(cached_input_tokens, 0)
                        END THEN MAX(IFNULL(input_tokens, 0), 0) - CASE
                            WHEN IFNULL(cached_input_tokens, 0) < 0 THEN 0
                            WHEN IFNULL(cached_input_tokens, 0) > IFNULL(input_tokens, 0)
                                THEN MAX(IFNULL(input_tokens, 0), 0)
                            ELSE IFNULL(cached_input_tokens, 0)
                        END
                        ELSE IFNULL(cache_write_input_tokens, 0)
                    END), 0) AS cache_write_input_tokens,
                    IFNULL(SUM(CASE WHEN IFNULL(output_tokens, 0) > 0 THEN output_tokens ELSE 0 END), 0) AS output_tokens,
                    IFNULL(SUM(
                        CASE
                            WHEN total_tokens IS NOT NULL THEN
                                CASE WHEN total_tokens > 0 THEN total_tokens ELSE 0 END
                            ELSE
                                CASE
                                    WHEN IFNULL(input_tokens, 0) + IFNULL(output_tokens, 0) > 0
                                        THEN IFNULL(input_tokens, 0) + IFNULL(output_tokens, 0)
                                    ELSE 0
                                END
                        END
                    ), 0) AS total_tokens,
                    IFNULL(SUM(CASE WHEN IFNULL(reasoning_output_tokens, 0) > 0 THEN reasoning_output_tokens ELSE 0 END), 0) AS reasoning_output_tokens,
                    IFNULL(SUM(estimated_cost_usd), 0.0) AS estimated_cost_usd
                 FROM all_stats
                 WHERE aggregate_api_id IS NOT NULL
                    AND TRIM(aggregate_api_id) <> ''
                 GROUP BY aggregate_api_id
             ),
             guard_retry_rollup AS (
                SELECT
                    source_id AS aggregate_api_id,
                    IFNULL(SUM(CASE WHEN IFNULL(total_tokens, 0) > 0 THEN total_tokens ELSE 0 END), 0) AS guard_retry_total_tokens,
                    IFNULL(SUM(CASE WHEN IFNULL(estimated_cost_usd, 0.0) > 0.0 THEN estimated_cost_usd ELSE 0.0 END), 0.0) AS guard_retry_estimated_cost_usd
                  FROM gateway_reasoning_guard_events
                  WHERE {retry_action_sql}
                     AND source_kind = 'aggregate_api'
                     AND source_id IS NOT NULL
                     AND TRIM(source_id) <> ''
                    AND created_at >= ?1
                    AND created_at < ?2
                 GROUP BY source_id
             )
             SELECT
                b.aggregate_api_id,
                b.aggregate_api_supplier_name,
                b.aggregate_api_url,
                b.request_count,
                b.input_tokens,
                b.cached_input_tokens,
                b.cache_write_input_tokens,
                b.output_tokens,
                b.total_tokens,
                b.reasoning_output_tokens,
                b.estimated_cost_usd,
                COALESCE(g.guard_retry_total_tokens, 0) AS guard_retry_total_tokens,
                COALESCE(g.guard_retry_estimated_cost_usd, 0.0) AS guard_retry_estimated_cost_usd
             FROM base_rollup b
             LEFT JOIN guard_retry_rollup g ON g.aggregate_api_id = b.aggregate_api_id
             ORDER BY
                b.estimated_cost_usd + COALESCE(g.guard_retry_estimated_cost_usd, 0.0) DESC,
                b.total_tokens + COALESCE(g.guard_retry_total_tokens, 0) DESC,
                b.aggregate_api_id ASC",
            retry_action_sql = GUARD_RETRY_ACTION_SQL
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query((start_ts, end_ts, start_ts, end_ts))?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            let input_tokens = row.get::<_, i64>(4)?;
            let cached_input_tokens = row.get::<_, i64>(5)?;
            let cache_write_input_tokens = row.get::<_, i64>(6)?;
            let billable_input_tokens = input_tokens
                .saturating_sub(cached_input_tokens)
                .saturating_sub(cache_write_input_tokens);
            let total_tokens = row.get::<_, i64>(8)?;
            let estimated_cost_usd = row.get::<_, f64>(10)?;
            let guard_retry_total_tokens = row.get::<_, i64>(11)?;
            let guard_retry_estimated_cost_usd = row.get::<_, f64>(12)?;
            items.push(AggregateApiDailyUsageSummary {
                aggregate_api_id: row.get(0)?,
                aggregate_api_supplier_name: row.get(1)?,
                aggregate_api_url: row.get(2)?,
                request_count: row.get(3)?,
                input_tokens,
                cached_input_tokens,
                cache_write_input_tokens,
                billable_input_tokens,
                output_tokens: row.get(7)?,
                total_tokens,
                reasoning_output_tokens: row.get(9)?,
                estimated_cost_usd,
                guard_retry_total_tokens,
                guard_retry_estimated_cost_usd,
                billable_total_tokens: total_tokens.saturating_add(guard_retry_total_tokens),
                billable_estimated_cost_usd: estimated_cost_usd + guard_retry_estimated_cost_usd,
                cache_hit_rate: cache_hit_rate(input_tokens, cached_input_tokens),
            });
        }
        Ok(items)
    }

    pub fn summarize_request_token_stats_by_model_for_keys(
        &self,
        start_ts: Option<i64>,
        end_ts: Option<i64>,
        key_ids: &[String],
    ) -> Result<Vec<TokenUsageSummary>> {
        self.summarize_request_token_stats_by_model_filtered(start_ts, end_ts, Some(key_ids))
    }

    fn summarize_request_token_stats_by_model_filtered(
        &self,
        start_ts: Option<i64>,
        end_ts: Option<i64>,
        key_ids: Option<&[String]>,
    ) -> Result<Vec<TokenUsageSummary>> {
        let Some(key_ids) = key_ids else {
            return self.query_request_token_stats_by_model(start_ts, end_ts, None);
        };
        let Some(key_filter) = TempKeyIdFilter::create(self, key_ids)? else {
            return Ok(Vec::new());
        };
        self.query_request_token_stats_by_model(start_ts, end_ts, Some(&key_filter))
    }

    fn query_request_token_stats_by_model(
