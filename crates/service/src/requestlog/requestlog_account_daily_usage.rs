use codexmanager_core::rpc::types::{
    AccountDailyUsageStatSummary, AccountDailyUsageStatsResult, DailyUsageStatsParams,
};

use crate::storage_helpers::open_storage;

use super::day_range::resolve_day_bounds_ts;

pub(crate) fn read_account_daily_usage_stats(
    params: DailyUsageStatsParams,
) -> Result<AccountDailyUsageStatsResult, String> {
    let storage = open_storage().ok_or_else(|| "open storage failed".to_string())?;
    let (start_ts, end_ts) = resolve_day_bounds_ts(params.day_start_ts, params.day_end_ts)?;
    let items = storage
        .summarize_request_token_stats_by_account_between(start_ts, end_ts)
        .map_err(|err| format!("summarize account daily usage failed: {err}"))?
        .into_iter()
        .map(|item| AccountDailyUsageStatSummary {
            account_id: item.account_id,
            request_count: item.request_count.max(0),
            input_tokens: item.input_tokens.max(0),
            cached_input_tokens: item.cached_input_tokens.max(0),
            cache_write_input_tokens: item.cache_write_input_tokens.max(0),
            billable_input_tokens: item.billable_input_tokens.max(0),
            output_tokens: item.output_tokens.max(0),
            total_tokens: item.total_tokens.max(0),
            reasoning_output_tokens: item.reasoning_output_tokens.max(0),
            estimated_cost_usd: item.estimated_cost_usd.max(0.0),
            cache_hit_rate: item.cache_hit_rate.clamp(0.0, 1.0),
        })
        .collect();
    Ok(AccountDailyUsageStatsResult { items })
}
