use codexmanager_core::rpc::types::{
    AggregateApiDailyUsageStatSummary, AggregateApiDailyUsageStatsResult, DailyUsageStatsParams,
};

use crate::storage_helpers::open_storage;

use super::day_range::resolve_day_bounds_ts;

pub(crate) fn read_aggregate_api_daily_usage_stats(
    params: DailyUsageStatsParams,
) -> Result<AggregateApiDailyUsageStatsResult, String> {
    let storage = open_storage().ok_or_else(|| "open storage failed".to_string())?;
    let (start_ts, end_ts) = resolve_day_bounds_ts(params.day_start_ts, params.day_end_ts)?;
    let items = storage
        .summarize_request_token_stats_by_aggregate_api_between(start_ts, end_ts)
        .map_err(|err| format!("summarize aggregate api daily usage failed: {err}"))?
        .into_iter()
        .map(|item| AggregateApiDailyUsageStatSummary {
            aggregate_api_id: item.aggregate_api_id,
            aggregate_api_supplier_name: item.aggregate_api_supplier_name,
            aggregate_api_url: item.aggregate_api_url,
            request_count: item.request_count.max(0),
            input_tokens: item.input_tokens.max(0),
            cached_input_tokens: item.cached_input_tokens.max(0),
            cache_write_input_tokens: item.cache_write_input_tokens.max(0),
            billable_input_tokens: item.billable_input_tokens.max(0),
            output_tokens: item.output_tokens.max(0),
            total_tokens: item.total_tokens.max(0),
            reasoning_output_tokens: item.reasoning_output_tokens.max(0),
            estimated_cost_usd: item.estimated_cost_usd.max(0.0),
            guard_retry_total_tokens: item.guard_retry_total_tokens.max(0),
            guard_retry_estimated_cost_usd: item.guard_retry_estimated_cost_usd.max(0.0),
            billable_total_tokens: item.billable_total_tokens.max(0),
            billable_estimated_cost_usd: item.billable_estimated_cost_usd.max(0.0),
            cache_hit_rate: item.cache_hit_rate.clamp(0.0, 1.0),
            budget_spent_usd: item.budget_spent_usd.map(|value| value.max(0.0)),
            budget_reserved_usd: item.budget_reserved_usd.map(|value| value.max(0.0)),
            budget_held_usd: item.budget_held_usd.map(|value| value.max(0.0)),
            budget_remaining_usd: item.budget_remaining_usd.map(|value| value.max(0.0)),
            budget_over_limit: item.budget_over_limit,
        })
        .collect();
    Ok(AggregateApiDailyUsageStatsResult { items })
}
