from pathlib import Path
p = Path('crates/core/src/storage/request_token_stats.rs')
t = p.read_text(encoding='utf-8')

replacements = [
(
'''fn token_usage_rollup_from_row(row: &Row<'_>, offset: usize) -> Result<TokenUsageRollup> {
    Ok(TokenUsageRollup {
        input_tokens: row.get::<_, i64>(offset)?.max(0),
        cached_input_tokens: row.get::<_, i64>(offset + 1)?.max(0),
        output_tokens: row.get::<_, i64>(offset + 2)?.max(0),
        reasoning_output_tokens: row.get::<_, i64>(offset + 3)?.max(0),
        total_tokens: row.get::<_, i64>(offset + 4)?.max(0),
        estimated_cost_usd: row.get::<_, f64>(offset + 5)?.max(0.0),
        request_count: row.get::<_, i64>(offset + 6)?.max(0),
        success_count: row.get::<_, i64>(offset + 7)?.max(0),
        error_count: row.get::<_, i64>(offset + 8)?.max(0),
    })
}''',
'''fn token_usage_rollup_from_row(row: &Row<'_>, offset: usize) -> Result<TokenUsageRollup> {
    Ok(TokenUsageRollup {
        input_tokens: row.get::<_, i64>(offset)?.max(0),
        cached_input_tokens: row.get::<_, i64>(offset + 1)?.max(0),
        cache_write_input_tokens: 0,
        output_tokens: row.get::<_, i64>(offset + 2)?.max(0),
        reasoning_output_tokens: row.get::<_, i64>(offset + 3)?.max(0),
        total_tokens: row.get::<_, i64>(offset + 4)?.max(0),
        estimated_cost_usd: row.get::<_, f64>(offset + 5)?.max(0.0),
        request_count: row.get::<_, i64>(offset + 6)?.max(0),
        success_count: row.get::<_, i64>(offset + 7)?.max(0),
        error_count: row.get::<_, i64>(offset + 8)?.max(0),
    })
}'''
),
(
'''fn request_log_query_summary_from_usage(usage: TokenUsageRollup) -> RequestLogQuerySummary {
    RequestLogQuerySummary {
        count: usage.request_count,
        success_count: usage.success_count,
        error_count: usage.error_count,
        total_tokens: usage.total_tokens,
        estimated_cost_usd: usage.estimated_cost_usd,
    }
}''',
'''fn request_log_query_summary_from_usage(usage: TokenUsageRollup) -> RequestLogQuerySummary {
    RequestLogQuerySummary {
        count: usage.request_count,
        success_count: usage.success_count,
        error_count: usage.error_count,
        total_tokens: usage.total_tokens,
        estimated_cost_usd: usage.estimated_cost_usd,
        guard_retry_total_tokens: 0,
        guard_retry_estimated_cost_usd: 0.0,
        long_context_count: 0,
        long_context_cost_usd: 0.0,
        long_context_uplift_usd: 0.0,
        legacy_candidate_count: 0,
    }
}'''
),
(
'''fn empty_request_log_today_summary() -> RequestLogTodaySummary {
    RequestLogTodaySummary {
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
        reasoning_output_tokens: 0,
        estimated_cost_usd: 0.0,
    }
}''',
'''fn empty_request_log_today_summary() -> RequestLogTodaySummary {
    RequestLogTodaySummary {
        input_tokens: 0,
        cached_input_tokens: 0,
        cache_write_input_tokens: 0,
        output_tokens: 0,
        reasoning_output_tokens: 0,
        estimated_cost_usd: 0.0,
    }
}'''
),
(
'''fn request_log_today_summary_from_row(row: &Row<'_>) -> Result<RequestLogTodaySummary> {
    Ok(RequestLogTodaySummary {
        input_tokens: row.get(0)?,
        cached_input_tokens: row.get(1)?,
        output_tokens: row.get(2)?,
        reasoning_output_tokens: row.get(3)?,
        estimated_cost_usd: row.get(4)?,
    })
}''',
'''fn request_log_today_summary_from_row(row: &Row<'_>) -> Result<RequestLogTodaySummary> {
    Ok(RequestLogTodaySummary {
        input_tokens: row.get(0)?,
        cached_input_tokens: row.get(1)?,
        cache_write_input_tokens: 0,
        output_tokens: row.get(2)?,
        reasoning_output_tokens: row.get(3)?,
        estimated_cost_usd: row.get(4)?,
    })
}'''
),
(
'''fn map_token_usage_summary(row: &Row<'_>) -> Result<TokenUsageSummary> {
    Ok(TokenUsageSummary {
        model: row.get(0)?,
        input_tokens: row.get::<_, i64>(1)?.max(0),
        cached_input_tokens: row.get::<_, i64>(2)?.max(0),
        output_tokens: row.get::<_, i64>(3)?.max(0),
        reasoning_output_tokens: row.get::<_, i64>(4)?.max(0),
        total_tokens: row.get::<_, i64>(5)?.max(0),
        estimated_cost_usd: row.get::<_, f64>(6)?.max(0.0),
    })
}''',
'''fn map_token_usage_summary(row: &Row<'_>) -> Result<TokenUsageSummary> {
    Ok(TokenUsageSummary {
        model: row.get(0)?,
        input_tokens: row.get::<_, i64>(1)?.max(0),
        cached_input_tokens: row.get::<_, i64>(2)?.max(0),
        cache_write_input_tokens: 0,
        output_tokens: row.get::<_, i64>(3)?.max(0),
        reasoning_output_tokens: row.get::<_, i64>(4)?.max(0),
        total_tokens: row.get::<_, i64>(5)?.max(0),
        estimated_cost_usd: row.get::<_, f64>(6)?.max(0.0),
    })
}'''
),
(
'''fn map_api_key_model_token_usage_summary(row: &Row<'_>) -> Result<ApiKeyModelTokenUsageSummary> {
    Ok(ApiKeyModelTokenUsageSummary {
        key_id: row.get(0)?,
        model: row.get(1)?,
        input_tokens: row.get::<_, i64>(2)?.max(0),
        cached_input_tokens: row.get::<_, i64>(3)?.max(0),
        output_tokens: row.get::<_, i64>(4)?.max(0),
        reasoning_output_tokens: row.get::<_, i64>(5)?.max(0),
        total_tokens: row.get::<_, i64>(6)?.max(0),
        estimated_cost_usd: row.get::<_, f64>(7)?.max(0.0),
    })
}''',
'''fn map_api_key_model_token_usage_summary(row: &Row<'_>) -> Result<ApiKeyModelTokenUsageSummary> {
    Ok(ApiKeyModelTokenUsageSummary {
        key_id: row.get(0)?,
        model: row.get(1)?,
        input_tokens: row.get::<_, i64>(2)?.max(0),
        cached_input_tokens: row.get::<_, i64>(3)?.max(0),
        cache_write_input_tokens: 0,
        output_tokens: row.get::<_, i64>(4)?.max(0),
        reasoning_output_tokens: row.get::<_, i64>(5)?.max(0),
        total_tokens: row.get::<_, i64>(6)?.max(0),
        estimated_cost_usd: row.get::<_, f64>(7)?.max(0.0),
    })
}'''
),
]
for i,(old,new) in enumerate(replacements):
    if old not in t:
        print('missing pattern', i)
    else:
        t = t.replace(old,new,1)
        print('fixed', i)
p.write_text(t, encoding='utf-8')
print('done')
