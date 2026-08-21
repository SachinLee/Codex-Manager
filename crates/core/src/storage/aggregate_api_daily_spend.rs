//! Durable per-API / per-local-day spend budget used by Aggregate API daily
//! enforcement.
//!
//! The service storage pool can open several SQLite connections, so admission
//! uses `BEGIN IMMEDIATE` transactions (the vendored rusqlite
//! `unchecked_transaction`) to serialize the read-modify-write decision across
//! connections. All amounts are non-negative integer micro-USD.

use rusqlite::{params, OptionalExtension, Result, Transaction};
use std::time::Duration;

use super::{now_ts, Storage};

const DAILY_SPEND_LOCK_RETRY_DELAY: Duration = Duration::from_millis(25);
const DAILY_SPEND_LOCK_MAX_ATTEMPTS: usize = 4;

/// Reservations older than this horizon are reclaimed to `held` on the next
/// reserve call. Held value remains committed to the day instead of being
/// silently dropped after a crash or an uncertain timeout.
pub const DAILY_SPEND_RESERVATION_HOLD_AFTER_SECS: i64 = 15 * 60;

pub const SPEND_ATTEMPT_KIND_INITIAL: &str = "initial";
pub const SPEND_ATTEMPT_KIND_TRANSPORT_RETRY: &str = "transport_retry";
pub const SPEND_ATTEMPT_KIND_CAPACITY_RETRY: &str = "capacity_retry";
pub const SPEND_ATTEMPT_KIND_GUARD_RETRY: &str = "guard_retry";
pub const SPEND_ATTEMPT_KIND_CONTINUATION_RECOVERY: &str = "continuation_recovery";

