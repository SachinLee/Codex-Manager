use rusqlite::{params, OptionalExtension, Result};

use super::{
    now_ts, AggregateApiHealthConfig, AggregateApiHealthEvent, AggregateApiHealthState, Storage,
};

const DEFAULT_INTERVAL_SECS: i64 = 15 * 60;
const DEFAULT_TIMEOUT_MS: i64 = 30_000;

fn scope(value: Option<&str>) -> String {
    value.unwrap_or("").trim().to_string()
}

fn map_config(row: &rusqlite::Row<'_>) -> Result<AggregateApiHealthConfig> {
    Ok(AggregateApiHealthConfig {
        aggregate_api_id: row.get(0)?,
        enabled: row.get(1)?,
        probe_interval_secs: row.get(2)?,
        probe_timeout_ms: row.get(3)?,
        probe_model: row.get(4)?,
        last_scheduled_at: row.get(5)?,
        next_probe_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn map_state(row: &rusqlite::Row<'_>) -> Result<AggregateApiHealthState> {
    Ok(AggregateApiHealthState {
        aggregate_api_id: row.get(0)?,
        upstream_model: row
            .get::<_, String>(1)?
            .is_empty()
            .then(|| None)
            .unwrap_or_else(|| row.get(1).unwrap_or_default()),
        protocol: row
            .get::<_, String>(2)?
            .is_empty()
            .then(|| None)
            .unwrap_or_else(|| row.get(2).unwrap_or_default()),
        state: row.get(3)?,
        consecutive_failures: row.get(4)?,
        consecutive_successes: row.get(5)?,
        failure_threshold: row.get(6)?,
        cooldown_until: row.get(7)?,
        half_open_at: row.get(8)?,
        last_observed_at: row.get(9)?,
        last_probe_at: row.get(10)?,
        last_success_at: row.get(11)?,
        last_failure_at: row.get(12)?,
        last_latency_ms: row.get(13)?,
        last_http_status: row.get(14)?,
        last_error_category: row.get(15)?,
        last_error_reason: row.get(16)?,
        last_observation_source: row.get(17)?,
        updated_at: row.get(18)?,
    })
}

impl Storage {
    pub fn aggregate_api_health_config(&self, api_id: &str) -> Result<AggregateApiHealthConfig> {
        let sql = "SELECT aggregate_api_id, enabled, probe_interval_secs, probe_timeout_ms, probe_model, last_scheduled_at, next_probe_at, updated_at FROM aggregate_api_health_configs WHERE aggregate_api_id = ?1";
        self.conn
            .query_row(sql, [api_id], map_config)
            .optional()
            .map(|config| {
                config.unwrap_or(AggregateApiHealthConfig {
                    aggregate_api_id: api_id.to_string(),
                    enabled: false,
                    probe_interval_secs: DEFAULT_INTERVAL_SECS,
                    probe_timeout_ms: DEFAULT_TIMEOUT_MS,
                    probe_model: None,
                    last_scheduled_at: None,
                    next_probe_at: None,
                    updated_at: 0,
                })
            })
    }

    pub fn list_enabled_aggregate_api_health_configs(
        &self,
        due_before: i64,
    ) -> Result<Vec<AggregateApiHealthConfig>> {
        let mut statement = self.conn.prepare("SELECT c.aggregate_api_id, c.enabled, c.probe_interval_secs, c.probe_timeout_ms, c.probe_model, c.last_scheduled_at, c.next_probe_at, c.updated_at FROM aggregate_api_health_configs c JOIN aggregate_apis a ON a.id = c.aggregate_api_id WHERE c.enabled = 1 AND a.status = 'active' AND (c.next_probe_at IS NULL OR c.next_probe_at <= ?1) ORDER BY COALESCE(c.next_probe_at, 0), c.aggregate_api_id")?;
        statement.query_map([due_before], map_config)?.collect()
    }

    pub fn upsert_aggregate_api_health_config(
        &self,
        config: &AggregateApiHealthConfig,
    ) -> Result<()> {
        self.conn.execute("INSERT INTO aggregate_api_health_configs (aggregate_api_id, enabled, probe_interval_secs, probe_timeout_ms, probe_model, last_scheduled_at, next_probe_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT(aggregate_api_id) DO UPDATE SET enabled=excluded.enabled, probe_interval_secs=excluded.probe_interval_secs, probe_timeout_ms=excluded.probe_timeout_ms, probe_model=excluded.probe_model, last_scheduled_at=excluded.last_scheduled_at, next_probe_at=excluded.next_probe_at, updated_at=excluded.updated_at", params![config.aggregate_api_id, config.enabled, config.probe_interval_secs, config.probe_timeout_ms, config.probe_model, config.last_scheduled_at, config.next_probe_at, config.updated_at])?;
        Ok(())
    }

    pub fn update_aggregate_api_health_schedule(
        &self,
        api_id: &str,
        last_scheduled_at: i64,
        next_probe_at: i64,
    ) -> Result<()> {
        self.conn.execute("UPDATE aggregate_api_health_configs SET last_scheduled_at=?2, next_probe_at=?3, updated_at=?2 WHERE aggregate_api_id=?1", params![api_id, last_scheduled_at, next_probe_at])?;
        Ok(())
    }

    pub fn aggregate_api_health_state(
        &self,
        api_id: &str,
        model: Option<&str>,
        protocol: Option<&str>,
    ) -> Result<Option<AggregateApiHealthState>> {
        self.conn.query_row("SELECT aggregate_api_id, upstream_model, protocol, state, consecutive_failures, consecutive_successes, failure_threshold, cooldown_until, half_open_at, last_observed_at, last_probe_at, last_success_at, last_failure_at, last_latency_ms, last_http_status, last_error_category, last_error_reason, last_observation_source, updated_at FROM aggregate_api_health_states WHERE aggregate_api_id=?1 AND upstream_model=?2 AND protocol=?3", params![api_id, scope(model), scope(protocol)], map_state).optional()
    }

    pub fn list_aggregate_api_health_states(
        &self,
        api_id: &str,
    ) -> Result<Vec<AggregateApiHealthState>> {
        let mut statement = self.conn.prepare("SELECT aggregate_api_id, upstream_model, protocol, state, consecutive_failures, consecutive_successes, failure_threshold, cooldown_until, half_open_at, last_observed_at, last_probe_at, last_success_at, last_failure_at, last_latency_ms, last_http_status, last_error_category, last_error_reason, last_observation_source, updated_at FROM aggregate_api_health_states WHERE aggregate_api_id=?1 ORDER BY upstream_model, protocol")?;
        statement.query_map([api_id], map_state)?.collect()
    }

    pub fn save_aggregate_api_health_observation(
        &self,
        state: &AggregateApiHealthState,
        event: &AggregateApiHealthEvent,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("INSERT INTO aggregate_api_health_states (aggregate_api_id, upstream_model, protocol, state, consecutive_failures, consecutive_successes, failure_threshold, cooldown_until, half_open_at, last_observed_at, last_probe_at, last_success_at, last_failure_at, last_latency_ms, last_http_status, last_error_category, last_error_reason, last_observation_source, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19) ON CONFLICT(aggregate_api_id, upstream_model, protocol) DO UPDATE SET state=excluded.state, consecutive_failures=excluded.consecutive_failures, consecutive_successes=excluded.consecutive_successes, failure_threshold=excluded.failure_threshold, cooldown_until=excluded.cooldown_until, half_open_at=excluded.half_open_at, last_observed_at=excluded.last_observed_at, last_probe_at=excluded.last_probe_at, last_success_at=excluded.last_success_at, last_failure_at=excluded.last_failure_at, last_latency_ms=excluded.last_latency_ms, last_http_status=excluded.last_http_status, last_error_category=excluded.last_error_category, last_error_reason=excluded.last_error_reason, last_observation_source=excluded.last_observation_source, updated_at=excluded.updated_at", params![state.aggregate_api_id, scope(state.upstream_model.as_deref()), scope(state.protocol.as_deref()), state.state, state.consecutive_failures, state.consecutive_successes, state.failure_threshold, state.cooldown_until, state.half_open_at, state.last_observed_at, state.last_probe_at, state.last_success_at, state.last_failure_at, state.last_latency_ms, state.last_http_status, state.last_error_category, state.last_error_reason, state.last_observation_source, state.updated_at])?;
        tx.execute("INSERT INTO aggregate_api_health_events (aggregate_api_id, upstream_model, protocol, trigger, outcome, state_before, state_after, error_category, http_status, latency_ms, reason, observed_at, cooldown_until) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)", params![event.aggregate_api_id, event.upstream_model, event.protocol, event.trigger, event.outcome, event.state_before, event.state_after, event.error_category, event.http_status, event.latency_ms, event.reason, event.observed_at, event.cooldown_until])?;
        tx.execute("DELETE FROM aggregate_api_health_events WHERE aggregate_api_id=?1 AND (observed_at < ?2 OR id NOT IN (SELECT id FROM aggregate_api_health_events WHERE aggregate_api_id=?1 ORDER BY observed_at DESC, id DESC LIMIT 500))", params![state.aggregate_api_id, now_ts() - 30 * 24 * 60 * 60])?;
        tx.commit()
    }

    pub fn list_aggregate_api_health_events(
        &self,
        api_id: &str,
        limit: i64,
    ) -> Result<Vec<AggregateApiHealthEvent>> {
        let mut statement = self.conn.prepare("SELECT aggregate_api_id, upstream_model, protocol, trigger, outcome, state_before, state_after, error_category, http_status, latency_ms, reason, observed_at, cooldown_until FROM aggregate_api_health_events WHERE aggregate_api_id=?1 ORDER BY observed_at DESC, id DESC LIMIT ?2")?;
        statement
            .query_map(params![api_id, limit.clamp(1, 200)], |row| {
                Ok(AggregateApiHealthEvent {
                    aggregate_api_id: row.get(0)?,
                    upstream_model: row.get(1)?,
                    protocol: row.get(2)?,
                    trigger: row.get(3)?,
                    outcome: row.get(4)?,
                    state_before: row.get(5)?,
                    state_after: row.get(6)?,
                    error_category: row.get(7)?,
                    http_status: row.get(8)?,
                    latency_ms: row.get(9)?,
                    reason: row.get(10)?,
                    observed_at: row.get(11)?,
                    cooldown_until: row.get(12)?,
                })
            })?
            .collect()
    }

    pub fn reset_aggregate_api_health_state(
        &self,
        api_id: &str,
        model: Option<&str>,
        protocol: Option<&str>,
    ) -> Result<()> {
        let mut sql =
            String::from("DELETE FROM aggregate_api_health_states WHERE aggregate_api_id=?1");
        if model.is_some() {
            sql.push_str(" AND upstream_model=?2");
        }
        if protocol.is_some() {
            sql.push_str(if model.is_some() {
                " AND protocol=?3"
            } else {
                " AND protocol=?2"
            });
        }
        match (model, protocol) {
            (Some(model), Some(protocol)) => {
                self.conn.execute(
                    &sql,
                    params![api_id, scope(Some(model)), scope(Some(protocol))],
                )?;
            }
            (Some(model), None) => {
                self.conn
                    .execute(&sql, params![api_id, scope(Some(model))])?;
            }
            (None, Some(protocol)) => {
                self.conn
                    .execute(&sql, params![api_id, scope(Some(protocol))])?;
            }
            (None, None) => {
                self.conn.execute(&sql, [api_id])?;
            }
        }
        Ok(())
    }
}
