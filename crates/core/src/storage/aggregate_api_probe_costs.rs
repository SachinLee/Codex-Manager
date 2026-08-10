use rusqlite::{params, Result};

use super::{AggregateApiProbeCost, AggregateApiProbeCostSummary, Storage};

impl Storage {
    pub fn insert_aggregate_api_probe_cost(&self, cost: &AggregateApiProbeCost) -> Result<()> {
        self.conn.execute(
            "INSERT INTO aggregate_api_probe_costs (
                aggregate_api_id, upstream_model, trigger, outcome,
                estimated_input_tokens, estimated_output_tokens, pricing_model, price_source,
                input_microusd_per_1m, output_microusd_per_1m, rate_multiplier_millis,
                estimated_cost_microusd, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                cost.aggregate_api_id,
                cost.upstream_model,
                cost.trigger,
                cost.outcome,
                cost.estimated_input_tokens,
                cost.estimated_output_tokens,
                cost.pricing_model,
                cost.price_source,
                cost.input_microusd_per_1m,
                cost.output_microusd_per_1m,
                cost.rate_multiplier_millis,
                cost.estimated_cost_microusd,
                cost.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn summarize_aggregate_api_probe_costs_between(
        &self,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<AggregateApiProbeCostSummary>> {
        if end_ts <= start_ts {
            return Ok(Vec::new());
        }
        let mut statement = self.conn.prepare(
            "SELECT aggregate_api_id,
                COUNT(*) AS probe_count,
                SUM(CASE WHEN estimated_cost_microusd IS NOT NULL THEN 1 ELSE 0 END),
                SUM(CASE WHEN estimated_cost_microusd IS NULL THEN 1 ELSE 0 END),
                SUM(CASE WHEN trigger='scheduled_probe' THEN 1 ELSE 0 END),
                SUM(CASE WHEN trigger='half_open' THEN 1 ELSE 0 END),
                SUM(CASE WHEN trigger='manual_probe' THEN 1 ELSE 0 END),
                COALESCE(SUM(estimated_cost_microusd), 0)
             FROM aggregate_api_probe_costs
             WHERE created_at >= ?1 AND created_at < ?2
             GROUP BY aggregate_api_id
             ORDER BY estimated_cost_microusd DESC, aggregate_api_id ASC",
        )?;
        statement
            .query_map(params![start_ts, end_ts], |row| {
                Ok(AggregateApiProbeCostSummary {
                    aggregate_api_id: row.get(0)?,
                    probe_count: row.get(1)?,
                    priced_probe_count: row.get(2)?,
                    unknown_cost_probe_count: row.get(3)?,
                    scheduled_probe_count: row.get(4)?,
                    half_open_probe_count: row.get(5)?,
                    manual_probe_count: row.get(6)?,
                    estimated_cost_microusd: row.get(7)?,
                })
            })?
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{now_ts, AggregateApi};

    fn sample_aggregate_api(id: &str, now: i64) -> AggregateApi {
        AggregateApi {
            id: id.to_string(),
            provider_type: "openai-compatible".to_string(),
            supplier_name: Some(id.to_string()),
            sort: 0,
            url: format!("https://{id}.example.test"),
            auth_type: "bearer".to_string(),
            auth_params_json: None,
            action: None,
            model_override: None,
            cost_multiplier: 1.0,
            daily_spend_limit_usd: None,
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
        }
    }

    fn probe_cost(
        api_id: &str,
        trigger: &str,
        estimated_cost_microusd: Option<i64>,
        created_at: i64,
    ) -> AggregateApiProbeCost {
        AggregateApiProbeCost {
            aggregate_api_id: api_id.to_string(),
            upstream_model: "gpt-5.6-sol".to_string(),
            trigger: trigger.to_string(),
            outcome: "success".to_string(),
            estimated_input_tokens: 100,
            estimated_output_tokens: 16,
            pricing_model: estimated_cost_microusd.map(|_| "gpt-5.6-sol".to_string()),
            price_source: estimated_cost_microusd.map(|_| "official".to_string()),
            input_microusd_per_1m: estimated_cost_microusd.map(|_| 1_000_000),
            output_microusd_per_1m: estimated_cost_microusd.map(|_| 10_000_000),
            rate_multiplier_millis: Some(1_000),
            estimated_cost_microusd,
            created_at,
        }
    }

    #[test]
    fn summarize_probe_costs_keeps_unknown_amounts_and_trigger_breakdown() {
        let storage = Storage::open_in_memory().expect("open storage");
        storage.init().expect("initialize storage");
        let now = now_ts();
        storage
            .insert_aggregate_api(&sample_aggregate_api("api-a", now))
            .expect("insert aggregate api");

        storage
            .insert_aggregate_api_probe_cost(&probe_cost("api-a", "scheduled_probe", Some(12), now))
            .expect("insert priced cost");
        storage
            .insert_aggregate_api_probe_cost(&probe_cost("api-a", "half_open", None, now + 1))
            .expect("insert unknown cost");
        storage
            .insert_aggregate_api_probe_cost(&probe_cost("api-a", "manual_probe", Some(8), now + 2))
            .expect("insert manual cost");

        let summaries = storage
            .summarize_aggregate_api_probe_costs_between(now, now + 3)
            .expect("summarize costs");

        assert_eq!(summaries.len(), 1);
        let summary = &summaries[0];
        assert_eq!(summary.aggregate_api_id, "api-a");
        assert_eq!(summary.probe_count, 3);
        assert_eq!(summary.priced_probe_count, 2);
        assert_eq!(summary.unknown_cost_probe_count, 1);
        assert_eq!(summary.scheduled_probe_count, 1);
        assert_eq!(summary.half_open_probe_count, 1);
        assert_eq!(summary.manual_probe_count, 1);
        assert_eq!(summary.estimated_cost_microusd, 20);
    }
}