pub const SPEND_PRICING_QUOTED: &str = "quoted";
pub const SPEND_PRICING_UNBOUNDED_OUTPUT: &str = "unbounded_output";
pub const SPEND_PRICING_UNPRICED_MODEL: &str = "unpriced_model";
pub const SPEND_PRICING_PROVIDER_REPORTED: &str = "provider_reported";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregateApiDailySpendBucket {
    pub aggregate_api_id: String,
    pub day_start_ts: i64,
    pub opening_spend_microusd: i64,
    pub settled_spend_microusd: i64,
    pub reserved_spend_microusd: i64,
    pub held_spend_microusd: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregateApiDailySpendSummary {
    pub aggregate_api_id: String,
    pub day_start_ts: i64,
    pub opening_spend_microusd: i64,
    pub settled_spend_microusd: i64,
    pub reserved_spend_microusd: i64,
    pub held_spend_microusd: i64,
    pub committed_spend_microusd: i64,
    pub limit_microusd: Option<i64>,
    pub remaining_microusd: Option<i64>,
    pub over_limit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregateApiSpendReservation {
    pub attempt_id: String,
    pub aggregate_api_id: String,
    pub day_start_ts: i64,
    pub reserved_microusd: i64,
    pub pricing_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggregateApiSpendReserveOutcome {
    /// No limit configured and no bucket exists yet: nothing to track.
    NotTracked,
    Granted(AggregateApiSpendReservation),
    Rejected {
        remaining_microusd: i64,
    },
}

fn is_retryable_lock_error(error: &rusqlite::Error) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("database is locked")
        || message.contains("database table is locked")
        || message.contains("database is busy")
        || message.contains("sqlite_busy")
        || message.contains("sqlite_locked")
}

/// Run `f` inside an IMMEDIATE transaction, retrying transient write locks.
fn with_immediate_tx<T>(storage: &Storage, f: impl Fn(&Transaction<'_>) -> Result<T>) -> Result<T> {
    for attempt in 0..DAILY_SPEND_LOCK_MAX_ATTEMPTS {
        let tx = storage.conn.unchecked_transaction()?;
        match f(&tx) {
            Ok(value) => {
                tx.commit()?;
                return Ok(value);
            }
            Err(error)
                if is_retryable_lock_error(&error)
                    && attempt + 1 < DAILY_SPEND_LOCK_MAX_ATTEMPTS =>
            {
                std::thread::sleep(DAILY_SPEND_LOCK_RETRY_DELAY);
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("immediate transaction retry loop always returns")
}

fn legacy_daily_spend_microusd_on(
    tx: &Transaction<'_>,
    aggregate_api_id: &str,
    day_start_ts: i64,
    day_end_ts: i64,
) -> Result<i64> {
    let snapshot_microusd: i64 = tx.query_row(
        "SELECT COALESCE(SUM(s.charged_cost_microusd), 0)
         FROM request_charge_snapshots s
         LEFT JOIN request_token_stats t ON t.request_log_id=s.request_log_id
         LEFT JOIN request_logs r ON r.id=s.request_log_id
         WHERE s.created_at >= ?2 AND s.created_at < ?3
           AND COALESCE(
                NULLIF(TRIM(t.aggregate_api_id), ''),
                CASE WHEN r.actual_source_kind='aggregate_api'
                     THEN NULLIF(TRIM(r.actual_source_id), '') END,
                NULLIF(TRIM(r.initial_aggregate_api_id), '')
           ) = ?1",
        params![aggregate_api_id, day_start_ts, day_end_ts],
        |row| row.get(0),
    )?;
    let guard_usd: f64 = tx.query_row(
        "SELECT COALESCE(SUM(estimated_cost_usd), 0.0)
         FROM gateway_reasoning_guard_events
         WHERE action IN ('internal_retry','continuation_recovery')
           AND source_kind = 'aggregate_api'
           AND source_id = ?1
           AND created_at >= ?2 AND created_at < ?3",
        params![aggregate_api_id, day_start_ts, day_end_ts],
        |row| row.get(0),
    )?;
    let guard_microusd = if guard_usd.is_finite() && guard_usd > 0.0 {
        (guard_usd * 1_000_000.0).ceil().max(0.0) as i64
    } else {
        0
    };
    Ok(snapshot_microusd.saturating_add(guard_microusd))
}

fn reclaim_stale_reservations_on(
    storage: &Storage,
    aggregate_api_id: &str,
    day_start_ts: i64,
    now: i64,
) -> Result<()> {
    let cutoff = now.saturating_sub(DAILY_SPEND_RESERVATION_HOLD_AFTER_SECS);
    let stale: Vec<(String, i64)> = {
        let mut statement = storage.conn.prepare(
            "SELECT id, reserved_microusd
             FROM aggregate_api_daily_spend_attempts
             WHERE aggregate_api_id=?1 AND day_start_ts=?2
               AND state='reserved' AND created_at < ?3",
        )?;
        let rows = statement.query_map(params![aggregate_api_id, day_start_ts, cutoff], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
        rows.collect::<Result<Vec<_>>>()?
    };
    for (attempt_id, amount) in stale {
        storage.conn.execute(
            "UPDATE aggregate_api_daily_spend_buckets
             SET reserved_spend_microusd = reserved_spend_microusd - ?3,
                 held_spend_microusd = held_spend_microusd + ?3,
                 updated_at = ?4
             WHERE aggregate_api_id=?1 AND day_start_ts=?2",
            params![aggregate_api_id, day_start_ts, amount, now],
        )?;
        storage.conn.execute(
            "UPDATE aggregate_api_daily_spend_attempts
             SET state='held', resolved_at=?2 WHERE id=?1",
            params![attempt_id, now],
        )?;
    }
    Ok(())
}

fn insert_attempt_on(
    tx: &Transaction<'_>,
    attempt_id: &str,
    aggregate_api_id: &str,
    day_start_ts: i64,
    trace_id: Option<&str>,
    attempt_kind: &str,
    pricing_state: &str,
    reserved_microusd: i64,
    now: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO aggregate_api_daily_spend_attempts (
            id, aggregate_api_id, day_start_ts, trace_id, attempt_kind, state,
            pricing_state, reserved_microusd, settled_microusd, request_log_id,
            created_at, resolved_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, 'reserved', ?6, ?7, NULL, NULL, ?8, NULL)",
        params![
            attempt_id,
            aggregate_api_id,
            day_start_ts,
            trace_id,
            attempt_kind,
            pricing_state,
            reserved_microusd,
            now
        ],
    )?;
    Ok(())
}

impl Storage {
    /// Sum the page-equivalent daily spend for an API using existing immutable
    /// rows: finalized charge snapshots plus Guard retry observability events.
    pub fn aggregate_api_legacy_daily_spend_microusd(
        &self,
        aggregate_api_id: &str,
        day_start_ts: i64,
        day_end_ts: i64,
    ) -> Result<i64> {
        if day_end_ts <= day_start_ts {
            return Ok(0);
        }
        let tx = self.conn.unchecked_transaction()?;
        let result =
            legacy_daily_spend_microusd_on(&tx, aggregate_api_id, day_start_ts, day_end_ts);
        tx.commit()?;
        result
    }

    /// Atomically create/refresh the day bucket and reserve `quote_microusd` for
    /// one upstream attempt. Returns `NotTracked` when there is no limit and no
    /// bucket yet, `Granted` when the reservation fits, or `Rejected` when it
    /// exceeds the remaining budget. When a limit was cleared mid-day but the
    /// bucket already exists, tracking continues without admission rejection.
    #[allow(clippy::too_many_arguments)]
    pub fn reserve_aggregate_api_daily_spend(
        &self,
        aggregate_api_id: &str,
        day_start_ts: i64,
        day_end_ts: i64,
        limit_microusd: Option<i64>,
        attempt_id: &str,
        trace_id: Option<&str>,
        attempt_kind: &str,
        pricing_state: &str,
        quote_microusd: i64,
    ) -> Result<AggregateApiSpendReserveOutcome> {
        let quote_microusd = quote_microusd.max(0);
        let now = now_ts();
        with_immediate_tx(self, |tx| {
            let bucket_exists = tx
                .query_row(
                    "SELECT 1 FROM aggregate_api_daily_spend_buckets
                     WHERE aggregate_api_id=?1 AND day_start_ts=?2",
                    params![aggregate_api_id, day_start_ts],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !bucket_exists && limit_microusd.is_none() {
                return Ok(AggregateApiSpendReserveOutcome::NotTracked);
            }

            reclaim_stale_reservations_on(self, aggregate_api_id, day_start_ts, now)?;

            let (opening, settled, reserved, held) = if bucket_exists {
                tx.query_row(
                    "SELECT opening_spend_microusd, settled_spend_microusd,
                            reserved_spend_microusd, held_spend_microusd
                     FROM aggregate_api_daily_spend_buckets
                     WHERE aggregate_api_id=?1 AND day_start_ts=?2",
                    params![aggregate_api_id, day_start_ts],
                    |row| -> Result<(i64, i64, i64, i64)> {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                    },
                )?
            } else {
                let opening =
                    legacy_daily_spend_microusd_on(tx, aggregate_api_id, day_start_ts, day_end_ts)?;
                tx.execute(
                    "INSERT INTO aggregate_api_daily_spend_buckets (
                        aggregate_api_id, day_start_ts, opening_spend_microusd,
                        settled_spend_microusd, reserved_spend_microusd,
                        held_spend_microusd, created_at, updated_at
                     ) VALUES (?1, ?2, ?3, 0, 0, 0, ?4, ?4)",
                    params![aggregate_api_id, day_start_ts, opening, now],
                )?;
                (opening, 0, 0, 0)
            };

            let committed = opening
                .saturating_add(settled)
                .saturating_add(reserved)
                .saturating_add(held);

            let Some(limit) = limit_microusd else {
                insert_attempt_on(
                    tx,
                    attempt_id,
                    aggregate_api_id,
                    day_start_ts,
                    trace_id,
                    attempt_kind,
                    pricing_state,
                    quote_microusd,
                    now,
                )?;
                tx.execute(
                    "UPDATE aggregate_api_daily_spend_buckets
                     SET reserved_spend_microusd = reserved_spend_microusd + ?3,
                         updated_at = ?4
                     WHERE aggregate_api_id=?1 AND day_start_ts=?2",
                    params![aggregate_api_id, day_start_ts, quote_microusd, now],
                )?;
                return Ok(AggregateApiSpendReserveOutcome::Granted(
                    AggregateApiSpendReservation {
                        attempt_id: attempt_id.to_string(),
                        aggregate_api_id: aggregate_api_id.to_string(),
                        day_start_ts,
                        reserved_microusd: quote_microusd,
                        pricing_state: pricing_state.to_string(),
                    },
                ));
            };

            let remaining = limit.saturating_sub(committed);
            if quote_microusd > remaining {
                return Ok(AggregateApiSpendReserveOutcome::Rejected {
                    remaining_microusd: remaining,
                });
            }

            insert_attempt_on(
                tx,
                attempt_id,
                aggregate_api_id,
                day_start_ts,
                trace_id,
                attempt_kind,
                pricing_state,
                quote_microusd,
                now,
            )?;
            tx.execute(
                "UPDATE aggregate_api_daily_spend_buckets
                 SET reserved_spend_microusd = reserved_spend_microusd + ?3,
                     updated_at = ?4
                 WHERE aggregate_api_id=?1 AND day_start_ts=?2",
                params![aggregate_api_id, day_start_ts, quote_microusd, now],
            )?;
            Ok(AggregateApiSpendReserveOutcome::Granted(
                AggregateApiSpendReservation {
                    attempt_id: attempt_id.to_string(),
                    aggregate_api_id: aggregate_api_id.to_string(),
                    day_start_ts,
                    reserved_microusd: quote_microusd,
                    pricing_state: pricing_state.to_string(),
                },
            ))
        })
    }

    /// Move a reservation to its actual charged amount. Idempotent; a settled
    /// or released attempt is never charged twice.
    pub fn settle_aggregate_api_daily_spend_attempt(
        &self,
        attempt_id: &str,
        settled_microusd: i64,
        request_log_id: Option<i64>,
    ) -> Result<bool> {
        let settled_microusd = settled_microusd.max(0);
        let now = now_ts();
        with_immediate_tx(self, |tx| {
            let Some((api_id, day_start, state, reserved)) = tx
                .query_row(
                    "SELECT aggregate_api_id, day_start_ts, state, reserved_microusd
                     FROM aggregate_api_daily_spend_attempts WHERE id=?1",
                    params![attempt_id],
                    |row| -> Result<(String, i64, String, i64)> {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                    },
                )
                .optional()?
            else {
                return Ok(false);
            };
            match state.as_str() {
                "reserved" => {
                    tx.execute(
                        "UPDATE aggregate_api_daily_spend_buckets
                         SET settled_spend_microusd = settled_spend_microusd + ?3,
                             reserved_spend_microusd = reserved_spend_microusd - ?4,
                             updated_at = ?5
                         WHERE aggregate_api_id=?1 AND day_start_ts=?2",
                        params![api_id, day_start, settled_microusd, reserved, now],
                    )?;
                }
                "held" => {
                    tx.execute(
                        "UPDATE aggregate_api_daily_spend_buckets
                         SET settled_spend_microusd = settled_spend_microusd + ?3,
                             held_spend_microusd = held_spend_microusd - ?4,
                             updated_at = ?5
                         WHERE aggregate_api_id=?1 AND day_start_ts=?2",
                        params![api_id, day_start, settled_microusd, reserved, now],
                    )?;
                }
                _ => return Ok(false),
            }
            tx.execute(
                "UPDATE aggregate_api_daily_spend_attempts
                 SET state='settled', settled_microusd=?2, request_log_id=?3, resolved_at=?4
                 WHERE id=?1",
                params![attempt_id, settled_microusd, request_log_id, now],
            )?;
            Ok(true)
        })
    }

    /// Idempotently release a reservation that definitively reached no
    /// billable upstream execution.
    pub fn release_aggregate_api_daily_spend_attempt(&self, attempt_id: &str) -> Result<bool> {
        let now = now_ts();
        with_immediate_tx(self, |tx| {
            let Some((api_id, day_start, state, reserved)) = tx
                .query_row(
                    "SELECT aggregate_api_id, day_start_ts, state, reserved_microusd
                     FROM aggregate_api_daily_spend_attempts WHERE id=?1",
                    params![attempt_id],
                    |row| -> Result<(String, i64, String, i64)> {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                    },
                )
                .optional()?
            else {
                return Ok(false);
            };
            match state.as_str() {
                "reserved" => {
                    tx.execute(
                        "UPDATE aggregate_api_daily_spend_buckets
                         SET reserved_spend_microusd = reserved_spend_microusd - ?3,
                             updated_at = ?4
                         WHERE aggregate_api_id=?1 AND day_start_ts=?2",
                        params![api_id, day_start, reserved, now],
                    )?;
                }
                "held" => {
                    tx.execute(
                        "UPDATE aggregate_api_daily_spend_buckets
                         SET held_spend_microusd = held_spend_microusd - ?3,
                             updated_at = ?4
                         WHERE aggregate_api_id=?1 AND day_start_ts=?2",
                        params![api_id, day_start, reserved, now],
                    )?;
                }
                _ => return Ok(false),
            }
            tx.execute(
                "UPDATE aggregate_api_daily_spend_attempts
                 SET state='released', resolved_at=?2 WHERE id=?1",
                params![attempt_id, now],
            )?;
            Ok(true)
        })
    }

    /// Idempotently move an ambiguous in-flight attempt to `held`. Held value
    /// remains committed to the day rather than silently disappearing.
    pub fn hold_aggregate_api_daily_spend_attempt(&self, attempt_id: &str) -> Result<bool> {
        let now = now_ts();
        with_immediate_tx(self, |tx| {
            let Some((api_id, day_start, state, reserved)) = tx
                .query_row(
                    "SELECT aggregate_api_id, day_start_ts, state, reserved_microusd
                     FROM aggregate_api_daily_spend_attempts WHERE id=?1",
                    params![attempt_id],
                    |row| -> Result<(String, i64, String, i64)> {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                    },
                )
                .optional()?
            else {
                return Ok(false);
            };
            if state != "reserved" {
                return Ok(false);
            }
            tx.execute(
                "UPDATE aggregate_api_daily_spend_buckets
                 SET reserved_spend_microusd = reserved_spend_microusd - ?3,
                     held_spend_microusd = held_spend_microusd + ?3,
                     updated_at = ?4
                 WHERE aggregate_api_id=?1 AND day_start_ts=?2",
                params![api_id, day_start, reserved, now],
            )?;
            tx.execute(
                "UPDATE aggregate_api_daily_spend_attempts
                 SET state='held', resolved_at=?2 WHERE id=?1",
                params![attempt_id, now],
            )?;
            Ok(true)
        })
    }

    fn map_summary(row: &rusqlite::Row<'_>) -> Result<AggregateApiDailySpendSummary> {
        let aggregate_api_id: String = row.get(0)?;
        let day_start_ts: i64 = row.get(1)?;
        let opening: i64 = row.get(2)?;
        let settled: i64 = row.get(3)?;
        let reserved: i64 = row.get(4)?;
        let held: i64 = row.get(5)?;
        let limit_usd: Option<f64> = row.get(6)?;
        let limit_microusd = limit_usd.and_then(|value| {
            if value.is_finite() && value > 0.0 {
                let scaled = value * 1_000_000.0;
                (scaled.is_finite() && scaled <= i64::MAX as f64).then_some(scaled.ceil() as i64)
            } else {
                None
            }
        });
        let committed = opening
            .saturating_add(settled)
            .saturating_add(reserved)
            .saturating_add(held);
        let remaining_microusd = limit_microusd.map(|limit| limit.saturating_sub(committed));
        let over_limit = limit_microusd.is_some_and(|limit| committed > limit);
        Ok(AggregateApiDailySpendSummary {
            aggregate_api_id,
            day_start_ts,
            opening_spend_microusd: opening,
            settled_spend_microusd: settled,
            reserved_spend_microusd: reserved,
            held_spend_microusd: held,
            committed_spend_microusd: committed,
            limit_microusd,
            remaining_microusd,
            over_limit,
        })
    }

    /// Read the budget summary for one API/day bucket, when it exists.
    pub fn read_aggregate_api_daily_spend_summary(
        &self,
        aggregate_api_id: &str,
        day_start_ts: i64,
    ) -> Result<Option<AggregateApiDailySpendSummary>> {
        self.conn
            .query_row(
                "SELECT b.aggregate_api_id, b.day_start_ts,
                        b.opening_spend_microusd, b.settled_spend_microusd,
                        b.reserved_spend_microusd, b.held_spend_microusd,
                        a.daily_spend_limit_usd
                 FROM aggregate_api_daily_spend_buckets b
                 LEFT JOIN aggregate_apis a ON a.id = b.aggregate_api_id
                 WHERE b.aggregate_api_id=?1 AND b.day_start_ts=?2",
                params![aggregate_api_id, day_start_ts],
                Self::map_summary,
            )
            .optional()
    }

    /// List budget summaries for every API with a bucket whose day starts in
    /// `[start_ts, end_ts)`.
    pub fn list_aggregate_api_daily_spend_summaries_between(
        &self,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<AggregateApiDailySpendSummary>> {
        if end_ts <= start_ts {
            return Ok(Vec::new());
        }
        let mut statement = self.conn.prepare(
            "SELECT b.aggregate_api_id, b.day_start_ts,
                    b.opening_spend_microusd, b.settled_spend_microusd,
                    b.reserved_spend_microusd, b.held_spend_microusd,
                    a.daily_spend_limit_usd
             FROM aggregate_api_daily_spend_buckets b
             LEFT JOIN aggregate_apis a ON a.id = b.aggregate_api_id
             WHERE b.day_start_ts >= ?1 AND b.day_start_ts < ?2
             ORDER BY b.aggregate_api_id, b.day_start_ts",
        )?;
        let rows = statement.query_map(params![start_ts, end_ts], Self::map_summary)?;
        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{
        AggregateApi, ChargeSnapshotInputV2, GatewayReasoningGuardEvent, RequestLog,
        RequestTokenStat,
    };

    fn aggregate_api(id: &str, limit_usd: Option<f64>) -> AggregateApi {
        let now = now_ts();
        AggregateApi {
            id: id.to_string(),
            provider_type: "codex".to_string(),
            supplier_name: Some(id.to_string()),
            sort: 0,
            url: format!("https://{id}.example.test"),
            auth_type: "apikey".to_string(),
            auth_params_json: None,
            action: None,
            model_override: Some("gpt-5.4".to_string()),
            cost_multiplier: 1.0,
            daily_spend_limit_usd: limit_usd,
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
            last_test_at: None,
            last_test_status: None,
            last_test_error: None,
            balance_query_enabled: false,
            balance_query_template: None,
            balance_query_base_url: None,
            balance_query_user_id: None,
            balance_query_config_json: None,
            last_balance_at: None,
            last_balance_status: None,
            last_balance_error: None,
            last_balance_json: None,
            enable_consecutive_failure_freeze: true,
            upstream_protocol: None,
        }
    }

    fn seed_charged_request(storage: &Storage, api_id: &str, at: i64) -> i64 {
        let log_id = storage
            .insert_request_log(&RequestLog {
                request_path: "/v1/responses".to_string(),
                method: "POST".to_string(),
                actual_source_kind: Some("aggregate_api".into()),
                actual_source_id: Some(api_id.to_string()),
                status_code: Some(200),
                created_at: at,
                ..Default::default()
            })
            .expect("insert request log");
        storage
            .insert_request_token_stat(&RequestTokenStat {
                request_log_id: log_id,
                aggregate_api_id: Some(api_id.to_string()),
                model: Some("gpt-5.4".into()),
                created_at: at,
                ..Default::default()
            })
            .expect("insert token stat");
        let snapshot = storage
            .record_charge_snapshot_v2(&ChargeSnapshotInputV2 {
                request_log_id: log_id,
                model_slug: "gpt-5.4".into(),
                usage_source: "actual".into(),
                input_tokens: 100,
                output_tokens: 10,
                rate_multiplier_millis: 1_000,
                ..Default::default()
            })
            .expect("record charge snapshot");
        snapshot.charged_cost_microusd
    }

    fn seed_guard_retry_event(storage: &Storage, api_id: &str, at: i64, cost_usd: f64) {
        storage
            .insert_gateway_reasoning_guard_event(&GatewayReasoningGuardEvent {
                trace_id: Some("trace-guard".to_string()),
                request_log_id: None,
                mode: "non_stream".to_string(),
                action: "internal_retry".to_string(),
                target_token: Some(1034),
                source_kind: Some("aggregate_api".to_string()),
                source_id: Some(api_id.to_string()),
                supplier_name: Some(api_id.to_string()),
                upstream_model: Some("gpt-5.4".to_string()),
                request_path: Some("/v1/responses".to_string()),
                attempt_index: 0,
                final_status_code: Some(200),
                input_tokens: Some(100),
                cached_input_tokens: None,
                output_tokens: Some(10),
                total_tokens: None,
                reasoning_output_tokens: Some(1034),
                estimated_cost_usd: Some(cost_usd),
                created_at: at,
            })
            .expect("insert guard event");
    }

    fn isolated_db_path(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "codexmanager-daily-spend-{label}-{}-{nanos}.db",
            std::process::id()
        ))
    }

    #[test]
    fn bucket_captures_legacy_opening_and_grants_bounded_quote() {
        let storage = Storage::open_in_memory().expect("open storage");
        storage.init().expect("init storage");
        let now = now_ts();
        let day_start = now - now.rem_euclid(86_400);
        storage
            .insert_aggregate_api(&aggregate_api("agg-a", Some(98.0)))
            .expect("insert api");

        let charged = seed_charged_request(&storage, "agg-a", day_start + 60);
        seed_guard_retry_event(&storage, "agg-a", day_start + 120, 0.25);
        let guard_microusd = 250_000;
        assert!(charged > 0);

        let outcome = storage
            .reserve_aggregate_api_daily_spend(
                "agg-a",
                day_start,
                day_start + 86_400,
                Some(98_000_000),
                "att-1",
                Some("trace-1"),
                SPEND_ATTEMPT_KIND_INITIAL,
                SPEND_PRICING_QUOTED,
                1_000_000,
            )
            .expect("reserve");
        match outcome {
            AggregateApiSpendReserveOutcome::Granted(reservation) => {
                assert_eq!(reservation.attempt_id, "att-1");
                assert_eq!(reservation.reserved_microusd, 1_000_000);
            }
            other => panic!("expected grant, got {other:?}"),
        }

        let summary = storage
            .read_aggregate_api_daily_spend_summary("agg-a", day_start)
            .expect("read summary")
            .expect("summary exists");
        assert_eq!(summary.opening_spend_microusd, charged + guard_microusd);
        assert_eq!(summary.settled_spend_microusd, 0);
        assert_eq!(summary.reserved_spend_microusd, 1_000_000);
        assert_eq!(summary.held_spend_microusd, 0);
        assert_eq!(summary.limit_microusd, Some(98_000_000));
        assert_eq!(
            summary.remaining_microusd,
            Some(98_000_000 - (charged + guard_microusd + 1_000_000))
        );
        assert!(!summary.over_limit);
    }

    #[test]
    fn rejects_quote_exceeding_remaining_and_tracks_without_limit_after_bucket_exists() {
        let storage = Storage::open_in_memory().expect("open storage");
        storage.init().expect("init storage");
        let now = now_ts();
        let day_start = now - now.rem_euclid(86_400);
        storage
            .insert_aggregate_api(&aggregate_api("agg-a", Some(1.0)))
            .expect("insert api");
        seed_charged_request(&storage, "agg-a", day_start + 60);
        // Seed a guard event so opening is small but nonzero.
        seed_guard_retry_event(&storage, "agg-a", day_start + 120, 0.01);

        let outcome = storage
            .reserve_aggregate_api_daily_spend(
                "agg-a",
                day_start,
                day_start + 86_400,
                Some(1_000_000),
                "att-1",
                None,
                SPEND_ATTEMPT_KIND_INITIAL,
                SPEND_PRICING_QUOTED,
                3_000_000,
            )
            .expect("reserve");
        assert!(matches!(
            outcome,
            AggregateApiSpendReserveOutcome::Rejected { .. }
        ));

        // A zero quote (unpriced model) is never rejected.
        let outcome = storage
            .reserve_aggregate_api_daily_spend(
                "agg-a",
                day_start,
                day_start + 86_400,
                Some(1_000_000),
                "att-2",
                None,
                SPEND_ATTEMPT_KIND_INITIAL,
                SPEND_PRICING_UNPRICED_MODEL,
                0,
            )
            .expect("reserve");
        assert!(matches!(
            outcome,
            AggregateApiSpendReserveOutcome::Granted(_)
        ));

        // Clearing the limit keeps tracking but no longer rejects.
        let outcome = storage
            .reserve_aggregate_api_daily_spend(
                "agg-a",
                day_start,
                day_start + 86_400,
                None,
                "att-3",
                None,
                SPEND_ATTEMPT_KIND_INITIAL,
                SPEND_PRICING_QUOTED,
                500_000,
            )
            .expect("reserve");
        assert!(matches!(
            outcome,
            AggregateApiSpendReserveOutcome::Granted(_)
        ));
    }

    #[test]
    fn settle_release_hold_transitions_are_idempotent() {
        let storage = Storage::open_in_memory().expect("open storage");
        storage.init().expect("init storage");
        let now = now_ts();
        let day_start = now - now.rem_euclid(86_400);
        storage
            .insert_aggregate_api(&aggregate_api("agg-a", Some(100.0)))
            .expect("insert api");

        for (id, amount) in [
            ("att-a", 1_000_000i64),
            ("att-b", 2_000_000),
            ("att-c", 3_000_000),
        ] {
            let outcome = storage
                .reserve_aggregate_api_daily_spend(
                    "agg-a",
                    day_start,
                    day_start + 86_400,
                    Some(100_000_000),
                    id,
                    None,
                    SPEND_ATTEMPT_KIND_INITIAL,
                    SPEND_PRICING_QUOTED,
                    amount,
                )
                .expect("reserve");
            assert!(matches!(
                outcome,
                AggregateApiSpendReserveOutcome::Granted(_)
            ));
        }

        assert!(storage
            .settle_aggregate_api_daily_spend_attempt("att-a", 1_500_000, Some(7))
            .expect("settle"));
        // Idempotent: settling again is a no-op and does not double-charge.
        assert!(!storage
            .settle_aggregate_api_daily_spend_attempt("att-a", 9_000_000, Some(7))
            .expect("settle twice"));
        assert!(storage
            .release_aggregate_api_daily_spend_attempt("att-b")
            .expect("release"));
        assert!(!storage
            .release_aggregate_api_daily_spend_attempt("att-b")
            .expect("release twice"));
        assert!(storage
            .hold_aggregate_api_daily_spend_attempt("att-c")
            .expect("hold"));
        assert!(!storage
            .hold_aggregate_api_daily_spend_attempt("att-c")
            .expect("hold twice"));

        let summary = storage
            .read_aggregate_api_daily_spend_summary("agg-a", day_start)
            .expect("read summary")
            .expect("summary exists");
        assert_eq!(summary.settled_spend_microusd, 1_500_000);
        assert_eq!(summary.reserved_spend_microusd, 0);
        assert_eq!(summary.held_spend_microusd, 3_000_000);
        assert_eq!(summary.committed_spend_microusd, 4_500_000);
    }

    #[test]
    fn cross_connection_reservation_does_not_double_spend() {
        let path = isolated_db_path("cross-connection");
        let first = Storage::open(&path).expect("open first connection");
        first.init().expect("init first connection");
        let now = now_ts();
        let day_start = now - now.rem_euclid(86_400);
        first
            .insert_aggregate_api(&aggregate_api("agg-a", Some(10.0)))
            .expect("insert api");
        seed_charged_request(&first, "agg-a", day_start + 60);

        let second = Storage::open(&path).expect("open second connection");
        // Both connections share the same file; WAL lets the second reader see
        // the first writer's committed reservation.
        let first_outcome = first
            .reserve_aggregate_api_daily_spend(
                "agg-a",
                day_start,
                day_start + 86_400,
                Some(10_000_000),
                "att-1",
                None,
                SPEND_ATTEMPT_KIND_INITIAL,
                SPEND_PRICING_QUOTED,
                3_000_000,
            )
            .expect("first reserve");
        assert!(matches!(
            first_outcome,
            AggregateApiSpendReserveOutcome::Granted(_)
        ));

        let second_outcome = second
            .reserve_aggregate_api_daily_spend(
                "agg-a",
                day_start,
                day_start + 86_400,
                Some(10_000_000),
                "att-2",
                None,
                SPEND_ATTEMPT_KIND_INITIAL,
                SPEND_PRICING_QUOTED,
                6_000_000,
            )
            .expect("second reserve");
        assert!(matches!(
            second_outcome,
            AggregateApiSpendReserveOutcome::Granted(_)
        ));

        let third_outcome = second
            .reserve_aggregate_api_daily_spend(
                "agg-a",
                day_start,
                day_start + 86_400,
                Some(10_000_000),
                "att-3",
                None,
                SPEND_ATTEMPT_KIND_INITIAL,
                SPEND_PRICING_QUOTED,
                2_000_000,
            )
            .expect("third reserve");
        assert!(matches!(
            third_outcome,
            AggregateApiSpendReserveOutcome::Rejected { .. }
        ));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }
}
