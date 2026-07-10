use rusqlite::{params, params_from_iter, Result, Row};

use super::{
    GatewayReasoningGuardAggregateApiStat, GatewayReasoningGuardEvent,
    GatewayReasoningGuardTraceSummary, Storage,
};

pub(super) const GUARD_RETRY_ACTION_SQL: &str =
    "action IN ('internal_retry', 'continuation_recovery')";

impl Storage {
    pub fn ensure_gateway_reasoning_guard_events_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS gateway_reasoning_guard_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                trace_id TEXT,
                request_log_id INTEGER,
                mode TEXT NOT NULL,
                action TEXT NOT NULL,
                target_token INTEGER,
                source_kind TEXT,
                source_id TEXT,
                supplier_name TEXT,
                upstream_model TEXT,
                request_path TEXT,
                attempt_index INTEGER NOT NULL DEFAULT 0,
                final_status_code INTEGER,
                created_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_reasoning_guard_events_created_at
                ON gateway_reasoning_guard_events(created_at DESC);
             CREATE INDEX IF NOT EXISTS idx_reasoning_guard_events_source_created_at
                ON gateway_reasoning_guard_events(source_kind, source_id, created_at DESC);
             CREATE INDEX IF NOT EXISTS idx_reasoning_guard_events_trace_id
                ON gateway_reasoning_guard_events(trace_id);",
        )?;
        self.ensure_column("gateway_reasoning_guard_events", "input_tokens", "INTEGER")?;
        self.ensure_column(
            "gateway_reasoning_guard_events",
            "cached_input_tokens",
            "INTEGER",
        )?;
        self.ensure_column("gateway_reasoning_guard_events", "output_tokens", "INTEGER")?;
        self.ensure_column("gateway_reasoning_guard_events", "total_tokens", "INTEGER")?;
        self.ensure_column(
            "gateway_reasoning_guard_events",
            "reasoning_output_tokens",
            "INTEGER",
        )?;
        self.ensure_column(
            "gateway_reasoning_guard_events",
            "estimated_cost_usd",
            "REAL",
        )?;
        Ok(())
    }

    pub fn insert_gateway_reasoning_guard_event(
        &self,
        event: &GatewayReasoningGuardEvent,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO gateway_reasoning_guard_events (
                trace_id, request_log_id, mode, action, target_token, source_kind, source_id,
                supplier_name, upstream_model, request_path, attempt_index, final_status_code,
                input_tokens, cached_input_tokens, output_tokens, total_tokens,
                reasoning_output_tokens, estimated_cost_usd, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            params![
                &event.trace_id,
                event.request_log_id,
                &event.mode,
                &event.action,
                event.target_token,
                &event.source_kind,
                &event.source_id,
                &event.supplier_name,
                &event.upstream_model,
                &event.request_path,
                event.attempt_index,
                event.final_status_code,
                event.input_tokens,
                event.cached_input_tokens,
                event.output_tokens,
                event.total_tokens,
                event.reasoning_output_tokens,
                event.estimated_cost_usd,
                event.created_at,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn summarize_reasoning_guard_by_aggregate_api_between(
        &self,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<GatewayReasoningGuardAggregateApiStat>> {
        self.ensure_gateway_reasoning_guard_events_table()?;
        let sql = format!(
            "WITH request_counts AS (
                SELECT actual_source_id AS aggregate_api_id, COUNT(1) AS total_request_count
                FROM request_logs
                WHERE actual_source_kind = 'aggregate_api'
                  AND actual_source_id IS NOT NULL
                  AND created_at >= ?1
                  AND created_at < ?2
                GROUP BY actual_source_id
             ),
             event_rollup AS (
                SELECT
                    source_id AS aggregate_api_id,
                    MAX(supplier_name) AS supplier_name,
                    COUNT(1) AS event_count,
                    COUNT(DISTINCT COALESCE(trace_id, CAST(id AS TEXT))) AS affected_request_count,
                    SUM(CASE WHEN {retry_action_sql} THEN 1 ELSE 0 END) AS internal_retry_count,
                    COUNT(DISTINCT CASE WHEN {retry_action_sql} THEN COALESCE(trace_id, CAST(id AS TEXT)) END) AS internal_retry_request_count,
                    SUM(CASE WHEN action = 'block' THEN 1 ELSE 0 END) AS block_count,
                    COUNT(DISTINCT CASE WHEN action = 'block' THEN COALESCE(trace_id, CAST(id AS TEXT)) END) AS blocked_request_count,
                    SUM(CASE WHEN action = 'observe_only' THEN 1 ELSE 0 END) AS observe_only_count,
                    SUM(CASE WHEN action = 'bypass_after_consecutive' THEN 1 ELSE 0 END) AS bypass_after_consecutive_count,
                    SUM(CASE WHEN action = 'recovered' THEN 1 ELSE 0 END) AS recovered_count,
                    SUM(COALESCE(input_tokens, 0)) AS guard_input_tokens,
                    SUM(COALESCE(cached_input_tokens, 0)) AS guard_cached_input_tokens,
                    SUM(COALESCE(output_tokens, 0)) AS guard_output_tokens,
                    SUM(COALESCE(total_tokens, 0)) AS guard_total_tokens,
                    SUM(COALESCE(reasoning_output_tokens, 0)) AS guard_reasoning_output_tokens,
                    SUM(COALESCE(estimated_cost_usd, 0.0)) AS guard_estimated_cost_usd,
                    MAX(created_at) AS last_event_at
                FROM gateway_reasoning_guard_events
                WHERE source_kind = 'aggregate_api'
                  AND source_id IS NOT NULL
                  AND created_at >= ?1
                  AND created_at < ?2
                GROUP BY source_id
             ),
             latest_event AS (
                SELECT source_id AS aggregate_api_id, target_token AS last_target_token
                FROM gateway_reasoning_guard_events latest
                WHERE latest.source_kind = 'aggregate_api'
                  AND latest.source_id IS NOT NULL
                  AND latest.target_token IS NOT NULL
                  AND latest.created_at >= ?1
                  AND latest.created_at < ?2
                  AND latest.id = (
                    SELECT inner_latest.id
                    FROM gateway_reasoning_guard_events inner_latest
                    WHERE inner_latest.source_kind = 'aggregate_api'
                      AND inner_latest.source_id = latest.source_id
                      AND inner_latest.target_token IS NOT NULL
                      AND inner_latest.created_at >= ?1
                      AND inner_latest.created_at < ?2
                    ORDER BY inner_latest.created_at DESC, inner_latest.id DESC
                    LIMIT 1
                  )
             ),
             aggregate_api_keys AS (
                SELECT aggregate_api_id FROM request_counts
                UNION
                SELECT aggregate_api_id FROM event_rollup
             )
             SELECT
                k.aggregate_api_id AS aggregate_api_id,
                COALESCE(e.supplier_name, a.supplier_name) AS supplier_name,
                a.url AS aggregate_api_url,
                COALESCE(r.total_request_count, 0) AS total_request_count,
                COALESCE(e.event_count, 0) AS event_count,
                COALESCE(e.affected_request_count, 0) AS affected_request_count,
                COALESCE(e.internal_retry_count, 0) AS internal_retry_count,
                COALESCE(e.internal_retry_request_count, 0) AS internal_retry_request_count,
                COALESCE(e.block_count, 0) AS block_count,
                COALESCE(e.blocked_request_count, 0) AS blocked_request_count,
                COALESCE(e.observe_only_count, 0) AS observe_only_count,
                COALESCE(e.bypass_after_consecutive_count, 0) AS bypass_after_consecutive_count,
                COALESCE(e.recovered_count, 0) AS recovered_count,
                COALESCE(e.guard_input_tokens, 0) AS guard_input_tokens,
                COALESCE(e.guard_cached_input_tokens, 0) AS guard_cached_input_tokens,
                COALESCE(e.guard_output_tokens, 0) AS guard_output_tokens,
                COALESCE(e.guard_total_tokens, 0) AS guard_total_tokens,
                COALESCE(e.guard_reasoning_output_tokens, 0) AS guard_reasoning_output_tokens,
                COALESCE(e.guard_estimated_cost_usd, 0.0) AS guard_estimated_cost_usd,
                l.last_target_token,
                e.last_event_at
             FROM aggregate_api_keys k
             LEFT JOIN request_counts r ON r.aggregate_api_id = k.aggregate_api_id
              LEFT JOIN event_rollup e ON e.aggregate_api_id = k.aggregate_api_id
              LEFT JOIN latest_event l ON l.aggregate_api_id = k.aggregate_api_id
              LEFT JOIN aggregate_apis a ON a.id = k.aggregate_api_id
              ORDER BY (e.last_event_at IS NULL) ASC, e.last_event_at DESC, total_request_count DESC, aggregate_api_id ASC",
            retry_action_sql = GUARD_RETRY_ACTION_SQL
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![start_ts, end_ts], map_reasoning_guard_stat_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn summarize_reasoning_guard_by_trace_ids(
        &self,
        trace_ids: &[String],
    ) -> Result<Vec<GatewayReasoningGuardTraceSummary>> {
        self.ensure_gateway_reasoning_guard_events_table()?;
        if trace_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = std::iter::repeat_n("?", trace_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "WITH event_rollup AS (
                SELECT
                    trace_id,
                    COUNT(1) AS event_count,
                    SUM(CASE WHEN {retry_action_sql} THEN 1 ELSE 0 END) AS internal_retry_count,
                    SUM(CASE WHEN action = 'block' THEN 1 ELSE 0 END) AS block_count,
                    SUM(CASE WHEN action = 'recovered' THEN 1 ELSE 0 END) AS recovered_count,
                    SUM(CASE WHEN {retry_action_sql} THEN COALESCE(total_tokens, 0) ELSE 0 END) AS retry_total_tokens,
                    SUM(CASE WHEN {retry_action_sql} THEN COALESCE(estimated_cost_usd, 0.0) ELSE 0.0 END) AS retry_estimated_cost_usd
                FROM gateway_reasoning_guard_events
                WHERE trace_id IN ({placeholders})
                GROUP BY trace_id
            ),
            latest_event AS (
                SELECT trace_id, action, target_token
                FROM gateway_reasoning_guard_events latest
                WHERE trace_id IN ({placeholders})
                  AND latest.id = (
                    SELECT inner_latest.id
                    FROM gateway_reasoning_guard_events inner_latest
                    WHERE inner_latest.trace_id = latest.trace_id
                    ORDER BY inner_latest.created_at DESC, inner_latest.id DESC
                    LIMIT 1
                  )
            )
            SELECT
                r.trace_id,
                r.event_count,
                r.internal_retry_count,
                r.block_count,
                r.recovered_count,
                r.retry_total_tokens,
                r.retry_estimated_cost_usd,
                l.action,
                l.target_token
            FROM event_rollup r
            LEFT JOIN latest_event l ON l.trace_id = r.trace_id",
            retry_action_sql = GUARD_RETRY_ACTION_SQL
        );
        let mut values = Vec::with_capacity(trace_ids.len() * 2);
        values.extend(trace_ids.iter().map(String::as_str));
        values.extend(trace_ids.iter().map(String::as_str));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(values), map_trace_summary_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

fn map_reasoning_guard_stat_row(row: &Row<'_>) -> Result<GatewayReasoningGuardAggregateApiStat> {
    Ok(GatewayReasoningGuardAggregateApiStat {
        aggregate_api_id: row.get(0)?,
        aggregate_api_supplier_name: row.get(1)?,
        aggregate_api_url: row.get(2)?,
        total_request_count: row.get(3)?,
        event_count: row.get(4)?,
        affected_request_count: row.get(5)?,
        internal_retry_count: row.get(6)?,
        internal_retry_request_count: row.get(7)?,
        block_count: row.get(8)?,
        blocked_request_count: row.get(9)?,
        observe_only_count: row.get(10)?,
        bypass_after_consecutive_count: row.get(11)?,
        recovered_count: row.get(12)?,
        guard_input_tokens: row.get(13)?,
        guard_cached_input_tokens: row.get(14)?,
        guard_output_tokens: row.get(15)?,
        guard_total_tokens: row.get(16)?,
        guard_reasoning_output_tokens: row.get(17)?,
        guard_estimated_cost_usd: row.get(18)?,
        last_target_token: row.get(19)?,
        last_event_at: row.get(20)?,
    })
}

fn map_trace_summary_row(row: &Row<'_>) -> Result<GatewayReasoningGuardTraceSummary> {
    Ok(GatewayReasoningGuardTraceSummary {
        trace_id: row.get(0)?,
        event_count: row.get(1)?,
        internal_retry_count: row.get(2)?,
        block_count: row.get(3)?,
        recovered_count: row.get(4)?,
        retry_total_tokens: row.get(5)?,
        retry_estimated_cost_usd: row.get(6)?,
        last_action: row.get(7)?,
        last_target_token: row.get(8)?,
    })
}
