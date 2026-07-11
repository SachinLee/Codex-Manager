use codexmanager_core::rpc::types::RequestLogTodaySummaryResult;
use codexmanager_core::storage::Storage;

use super::day_range::resolve_day_bounds_ts;
use crate::storage_helpers::open_storage;

/// 函数 `read_requestlog_today_summary`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - day_start_ts: 参数 day_start_ts
/// - day_end_ts: 参数 day_end_ts
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn read_requestlog_today_summary(
    day_start_ts: Option<i64>,
    day_end_ts: Option<i64>,
) -> Result<RequestLogTodaySummaryResult, String> {
    let storage = open_storage().ok_or_else(|| "open storage failed".to_string())?;
    read_requestlog_today_summary_with_storage(&storage, day_start_ts, day_end_ts)
}

pub(crate) fn read_requestlog_today_summary_with_storage(
    storage: &Storage,
    day_start_ts: Option<i64>,
    day_end_ts: Option<i64>,
) -> Result<RequestLogTodaySummaryResult, String> {
    let (start_ts, end_ts) = resolve_day_bounds_ts(day_start_ts, day_end_ts)?;
    let summary = storage
        .summarize_request_logs_between(start_ts, end_ts)
        .map_err(|err| format!("summarize request logs failed: {err}"))?;
    Ok(map_today_summary(summary))
}

pub(crate) fn read_requestlog_today_summary_for_key_ids(
    day_start_ts: Option<i64>,
    day_end_ts: Option<i64>,
    key_ids: &[String],
) -> Result<RequestLogTodaySummaryResult, String> {
    let storage = open_storage().ok_or_else(|| "open storage failed".to_string())?;
    read_requestlog_today_summary_for_key_ids_with_storage(
        &storage,
        day_start_ts,
        day_end_ts,
        key_ids,
    )
}

pub(crate) fn read_requestlog_today_summary_for_key_ids_with_storage(
    storage: &Storage,
    day_start_ts: Option<i64>,
    day_end_ts: Option<i64>,
    key_ids: &[String],
) -> Result<RequestLogTodaySummaryResult, String> {
    let (start_ts, end_ts) = resolve_day_bounds_ts(day_start_ts, day_end_ts)?;
    let summary = storage
        .summarize_request_logs_between_for_keys(start_ts, end_ts, key_ids)
        .map_err(|err| format!("summarize request logs failed: {err}"))?;
    Ok(map_today_summary(summary))
}

fn map_today_summary(
    summary: codexmanager_core::storage::RequestLogTodaySummary,
) -> RequestLogTodaySummaryResult {
    let input_tokens = summary.input_tokens.max(0);
    let cached_input_tokens = summary.cached_input_tokens.max(0);
    let cache_write_input_tokens = summary.cache_write_input_tokens.max(0);
    let output_tokens = summary.output_tokens.max(0);
    let reasoning_output_tokens = summary.reasoning_output_tokens.max(0);
    let non_cached_input_tokens = input_tokens
        .saturating_sub(cached_input_tokens)
        .saturating_sub(cache_write_input_tokens);
    RequestLogTodaySummaryResult {
        input_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        output_tokens,
        reasoning_output_tokens,
        today_tokens: non_cached_input_tokens.saturating_add(output_tokens),
        estimated_cost: summary.estimated_cost_usd.max(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::super::day_range::MAX_REQUESTED_DAY_RANGE_SECS;
    use super::resolve_day_bounds_ts;

    #[test]
    fn resolve_day_bounds_uses_requested_range_when_complete() {
        assert_eq!(
            resolve_day_bounds_ts(Some(1_700_000_000), Some(1_700_086_400)).unwrap(),
            (1_700_000_000, 1_700_086_400)
        );
    }

    #[test]
    fn resolve_day_bounds_rejects_partial_range() {
        let error = resolve_day_bounds_ts(Some(1_700_000_000), None).unwrap_err();
        assert!(error.contains("provided together"));
    }

    #[test]
    fn resolve_day_bounds_rejects_oversized_range() {
        let error =
            resolve_day_bounds_ts(Some(0), Some(MAX_REQUESTED_DAY_RANGE_SECS + 1)).unwrap_err();
        assert!(error.contains("too large"));
    }
}
