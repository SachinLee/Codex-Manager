const fs = require("fs");
const path = "crates/core/src/storage/request_token_stats.rs";
let s = fs.readFileSync(path, "utf8");
if (!s.includes("ModelDailyUsageSummary")) {
  s = s.replace(
    "AccountDailyUsageSummary, AggregateApiDailyUsageSummary,",
    "AccountDailyUsageSummary, AggregateApiDailyUsageSummary, ModelDailyUsageSummary,"
  );
}
if (s.includes("summarize_request_token_stats_by_model_between")) {
  fs.writeFileSync(path, s);
  console.log("method already exists");
  process.exit(0);
}
const method = String.raw`
    pub fn summarize_request_token_stats_by_model_between(
        &self,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<ModelDailyUsageSummary>> {
        let mut stmt = self.conn.prepare(
            "WITH all_stats AS (
                SELECT COALESCE(NULLIF(TRIM(model), ''), 'unknown') AS model,
                       input_tokens, cached_input_tokens, cache_write_input_tokens,
                       output_tokens, total_tokens, reasoning_output_tokens, estimated_cost_usd,
                       1 AS request_count
                  FROM request_token_stats
                 WHERE created_at >= ?1 AND created_at < ?2
                UNION ALL
                SELECT COALESCE(NULLIF(TRIM(model), ''), 'unknown') AS model,
                       input_tokens, cached_input_tokens, cache_write_input_tokens,
                       output_tokens, total_tokens, reasoning_output_tokens, estimated_cost_usd,
                       request_count
                  FROM request_token_stat_hourly_rollups
                 WHERE bucket_start >= ?3 AND bucket_end <= ?4
             )
             SELECT
                model,
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
             GROUP BY model
             ORDER BY estimated_cost_usd DESC, total_tokens DESC, model ASC",
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
            items.push(ModelDailyUsageSummary {
                model: row.get(0)?,
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
`;
const marker = "    pub fn summarize_request_token_stats_by_aggregate_api_between(";
const i = s.indexOf(marker);
if (i < 0) throw new Error("marker missing");
fs.writeFileSync(path, s.slice(0, i) + method + "\n" + s.slice(i));
console.log("method added");
