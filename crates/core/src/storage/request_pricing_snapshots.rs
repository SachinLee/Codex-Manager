use rusqlite::{params, params_from_iter, types::Value, Result};

use super::{RequestPricingSnapshot, Storage};

impl Storage {
    pub fn insert_request_pricing_snapshot(&self, snapshot: &RequestPricingSnapshot) -> Result<()> {
        self.conn.execute(
            "INSERT INTO request_pricing_snapshots (
                request_log_id, billing_mode, context_band, long_context_threshold_tokens,
                long_context_threshold_inclusive,
                matched_rule_id, matched_pattern, price_source, match_quality, price_status,
                cost_source, provider_cost_usd_ticks, provider_cost_usd, local_estimated_cost_usd,
                pricing_variance_usd,
                plain_input_cost_usd, cached_input_cost_usd, cache_write_cost_usd, output_cost_usd,
                total_cost_usd, short_baseline_cost_usd, long_context_uplift_usd, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)
             ON CONFLICT(request_log_id) DO UPDATE SET
                billing_mode = excluded.billing_mode,
                context_band = excluded.context_band,
                long_context_threshold_tokens = excluded.long_context_threshold_tokens,
                long_context_threshold_inclusive = excluded.long_context_threshold_inclusive,
                matched_rule_id = excluded.matched_rule_id,
                matched_pattern = excluded.matched_pattern,
                price_source = excluded.price_source,
                match_quality = excluded.match_quality,
                price_status = excluded.price_status,
                cost_source = excluded.cost_source,
                provider_cost_usd_ticks = excluded.provider_cost_usd_ticks,
                provider_cost_usd = excluded.provider_cost_usd,
                local_estimated_cost_usd = excluded.local_estimated_cost_usd,
                pricing_variance_usd = excluded.pricing_variance_usd,
                plain_input_cost_usd = excluded.plain_input_cost_usd,
                cached_input_cost_usd = excluded.cached_input_cost_usd,
                cache_write_cost_usd = excluded.cache_write_cost_usd,
                output_cost_usd = excluded.output_cost_usd,
                total_cost_usd = excluded.total_cost_usd,
                short_baseline_cost_usd = excluded.short_baseline_cost_usd,
                long_context_uplift_usd = excluded.long_context_uplift_usd,
                created_at = excluded.created_at",
            params![
                snapshot.request_log_id,
                snapshot.billing_mode,
                snapshot.context_band,
                snapshot.long_context_threshold_tokens,
                snapshot.long_context_threshold_inclusive.map(i64::from),
                snapshot.matched_rule_id,
                snapshot.matched_pattern,
                snapshot.price_source,
                snapshot.match_quality,
                snapshot.price_status,
                snapshot.cost_source,
                snapshot.provider_cost_usd_ticks,
                snapshot.provider_cost_usd,
                snapshot.local_estimated_cost_usd,
                snapshot.pricing_variance_usd,
                snapshot.plain_input_cost_usd,
                snapshot.cached_input_cost_usd,
                snapshot.cache_write_cost_usd,
                snapshot.output_cost_usd,
                snapshot.total_cost_usd,
                snapshot.short_baseline_cost_usd,
                snapshot.long_context_uplift_usd,
                snapshot.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_request_pricing_snapshots_for_trace_ids(
        &self,
        trace_ids: &[String],
    ) -> Result<Vec<(String, RequestPricingSnapshot)>> {
        if trace_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = std::iter::repeat("?")
            .take(trace_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT r.trace_id,
                p.request_log_id, p.billing_mode, p.context_band, p.long_context_threshold_tokens,
                p.long_context_threshold_inclusive,
                p.matched_rule_id, p.matched_pattern, p.price_source, p.match_quality, p.price_status,
                p.cost_source, p.provider_cost_usd_ticks, p.provider_cost_usd, p.local_estimated_cost_usd,
                p.pricing_variance_usd,
                p.plain_input_cost_usd, p.cached_input_cost_usd, p.cache_write_cost_usd, p.output_cost_usd,
                p.total_cost_usd, p.short_baseline_cost_usd, p.long_context_uplift_usd, p.created_at
             FROM request_pricing_snapshots p
             JOIN request_logs r ON r.id = p.request_log_id
             WHERE r.trace_id IN ({placeholders})"
        );
        let params = trace_ids.iter().cloned().map(Value::Text).collect::<Vec<_>>();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(params.iter()), |row| {
            Ok((
                row.get(0)?,
                RequestPricingSnapshot {
                    request_log_id: row.get(1)?,
                    billing_mode: row.get(2)?,
                    context_band: row.get(3)?,
                    long_context_threshold_tokens: row.get(4)?,
                    long_context_threshold_inclusive: row.get::<_, Option<i64>>(5)?.map(|v| v != 0),
                    matched_rule_id: row.get(6)?,
                    matched_pattern: row.get(7)?,
                    price_source: row.get(8)?,
                    match_quality: row.get(9)?,
                    price_status: row.get(10)?,
                    cost_source: row.get(11)?,
                    provider_cost_usd_ticks: row.get(12)?,
                    provider_cost_usd: row.get(13)?,
                    local_estimated_cost_usd: row.get(14)?,
                    pricing_variance_usd: row.get(15)?,
                    plain_input_cost_usd: row.get(16)?,
                    cached_input_cost_usd: row.get(17)?,
                    cache_write_cost_usd: row.get(18)?,
                    output_cost_usd: row.get(19)?,
                    total_cost_usd: row.get(20)?,
                    short_baseline_cost_usd: row.get(21)?,
                    long_context_uplift_usd: row.get(22)?,
                    created_at: row.get(23)?,
                },
            ))
        })?;
        rows.collect()
    }

    pub(super) fn ensure_request_pricing_snapshots_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS request_pricing_snapshots (
                request_log_id INTEGER PRIMARY KEY,
                billing_mode TEXT NOT NULL,
                context_band TEXT NOT NULL,
                long_context_threshold_tokens INTEGER,
                long_context_threshold_inclusive INTEGER,
                matched_rule_id TEXT,
                matched_pattern TEXT,
                price_source TEXT,
                match_quality TEXT,
                price_status TEXT NOT NULL,
                cost_source TEXT,
                provider_cost_usd_ticks INTEGER,
                provider_cost_usd REAL,
                local_estimated_cost_usd REAL,
                pricing_variance_usd REAL,
                plain_input_cost_usd REAL,
                cached_input_cost_usd REAL,
                cache_write_cost_usd REAL,
                output_cost_usd REAL,
                total_cost_usd REAL,
                short_baseline_cost_usd REAL,
                long_context_uplift_usd REAL,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_request_pricing_snapshots_context_band_created_at
                ON request_pricing_snapshots(context_band, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_request_pricing_snapshots_price_status_created_at
                ON request_pricing_snapshots(price_status, created_at DESC);",
        )?;
        self.ensure_column(
            "request_pricing_snapshots",
            "long_context_threshold_inclusive",
            "INTEGER",
        )?;
        self.ensure_column("request_pricing_snapshots", "cost_source", "TEXT")?;
        self.ensure_column(
            "request_pricing_snapshots",
            "provider_cost_usd_ticks",
            "INTEGER",
        )?;
        self.ensure_column("request_pricing_snapshots", "provider_cost_usd", "REAL")?;
        self.ensure_column(
            "request_pricing_snapshots",
            "local_estimated_cost_usd",
            "REAL",
        )?;
        self.ensure_column(
            "request_pricing_snapshots",
            "pricing_variance_usd",
            "REAL",
        )?;
        Ok(())
    }
}
