use rusqlite::{params, OptionalExtension, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::{now_ts, Storage};

const HARDENING_MIGRATION_VERSION: &str = "113_model_billing_v2_hardening";
const CHARGE_SNAPSHOT_LOCK_RETRY_DELAY: Duration = Duration::from_millis(25);

fn is_retryable_sqlite_write_error(error: &rusqlite::Error) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("database is locked")
        || message.contains("database table is locked")
        || message.contains("database is busy")
        || message.contains("sqlite_busy")
        || message.contains("sqlite_locked")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelPriceTierV2 {
    #[serde(alias = "min_input_tokens")]
    pub min_input_tokens: i64,
    #[serde(alias = "input_microusd_per_1m")]
    pub input_microusd_per_1m: i64,
    #[serde(alias = "cached_input_microusd_per_1m")]
    pub cached_input_microusd_per_1m: i64,
    #[serde(alias = "output_microusd_per_1m")]
    pub output_microusd_per_1m: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChargeComputationV2 {
    pub uncached_input_tokens: i64,
    pub numerator: i128,
    pub base_cost_microusd: i64,
    pub charged_cost_microusd: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChargeSnapshotInputV2 {
    pub request_log_id: i64,
    pub model_slug: String,
    pub usage_source: String,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub rate_multiplier_millis: i64,
    /// Controls whether this new local charge may select a non-base price tier.
    /// `None` preserves the historical default of enabling long-context billing.
    #[serde(default)]
    pub long_context_billing_enabled: Option<bool>,
    /// A validated provider-reported pre-multiplier charge, in micro-USD.
    /// When set, this remains immutable in the snapshot and is multiplied once
    /// by `rate_multiplier_millis` to produce the final charged amount.
    #[serde(default)]
    pub base_cost_override_microusd: Option<i64>,
    #[serde(default)]
    pub wallet_id: Option<String>,
    #[serde(default)]
    pub api_key_id: Option<String>,
    #[serde(default)]
    pub pricing_rule_id: Option<String>,
    #[serde(default)]
    pub raw_usage_json: Option<String>,
    #[serde(default)]
    pub ledger_note: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChargeSnapshotV2 {
    pub request_log_id: i64,
    pub model_id: Option<String>,
    pub model_slug: String,
    pub tier_min_input_tokens: i64,
    pub long_context_billing_enabled: bool,
    pub usage_source: String,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub input_microusd_per_1m: i64,
    pub cached_input_microusd_per_1m: i64,
    pub output_microusd_per_1m: i64,
    pub rate_multiplier_millis: i64,
    pub base_cost_microusd: i64,
    pub charged_cost_microusd: i64,
    pub currency: String,
    pub created_at: i64,
}

fn checked_i64(value: i128, label: &str) -> Result<i64> {
    i64::try_from(value).map_err(|_| {
        rusqlite::Error::InvalidParameterName(format!("{label} exceeds SQLite INTEGER range"))
    })
}

fn ceil_div(value: i128, divisor: i128) -> i128 {
    if value <= 0 {
        0
    } else {
        value / divisor + i128::from(value % divisor != 0)
    }
}

fn apply_rate_multiplier_millis(
    base_cost_microusd: i64,
    rate_multiplier_millis: i64,
) -> Result<i64> {
    let numerator = i128::from(base_cost_microusd)
        .checked_mul(i128::from(rate_multiplier_millis))
        .ok_or_else(|| {
            rusqlite::Error::InvalidParameterName("multiplied charge overflow".to_string())
        })?;
    checked_i64(ceil_div(numerator, 1_000), "charged cost")
}

pub fn compute_charge_v2(
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
    tier: &ModelPriceTierV2,
    rate_multiplier_millis: i64,
) -> Result<ChargeComputationV2> {
    if input_tokens < 0
        || cached_input_tokens < 0
        || output_tokens < 0
        || tier.input_microusd_per_1m < 0
        || tier.cached_input_microusd_per_1m < 0
        || tier.output_microusd_per_1m < 0
        || rate_multiplier_millis < 0
    {
        return Err(rusqlite::Error::InvalidParameterName(
            "tokens, rates, and multiplier must be non-negative".to_string(),
        ));
    }
    let uncached_input_tokens = input_tokens.saturating_sub(cached_input_tokens).max(0);
    let cached_tokens_for_charge = cached_input_tokens.min(input_tokens);
    let input_part = i128::from(uncached_input_tokens)
        .checked_mul(i128::from(tier.input_microusd_per_1m))
        .ok_or_else(|| {
            rusqlite::Error::InvalidParameterName("input charge overflow".to_string())
        })?;
    let cached_part = i128::from(cached_tokens_for_charge)
        .checked_mul(i128::from(tier.cached_input_microusd_per_1m))
        .ok_or_else(|| {
            rusqlite::Error::InvalidParameterName("cached charge overflow".to_string())
        })?;
    let output_part = i128::from(output_tokens)
        .checked_mul(i128::from(tier.output_microusd_per_1m))
        .ok_or_else(|| {
            rusqlite::Error::InvalidParameterName("output charge overflow".to_string())
        })?;
    let numerator = input_part
        .checked_add(cached_part)
        .and_then(|value| value.checked_add(output_part))
        .ok_or_else(|| rusqlite::Error::InvalidParameterName("charge overflow".to_string()))?;
    let charged_numerator = numerator
        .checked_mul(i128::from(rate_multiplier_millis))
        .ok_or_else(|| {
            rusqlite::Error::InvalidParameterName("multiplied charge overflow".to_string())
        })?;
    Ok(ChargeComputationV2 {
        uncached_input_tokens,
        numerator,
        base_cost_microusd: checked_i64(ceil_div(numerator, 1_000_000), "base cost")?,
        charged_cost_microusd: checked_i64(
            ceil_div(charged_numerator, 1_000_000_000),
            "charged cost",
        )?,
    })
}

fn map_snapshot(row: &rusqlite::Row<'_>) -> Result<ChargeSnapshotV2> {
    Ok(ChargeSnapshotV2 {
        request_log_id: row.get(0)?,
        model_id: row.get(1)?,
        model_slug: row.get(2)?,
        tier_min_input_tokens: row.get(3)?,
        long_context_billing_enabled: row.get::<_, i64>(4)? != 0,
        usage_source: row.get(5)?,
        input_tokens: row.get(6)?,
        cached_input_tokens: row.get(7)?,
        output_tokens: row.get(8)?,
        input_microusd_per_1m: row.get(9)?,
        cached_input_microusd_per_1m: row.get(10)?,
        output_microusd_per_1m: row.get(11)?,
        rate_multiplier_millis: row.get(12)?,
        base_cost_microusd: row.get(13)?,
        charged_cost_microusd: row.get(14)?,
        currency: row.get(15)?,
        created_at: row.get(16)?,
    })
}

const SNAPSHOT_SELECT: &str = "SELECT request_log_id,model_id,model_slug,tier_min_input_tokens,
    long_context_billing_enabled,usage_source,input_tokens,cached_input_tokens,output_tokens,input_microusd_per_1m,
    cached_input_microusd_per_1m,output_microusd_per_1m,rate_multiplier_millis,
    base_cost_microusd,charged_cost_microusd,currency,created_at
  FROM request_charge_snapshots";

impl Storage {
    pub(super) fn apply_model_billing_v2_hardening_migration(&self) -> Result<()> {
        if self.has_migration(HARDENING_MIGRATION_VERSION)? {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute_batch(include_str!(
            "../../migrations/113_model_billing_v2_hardening.sql"
        ))?;
        let invalid_snapshots: i64 = tx.query_row(
            "SELECT COUNT(*) FROM request_charge_snapshots
             WHERE cached_input_tokens>input_tokens OR rate_multiplier_millis<0",
            [],
            |row| row.get(0),
        )?;
        if invalid_snapshots != 0 {
            return Err(rusqlite::Error::SqliteFailure(
                (),
                Some("model billing V2 hardening smoke check failed".to_string()),
            ));
        }
        tx.execute(
            "INSERT INTO model_catalog_v2_meta(key,value)
             VALUES('billing_hardening_state','complete')
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [],
        )?;
        tx.execute(
            "INSERT INTO schema_migrations(version,applied_at) VALUES(?1,?2)",
            params![HARDENING_MIGRATION_VERSION, now_ts()],
        )?;
        tx.commit()?;
        if let Some(migrations) = self.applied_migrations.borrow_mut().as_mut() {
            migrations.insert(HARDENING_MIGRATION_VERSION.to_string());
        }
        Ok(())
    }

    pub fn select_model_price_tier_v2(
        &self,
        model_slug: &str,
        input_tokens: i64,
    ) -> Result<Option<(String, ModelPriceTierV2)>> {
        self.select_model_price_tier_with_long_context_billing_v2(model_slug, input_tokens, true)
    }

    pub fn select_model_price_tier_with_long_context_billing_v2(
        &self,
        model_slug: &str,
        input_tokens: i64,
        long_context_billing_enabled: bool,
    ) -> Result<Option<(String, ModelPriceTierV2)>> {
        if input_tokens < 0 {
            return Err(rusqlite::Error::InvalidParameterName(
                "input tokens cannot be negative".to_string(),
            ));
        }
        self.conn
            .query_row(
                "SELECT m.id,t.min_input_tokens,t.input_microusd_per_1m,
                        t.cached_input_microusd_per_1m,t.output_microusd_per_1m
                 FROM models m
                 JOIN model_prices p ON p.model_id=m.id AND p.price_status<>'missing'
                 JOIN model_price_tiers t ON t.model_id=m.id
                    AND (t.min_input_tokens=0 OR (?3<>0 AND t.min_input_tokens<?2))
                 WHERE m.slug=?1 COLLATE NOCASE
                 ORDER BY t.min_input_tokens DESC LIMIT 1",
                params![
                    model_slug.trim(),
                    input_tokens,
                    i64::from(long_context_billing_enabled)
                ],
                |row| {
                    Ok((
                        row.get(0)?,
                        ModelPriceTierV2 {
                            min_input_tokens: row.get(1)?,
                            input_microusd_per_1m: row.get(2)?,
                            cached_input_microusd_per_1m: row.get(3)?,
                            output_microusd_per_1m: row.get(4)?,
                        },
                    ))
                },
            )
            .optional()
    }

    pub fn get_charge_snapshot_v2(&self, request_log_id: i64) -> Result<Option<ChargeSnapshotV2>> {
        self.conn
            .query_row(
                &format!("{SNAPSHOT_SELECT} WHERE request_log_id=?1"),
                [request_log_id],
                map_snapshot,
            )
            .optional()
    }

    /// Returns final, immutable charged spend for an Aggregate API in the
    /// half-open interval `[start_ts, end_ts)`.  This deliberately reads charge
    /// snapshots instead of request-log visibility, so clearing observability
    /// rows never changes billing limits or audit totals.
    pub fn sum_aggregate_api_charged_spend_microusd_between(
        &self,
        aggregate_api_id: &str,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<i64> {
        let aggregate_api_id = aggregate_api_id.trim();
        if aggregate_api_id.is_empty() || end_ts <= start_ts {
            return Ok(0);
        }
        self.conn.query_row(
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
            params![aggregate_api_id, start_ts, end_ts],
            |row| row.get(0),
        )
    }

    pub fn record_charge_snapshot_v2(
        &self,
        input: &ChargeSnapshotInputV2,
    ) -> Result<ChargeSnapshotV2> {
        if !matches!(input.usage_source.as_str(), "actual" | "estimated") {
            return Err(rusqlite::Error::InvalidParameterName(
                "usage_source must be actual or estimated".to_string(),
            ));
        }
        if input.input_tokens < 0
            || input.cached_input_tokens < 0
            || input.output_tokens < 0
            || input.rate_multiplier_millis < 0
        {
            return Err(rusqlite::Error::InvalidParameterName(
                "tokens and multiplier must be non-negative".to_string(),
            ));
        }
        if input
            .base_cost_override_microusd
            .is_some_and(|cost| cost < 0)
        {
            return Err(rusqlite::Error::InvalidParameterName(
                "base_cost_override_microusd must be non-negative".to_string(),
            ));
        }

        match self.record_charge_snapshot_v2_once(input) {
            Ok(snapshot) => Ok(snapshot),
            Err(error) if is_retryable_sqlite_write_error(&error) => {
                std::thread::sleep(CHARGE_SNAPSHOT_LOCK_RETRY_DELAY);
                self.record_charge_snapshot_v2_once(input)
            }
            Err(error) => Err(error),
        }
    }

    fn record_charge_snapshot_v2_once(
        &self,
        input: &ChargeSnapshotInputV2,
    ) -> Result<ChargeSnapshotV2> {
        let tx = self.conn.unchecked_transaction()?;
        if let Some(existing) = tx
            .query_row(
                &format!("{SNAPSHOT_SELECT} WHERE request_log_id=?1"),
                [input.request_log_id],
                map_snapshot,
            )
            .optional()?
        {
            tx.execute(
                "UPDATE request_token_stats
                 SET estimated_cost_usd=CAST(?2 AS REAL)/1000000.0
                 WHERE request_log_id=?1",
                params![input.request_log_id, existing.charged_cost_microusd],
            )?;
            tx.commit()?;
            return Ok(existing);
        }
        let read_catalog_tier = || {
            tx.query_row(
                "SELECT m.id,t.min_input_tokens,t.input_microusd_per_1m,
                        t.cached_input_microusd_per_1m,t.output_microusd_per_1m
                 FROM models m JOIN model_price_tiers t ON t.model_id=m.id
                    AND (t.min_input_tokens=0 OR (?3<>0 AND t.min_input_tokens<?2))
                 WHERE m.slug=?1 COLLATE NOCASE ORDER BY t.min_input_tokens DESC LIMIT 1",
                params![
                    input.model_slug.trim(),
                    input.input_tokens,
                    i64::from(input.long_context_billing_enabled.unwrap_or(true)),
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        ModelPriceTierV2 {
                            min_input_tokens: row.get(1)?,
                            input_microusd_per_1m: row.get(2)?,
                            cached_input_microusd_per_1m: row.get(3)?,
                            output_microusd_per_1m: row.get(4)?,
                        },
                    ))
                },
            )
        };
        let (model_id, tier, base_cost_microusd, charged_cost_microusd) =
            if let Some(base_cost_microusd) = input.base_cost_override_microusd {
                // A provider-reported amount is authoritative even when the model is
                // not present in the local catalog. Keep catalog metadata when it is
                // available, but never drop a valid upstream cost because it is not.
                let (model_id, tier) = read_catalog_tier()
                    .optional()?
                    .map(|(id, tier)| (Some(id), tier))
                    .unwrap_or((None, ModelPriceTierV2::default()));
                let charged_cost_microusd =
                    apply_rate_multiplier_millis(base_cost_microusd, input.rate_multiplier_millis)?;
                (model_id, tier, base_cost_microusd, charged_cost_microusd)
            } else {
                let price_status: Option<String> = tx
                    .query_row(
                        "SELECT p.price_status FROM models m JOIN model_prices p ON p.model_id=m.id
                         WHERE m.slug=?1 COLLATE NOCASE",
                        [input.model_slug.trim()],
                        |row| row.get(0),
                    )
                    .optional()?;
                match price_status.as_deref() {
                    None => return Err(rusqlite::Error::QueryReturnedNoRows),
                    Some("missing") => {
                        return Err(rusqlite::Error::InvalidParameterName(
                            "model_price_missing".to_string(),
                        ))
                    }
                    _ => {}
                }
                let (model_id, tier) = read_catalog_tier()?;
                let computation = compute_charge_v2(
                    input.input_tokens,
                    input.cached_input_tokens,
                    input.output_tokens,
                    &tier,
                    input.rate_multiplier_millis,
                )?;
                (
                    Some(model_id),
                    tier,
                    computation.base_cost_microusd,
                    computation.charged_cost_microusd,
                )
            };
        let cached_input_tokens = input.cached_input_tokens.min(input.input_tokens);
        let now = now_ts();
        tx.execute(
            "INSERT INTO request_charge_snapshots(request_log_id,model_id,model_slug,
               tier_min_input_tokens,long_context_billing_enabled,usage_source,input_tokens,cached_input_tokens,output_tokens,
               input_microusd_per_1m,cached_input_microusd_per_1m,output_microusd_per_1m,
               rate_multiplier_millis,base_cost_microusd,charged_cost_microusd,currency,created_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,'USD',?16)",
            params![
                input.request_log_id,
                model_id,
                input.model_slug.trim(),
                tier.min_input_tokens,
                i64::from(input.long_context_billing_enabled.unwrap_or(true)),
                input.usage_source,
                input.input_tokens,
                cached_input_tokens,
                input.output_tokens,
                tier.input_microusd_per_1m,
                tier.cached_input_microusd_per_1m,
                tier.output_microusd_per_1m,
                input.rate_multiplier_millis,
                base_cost_microusd,
                charged_cost_microusd,
                now
            ],
        )?;
        tx.execute(
            "UPDATE request_token_stats
             SET estimated_cost_usd=CAST(?2 AS REAL)/1000000.0
             WHERE request_log_id=?1",
            params![input.request_log_id, charged_cost_microusd],
        )?;
        if let Some(wallet_id) = input
            .wallet_id
            .as_deref()
            .filter(|id| !id.trim().is_empty())
        {
            let prior_ledger: Option<String> = tx
                .query_row(
                    "SELECT id FROM app_wallet_ledger_entries
                 WHERE request_log_id=?1 AND entry_kind='request_charge' LIMIT 1",
                    [input.request_log_id],
                    |row| row.get(0),
                )
                .optional()?;
            if prior_ledger.is_some() {
                return Err(rusqlite::Error::InvalidParameterName(
                    "request charge ledger exists without snapshot".to_string(),
                ));
            }
            let charge = charged_cost_microusd;
            let changed = tx.execute(
                "UPDATE app_wallets SET balance_credit_micros=balance_credit_micros-?2,updated_at=?3
                 WHERE id=?1 AND status='active'
                   AND balance_credit_micros-?2>=frozen_credit_micros",
                params![wallet_id,charge,now],
            )?;
            if changed != 1 {
                return Err(rusqlite::Error::InvalidParameterName(
                    "wallet_insufficient_balance".to_string(),
                ));
            }
            let balance_after: i64 = tx.query_row(
                "SELECT balance_credit_micros FROM app_wallets WHERE id=?1",
                [wallet_id],
                |row| row.get(0),
            )?;
            tx.execute(
                "INSERT INTO app_wallet_ledger_entries(id,wallet_id,entry_kind,
                   amount_credit_micros,balance_after_credit_micros,request_log_id,api_key_id,
                   pricing_rule_id,raw_usage_json,note,created_by_user_id,created_at)
                 VALUES(?1,?2,'request_charge',?3,?4,?5,?6,?7,?8,?9,NULL,?10)",
                params![
                    format!("wl_request_{}", input.request_log_id),
                    wallet_id,
                    -charge,
                    balance_after,
                    input.request_log_id,
                    input.api_key_id,
                    input.pricing_rule_id,
                    input.raw_usage_json,
                    input.ledger_note.as_deref().unwrap_or("model_catalog_v2"),
                    now
                ],
            )?;
        }
        tx.commit()?;
        self.get_charge_snapshot_v2(input.request_log_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_formula_charges_cached_subset_once() {
        let tier = ModelPriceTierV2 {
            min_input_tokens: 0,
            input_microusd_per_1m: 2_000_000,
            cached_input_microusd_per_1m: 200_000,
            output_microusd_per_1m: 10_000_000,
        };
        let result = compute_charge_v2(100, 40, 10, &tier, 1_500).unwrap();
        assert_eq!(result.uncached_input_tokens, 60);
        assert_eq!(result.numerator, 228_000_000);
        assert_eq!(result.base_cost_microusd, 228);
        assert_eq!(result.charged_cost_microusd, 342);
        let free = compute_charge_v2(100, 40, 10, &tier, 0).unwrap();
        assert_eq!(free.base_cost_microusd, 228);
        assert_eq!(free.charged_cost_microusd, 0);
    }

    #[test]
    fn extreme_integer_inputs_return_an_error_without_panicking() {
        let tier = ModelPriceTierV2 {
            min_input_tokens: 0,
            input_microusd_per_1m: i64::MAX,
            cached_input_microusd_per_1m: i64::MAX,
            output_microusd_per_1m: i64::MAX,
        };
        let error = compute_charge_v2(i64::MAX, 0, i64::MAX, &tier, i64::MAX)
            .expect_err("overflow must be rejected");
        assert!(error.to_string().contains("overflow"));
    }

    #[test]
    fn cached_above_input_is_clamped_and_long_tier_is_strictly_above_threshold() {
        let storage = Storage::open_in_memory().unwrap();
        storage.init().unwrap();
        let (_, low) = storage
            .select_model_price_tier_v2("gpt-5.4", 271_999)
            .unwrap()
            .unwrap();
        let (_, exact) = storage
            .select_model_price_tier_v2("gpt-5.4", 272_000)
            .unwrap()
            .unwrap();
        let (_, high) = storage
            .select_model_price_tier_v2("gpt-5.4", 272_001)
            .unwrap()
            .unwrap();
        assert_eq!(low.min_input_tokens, 0);
        assert_eq!(exact.min_input_tokens, 0);
        assert_eq!(high.min_input_tokens, 272_000);
        let result = compute_charge_v2(10, 20, 0, &low, 1_000).unwrap();
        assert_eq!(result.uncached_input_tokens, 0);
        assert_eq!(
            result.numerator,
            10_i128 * i128::from(low.cached_input_microusd_per_1m)
        );
    }

    #[test]
    fn missing_price_is_rejected_and_snapshot_is_idempotent() {
        let storage = Storage::open_in_memory().unwrap();
        storage.init().unwrap();
        storage.conn.execute("INSERT INTO request_logs(request_path,method,created_at) VALUES('/v1/responses','POST',1)",[]).unwrap();
        let request_log_id = storage.conn.last_insert_rowid();
        let mut input = ChargeSnapshotInputV2 {
            request_log_id,
            model_slug: "codex-auto-review".into(),
            usage_source: "actual".into(),
            input_tokens: 1,
            cached_input_tokens: 0,
            output_tokens: 1,
            rate_multiplier_millis: 1_000,
            ..Default::default()
        };
        assert!(storage
            .record_charge_snapshot_v2(&input)
            .unwrap_err()
            .to_string()
            .contains("model_price_missing"));
        input.model_slug = "gpt-5.4-mini".into();
        input.input_tokens = 10;
        input.cached_input_tokens = 20;
        let first = storage.record_charge_snapshot_v2(&input).unwrap();
        let second = storage.record_charge_snapshot_v2(&input).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.cached_input_tokens, 10);
    }

    #[test]
    fn provider_base_override_is_multiplied_once_and_survives_log_clear() {
        let storage = Storage::open_in_memory().unwrap();
        storage.init().unwrap();
        let now = now_ts();
        let request_log_id = storage
            .insert_request_log(&super::super::RequestLog {
                request_path: "/v1/responses".into(),
                method: "POST".into(),
                actual_source_kind: Some("aggregate_api".into()),
                actual_source_id: Some("agg-provider".into()),
                created_at: now,
                ..Default::default()
            })
            .unwrap();
        storage
            .insert_request_token_stat(&super::super::RequestTokenStat {
                request_log_id,
                aggregate_api_id: Some("agg-provider".into()),
                model: Some("gpt-5.4-mini".into()),
                created_at: now,
                ..Default::default()
            })
            .unwrap();
        let snapshot = storage
            .record_charge_snapshot_v2(&ChargeSnapshotInputV2 {
                request_log_id,
                model_slug: "gpt-5.4-mini".into(),
                usage_source: "actual".into(),
                input_tokens: 10,
                output_tokens: 1,
                rate_multiplier_millis: 1_500,
                base_cost_override_microusd: Some(101),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(snapshot.base_cost_microusd, 101);
        assert_eq!(snapshot.charged_cost_microusd, 152);
        assert_eq!(
            storage
                .sum_aggregate_api_charged_spend_microusd_between("agg-provider", now - 1, now + 1,)
                .unwrap(),
            152
        );

        storage.clear_request_logs().unwrap();
        assert_eq!(
            storage
                .sum_aggregate_api_charged_spend_microusd_between("agg-provider", now - 1, now + 1,)
                .unwrap(),
            152
        );
    }

    #[test]
    fn long_context_billing_can_force_the_base_tier() {
        let storage = Storage::open_in_memory().unwrap();
        storage.init().unwrap();
        let (_, long) = storage
            .select_model_price_tier_with_long_context_billing_v2("gpt-5.4", 300_000, true)
            .unwrap()
            .unwrap();
        let (_, base) = storage
            .select_model_price_tier_with_long_context_billing_v2("gpt-5.4", 300_000, false)
            .unwrap()
            .unwrap();
        assert_eq!(long.min_input_tokens, 272_000);
        assert_eq!(base.min_input_tokens, 0);
    }

    #[test]
    fn charge_snapshot_records_when_long_context_billing_is_disabled() {
        let storage = Storage::open_in_memory().unwrap();
        storage.init().unwrap();
        storage
            .conn
            .execute(
                "INSERT INTO request_logs(request_path,method,created_at) VALUES('/v1/responses','POST',1)",
                [],
            )
            .unwrap();
        let request_log_id = storage.conn.last_insert_rowid();

        let snapshot = storage
            .record_charge_snapshot_v2(&ChargeSnapshotInputV2 {
                request_log_id,
                model_slug: "gpt-5.6-terra".into(),
                usage_source: "actual".into(),
                input_tokens: 300_000,
                output_tokens: 0,
                rate_multiplier_millis: 1_000,
                long_context_billing_enabled: Some(false),
                ..Default::default()
            })
            .unwrap();

        assert!(!snapshot.long_context_billing_enabled);
        assert_eq!(snapshot.tier_min_input_tokens, 0);
    }

    #[test]
    fn provider_base_override_can_charge_unknown_model() {
        let storage = Storage::open_in_memory().unwrap();
        storage.init().unwrap();
        storage
            .conn
            .execute(
                "INSERT INTO request_logs(request_path,method,created_at) VALUES('/v1/responses','POST',1)",
                [],
            )
            .unwrap();
        let request_log_id = storage.conn.last_insert_rowid();
        storage
            .insert_request_token_stat(&super::super::RequestTokenStat {
                request_log_id,
                model: Some("provider-only-model".into()),
                created_at: 1,
                ..Default::default()
            })
            .unwrap();
        let snapshot = storage
            .record_charge_snapshot_v2(&ChargeSnapshotInputV2 {
                request_log_id,
                model_slug: "provider-only-model".into(),
                usage_source: "actual".into(),
                input_tokens: 10,
                output_tokens: 2,
                rate_multiplier_millis: 1_250,
                base_cost_override_microusd: Some(800),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(snapshot.model_id, None);
        assert_eq!(snapshot.base_cost_microusd, 800);
        assert_eq!(snapshot.charged_cost_microusd, 1_000);
        let cost: f64 = storage
            .conn
            .query_row(
                "SELECT estimated_cost_usd FROM request_token_stats WHERE request_log_id=?1",
                [request_log_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!((cost - 0.001).abs() < f64::EPSILON);
    }

    #[test]
    fn zero_multiplier_records_a_free_ledger_and_is_idempotent() {
        let storage = Storage::open_in_memory().unwrap();
        storage.init().unwrap();
        storage
            .conn
            .execute(
                "INSERT INTO app_wallets(id,owner_kind,owner_id,balance_credit_micros,
                   frozen_credit_micros,status,created_at,updated_at)
                 VALUES('wallet-free','user','free-user',0,0,'active',1,1)",
                [],
            )
            .unwrap();
        storage.conn.execute("INSERT INTO request_logs(request_path,method,created_at) VALUES('/v1/responses','POST',1)",[]).unwrap();
        let request_log_id = storage.conn.last_insert_rowid();
        let input = ChargeSnapshotInputV2 {
            request_log_id,
            model_slug: "gpt-5.4-mini".into(),
            usage_source: "estimated".into(),
            input_tokens: 100,
            cached_input_tokens: 0,
            output_tokens: 0,
            rate_multiplier_millis: 0,
            wallet_id: Some("wallet-free".into()),
            ..Default::default()
        };
        let first = storage.record_charge_snapshot_v2(&input).unwrap();
        let second = storage.record_charge_snapshot_v2(&input).unwrap();
        assert_eq!(first, second);
        assert!(first.base_cost_microusd > 0);
        assert_eq!(first.charged_cost_microusd, 0);
        assert_eq!(storage.request_charge_ledger_entry_count().unwrap(), 1);
        let duplicate = storage.conn.execute(
            "INSERT INTO app_wallet_ledger_entries(id,wallet_id,entry_kind,
               amount_credit_micros,balance_after_credit_micros,request_log_id,created_at)
             VALUES('duplicate-charge','wallet-free','request_charge',0,0,?1,2)",
            [request_log_id],
        );
        assert!(
            duplicate.is_err(),
            "partial unique index must reject duplicates"
        );
        let balance: i64 = storage
            .conn
            .query_row(
                "SELECT balance_credit_micros FROM app_wallets WHERE id='wallet-free'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(balance, 0);
    }

    #[test]
    fn insufficient_wallet_balance_rolls_back_snapshot_and_ledger() {
        let storage = Storage::open_in_memory().unwrap();
        storage.init().unwrap();
        storage
            .conn
            .execute(
                "INSERT INTO app_wallets(id,owner_kind,owner_id,balance_credit_micros,
                   frozen_credit_micros,status,created_at,updated_at)
                 VALUES('wallet-low','user','low-user',1,0,'active',1,1)",
                [],
            )
            .unwrap();
        storage.conn.execute("INSERT INTO request_logs(request_path,method,created_at) VALUES('/v1/responses','POST',1)",[]).unwrap();
        let request_log_id = storage.conn.last_insert_rowid();
        let error = storage
            .record_charge_snapshot_v2(&ChargeSnapshotInputV2 {
                request_log_id,
                model_slug: "gpt-5.4-mini".into(),
                usage_source: "actual".into(),
                input_tokens: 1_000,
                cached_input_tokens: 0,
                output_tokens: 1_000,
                rate_multiplier_millis: 1_000,
                wallet_id: Some("wallet-low".into()),
                ..Default::default()
            })
            .expect_err("insufficient wallet must fail");
        assert!(error.to_string().contains("wallet_insufficient_balance"));
        assert!(storage
            .get_charge_snapshot_v2(request_log_id)
            .unwrap()
            .is_none());
        assert_eq!(storage.request_charge_ledger_entry_count().unwrap(), 0);
        let balance: i64 = storage
            .conn
            .query_row(
                "SELECT balance_credit_micros FROM app_wallets WHERE id='wallet-low'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(balance, 1);
    }

    #[test]
    fn charge_snapshot_retries_after_transient_sqlite_write_lock() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "codexmanager-charge-snapshot-lock-{}-{nonce}.db",
            std::process::id()
        ));
        let storage = Storage::open(&path).unwrap();
        storage.init().unwrap();
        let request_log_id = storage
            .insert_request_log(&super::super::RequestLog {
                request_path: "/v1/responses".into(),
                method: "POST".into(),
                created_at: 1,
                ..Default::default()
            })
            .unwrap();
        storage
            .insert_request_token_stat(&super::super::RequestTokenStat {
                request_log_id,
                model: Some("gpt-5.4-mini".into()),
                created_at: 1,
                ..Default::default()
            })
            .unwrap();

        let retrying_storage = Storage::open(&path).unwrap();
        retrying_storage
            .conn
            .busy_timeout(Duration::from_millis(1))
            .unwrap();
        let blocking_storage = Storage::open(&path).unwrap();
        let (locked_tx, locked_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let blocker = std::thread::spawn(move || {
            let transaction = blocking_storage.conn.unchecked_transaction().unwrap();
            locked_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            transaction.commit().unwrap();
        });
        locked_rx.recv().unwrap();
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(10));
            release_tx.send(()).unwrap();
        });

        let snapshot = retrying_storage
            .record_charge_snapshot_v2(&ChargeSnapshotInputV2 {
                request_log_id,
                model_slug: "gpt-5.4-mini".into(),
                usage_source: "actual".into(),
                input_tokens: 100,
                output_tokens: 10,
                rate_multiplier_millis: 1_000,
                ..Default::default()
            })
            .expect("a transient write lock should be retried");

        releaser.join().unwrap();
        blocker.join().unwrap();
        assert!(snapshot.charged_cost_microusd > 0);
        assert!(retrying_storage
            .get_charge_snapshot_v2(request_log_id)
            .unwrap()
            .is_some());

        drop(retrying_storage);
        drop(storage);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }
}
