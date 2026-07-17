use rusqlite::{params, Result};

use super::{
    GatewayCapabilityObservationRecord, GatewayCapabilityOverrideRecord, GatewayCapabilityScope,
    GatewayUpstreamAttemptEvent, Storage,
};

impl Storage {
    pub(super) fn ensure_gateway_capability_tables(&self) -> Result<()> {
        self.conn.execute_batch(include_str!(
            "../../migrations/116_gateway_capability_routing.sql"
        ))
    }

    pub fn upsert_gateway_capability_override(
        &self,
        value: &GatewayCapabilityOverrideRecord,
    ) -> Result<()> {
        self.ensure_gateway_capability_tables()?;
        self.conn.execute(
            "INSERT INTO gateway_capability_overrides (
                source_kind, source_id, upstream_model_pattern, protocol, capability_key,
                state, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(source_kind, source_id, upstream_model_pattern, protocol, capability_key)
             DO UPDATE SET state = excluded.state, updated_at = excluded.updated_at",
            params![
                value.scope.source_kind,
                value.scope.source_id,
                value.scope.upstream_model_pattern,
                value.scope.protocol,
                value.scope.capability_key,
                value.state,
                value.created_at,
                value.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_gateway_capability_override(
        &self,
        scope: &GatewayCapabilityScope,
    ) -> Result<usize> {
        self.ensure_gateway_capability_tables()?;
        self.conn.execute(
            "DELETE FROM gateway_capability_overrides
             WHERE source_kind = ?1 AND source_id = ?2 AND upstream_model_pattern = ?3
               AND protocol = ?4 AND capability_key = ?5",
            params![
                scope.source_kind,
                scope.source_id,
                scope.upstream_model_pattern,
                scope.protocol,
                scope.capability_key,
            ],
        )
    }

    pub fn list_gateway_capability_overrides(
        &self,
        source_kind: &str,
        source_id: &str,
    ) -> Result<Vec<GatewayCapabilityOverrideRecord>> {
        self.ensure_gateway_capability_tables()?;
        let mut stmt = self.conn.prepare(
            "SELECT source_kind, source_id, upstream_model_pattern, protocol, capability_key,
                    state, created_at, updated_at
             FROM gateway_capability_overrides
             WHERE source_kind = ?1 AND source_id = ?2
             ORDER BY capability_key, upstream_model_pattern, protocol",
        )?;
        let rows = stmt.query_map(params![source_kind, source_id], |row| {
            Ok(GatewayCapabilityOverrideRecord {
                scope: GatewayCapabilityScope {
                    source_kind: row.get(0)?,
                    source_id: row.get(1)?,
                    upstream_model_pattern: row.get(2)?,
                    protocol: row.get(3)?,
                    capability_key: row.get(4)?,
                },
                state: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?;
        rows.collect()
    }

    pub fn upsert_gateway_capability_observation(
        &self,
        value: &GatewayCapabilityObservationRecord,
    ) -> Result<()> {
        self.ensure_gateway_capability_tables()?;
        self.conn.execute(
            "INSERT INTO gateway_capability_observations (
                source_kind, source_id, upstream_model_pattern, protocol, capability_key,
                state, observation_source, confidence, evidence_code, first_observed_at,
                last_observed_at, expires_at, occurrence_count
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(
                source_kind, source_id, upstream_model_pattern, protocol, capability_key,
                state, observation_source, evidence_code
             ) DO UPDATE SET
                confidence = excluded.confidence,
                last_observed_at = excluded.last_observed_at,
                expires_at = excluded.expires_at,
                occurrence_count = gateway_capability_observations.occurrence_count + 1",
            params![
                value.scope.source_kind,
                value.scope.source_id,
                value.scope.upstream_model_pattern,
                value.scope.protocol,
                value.scope.capability_key,
                value.state,
                value.observation_source,
                value.confidence,
                value.evidence_code,
                value.first_observed_at,
                value.last_observed_at,
                value.expires_at,
                value.occurrence_count.max(1),
            ],
        )?;
        Ok(())
    }

    pub fn list_gateway_capability_observations(
        &self,
        source_kind: &str,
        source_id: &str,
        now: i64,
    ) -> Result<Vec<GatewayCapabilityObservationRecord>> {
        self.ensure_gateway_capability_tables()?;
        let mut stmt = self.conn.prepare(
            "SELECT id, source_kind, source_id, upstream_model_pattern, protocol, capability_key,
                    state, observation_source, confidence, evidence_code, first_observed_at,
                    last_observed_at, expires_at, occurrence_count
             FROM gateway_capability_observations
             WHERE source_kind = ?1 AND source_id = ?2 AND expires_at > ?3
             ORDER BY capability_key, last_observed_at DESC, id DESC",
        )?;
        let rows = stmt.query_map(params![source_kind, source_id, now], |row| {
            Ok(GatewayCapabilityObservationRecord {
                id: row.get(0)?,
                scope: GatewayCapabilityScope {
                    source_kind: row.get(1)?,
                    source_id: row.get(2)?,
                    upstream_model_pattern: row.get(3)?,
                    protocol: row.get(4)?,
                    capability_key: row.get(5)?,
                },
                state: row.get(6)?,
                observation_source: row.get(7)?,
                confidence: row.get(8)?,
                evidence_code: row.get(9)?,
                first_observed_at: row.get(10)?,
                last_observed_at: row.get(11)?,
                expires_at: row.get(12)?,
                occurrence_count: row.get(13)?,
            })
        })?;
        rows.collect()
    }

    pub fn clear_gateway_capability_observations(
        &self,
        scope: &GatewayCapabilityScope,
    ) -> Result<usize> {
        self.ensure_gateway_capability_tables()?;
        self.conn.execute(
            "DELETE FROM gateway_capability_observations
             WHERE source_kind = ?1 AND source_id = ?2 AND upstream_model_pattern = ?3
               AND protocol = ?4 AND capability_key = ?5",
            params![
                scope.source_kind,
                scope.source_id,
                scope.upstream_model_pattern,
                scope.protocol,
                scope.capability_key,
            ],
        )
    }

    pub fn prune_expired_gateway_capability_observations(&self, now: i64) -> Result<usize> {
        self.ensure_gateway_capability_tables()?;
        self.conn.execute(
            "DELETE FROM gateway_capability_observations WHERE expires_at <= ?1",
            [now],
        )
    }

    pub fn insert_gateway_upstream_attempt_event(
        &self,
        event: &GatewayUpstreamAttemptEvent,
    ) -> Result<i64> {
        self.ensure_gateway_capability_tables()?;
        self.conn.execute(
            "INSERT INTO gateway_upstream_attempt_events (
                trace_id, request_log_id, attempt_index, phase, source_kind, source_id,
                supplier_name, upstream_model, protocol, request_path, contract_signature,
                capability_decisions_json, transform_codes_json, error_class, error_code,
                http_status, duration_ms, outcome, delivery_started, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                       ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                event.trace_id,
                event.request_log_id,
                event.attempt_index,
                event.phase,
                event.source_kind,
                event.source_id,
                event.supplier_name,
                event.upstream_model,
                event.protocol,
                event.request_path,
                event.contract_signature,
                event.capability_decisions_json,
                event.transform_codes_json,
                event.error_class,
                event.error_code,
                event.http_status,
                event.duration_ms,
                event.outcome,
                i64::from(event.delivery_started),
                event.created_at,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_gateway_upstream_attempt_events(
        &self,
        source_kind: &str,
        source_id: &str,
        limit: i64,
    ) -> Result<Vec<GatewayUpstreamAttemptEvent>> {
        self.ensure_gateway_capability_tables()?;
        let mut stmt = self.conn.prepare(
            "SELECT id, trace_id, request_log_id, attempt_index, phase, source_kind, source_id,
                    supplier_name, upstream_model, protocol, request_path, contract_signature,
                    capability_decisions_json, transform_codes_json, error_class, error_code,
                    http_status, duration_ms, outcome, delivery_started, created_at
             FROM gateway_upstream_attempt_events
             WHERE source_kind = ?1 AND source_id = ?2
             ORDER BY created_at DESC, id DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            params![source_kind, source_id, limit.clamp(1, 200)],
            |row| {
                Ok(GatewayUpstreamAttemptEvent {
                    id: row.get(0)?,
                    trace_id: row.get(1)?,
                    request_log_id: row.get(2)?,
                    attempt_index: row.get(3)?,
                    phase: row.get(4)?,
                    source_kind: row.get(5)?,
                    source_id: row.get(6)?,
                    supplier_name: row.get(7)?,
                    upstream_model: row.get(8)?,
                    protocol: row.get(9)?,
                    request_path: row.get(10)?,
                    contract_signature: row.get(11)?,
                    capability_decisions_json: row.get(12)?,
                    transform_codes_json: row.get(13)?,
                    error_class: row.get(14)?,
                    error_code: row.get(15)?,
                    http_status: row.get(16)?,
                    duration_ms: row.get(17)?,
                    outcome: row.get(18)?,
                    delivery_started: row.get::<_, i64>(19)? != 0,
                    created_at: row.get(20)?,
                })
            },
        )?;
        rows.collect()
    }

    pub fn prune_gateway_upstream_attempt_events_by_retention(&self, now: i64) -> Result<usize> {
        self.ensure_gateway_capability_tables()?;
        let days = super::request_logs::request_log_retention_days();
        if days <= 0 {
            return Ok(0);
        }
        let cutoff = now.saturating_sub(days.saturating_mul(86_400));
        self.conn.execute(
            "DELETE FROM gateway_upstream_attempt_events WHERE created_at < ?1",
            [cutoff],
        )
    }
}

#[cfg(test)]
#[path = "gateway_capabilities_tests.rs"]
mod tests;
