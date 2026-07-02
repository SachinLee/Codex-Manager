use codexmanager_core::rpc::types::{
    AggregateApiReasoningGuardStatSummary, AggregateApiReasoningGuardStatsResult,
    DailyUsageStatsParams,
};

use crate::storage_helpers::open_storage;

use super::day_range::resolve_day_bounds_ts;

pub(crate) fn read_aggregate_api_reasoning_guard_stats(
    params: DailyUsageStatsParams,
) -> Result<AggregateApiReasoningGuardStatsResult, String> {
    let storage = open_storage().ok_or_else(|| "open storage failed".to_string())?;
    let (start_ts, end_ts) = resolve_day_bounds_ts(params.day_start_ts, params.day_end_ts)?;
    let items = storage
        .summarize_reasoning_guard_by_aggregate_api_between(start_ts, end_ts)
        .map_err(|err| format!("summarize aggregate api reasoning guard failed: {err}"))?
        .into_iter()
        .map(|item| {
            let total_request_count = item.total_request_count.max(0);
            let internal_retry_request_count = item.internal_retry_request_count.max(0);
            let retry_recovery_count = item
                .recovered_count
                .max(0)
                .min(internal_retry_request_count);
            AggregateApiReasoningGuardStatSummary {
                aggregate_api_id: item.aggregate_api_id,
                aggregate_api_supplier_name: item.aggregate_api_supplier_name,
                aggregate_api_url: item.aggregate_api_url,
                total_request_count,
                event_count: item.event_count.max(0),
                affected_request_count: item.affected_request_count.max(0),
                match_rate: ratio(item.affected_request_count, total_request_count),
                internal_retry_count: item.internal_retry_count.max(0),
                internal_retry_request_count,
                retry_recovery_count,
                retry_recovery_rate: ratio(retry_recovery_count, internal_retry_request_count),
                block_count: item.block_count.max(0),
                blocked_request_count: item.blocked_request_count.max(0),
                block_rate: ratio(item.blocked_request_count, total_request_count),
                observe_only_count: item.observe_only_count.max(0),
                bypass_after_consecutive_count: item.bypass_after_consecutive_count.max(0),
                guard_input_tokens: item.guard_input_tokens.max(0),
                guard_cached_input_tokens: item.guard_cached_input_tokens.max(0),
                guard_output_tokens: item.guard_output_tokens.max(0),
                guard_total_tokens: item.guard_total_tokens.max(0),
                guard_reasoning_output_tokens: item.guard_reasoning_output_tokens.max(0),
                guard_estimated_cost_usd: item.guard_estimated_cost_usd.max(0.0),
                last_target_token: item.last_target_token,
                last_event_at: item.last_event_at,
            }
        })
        .collect();
    Ok(AggregateApiReasoningGuardStatsResult { items })
}

fn ratio(numerator: i64, denominator: i64) -> f64 {
    if denominator <= 0 {
        return 0.0;
    }
    ((numerator.max(0) as f64) / (denominator as f64)).clamp(0.0, 1.0)
}
