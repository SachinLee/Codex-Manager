use super::{write_request_log, RequestLogTraceContext, RequestLogUsage};
use codexmanager_core::storage::Storage;

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-12,
        "actual={actual}, expected={expected}"
    );
}

fn sample_usage(cache_write_input_tokens: Option<i64>) -> RequestLogUsage {
    RequestLogUsage {
        input_tokens: Some(1_000),
        cached_input_tokens: Some(200),
        cache_write_input_tokens,
        output_tokens: Some(500),
        total_tokens: Some(1_500),
        reasoning_output_tokens: None,
        first_response_ms: Some(120),
    }
}

#[test]
fn write_request_log_persists_cost_with_aggregate_api_multiplier() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let attempted_aggregate_api_ids = vec!["ag_cost_multiplier".to_string()];

    write_request_log(
        &storage,
        RequestLogTraceContext {
            aggregate_api_supplier_name: Some("upstream"),
            aggregate_api_url: Some("https://upstream.example/v1"),
            attempted_aggregate_api_ids: Some(attempted_aggregate_api_ids.as_slice()),
            cost_multiplier: Some(2.5),
            ..Default::default()
        },
        Some("gk_cost_multiplier"),
        None,
        "/v1/chat/completions",
        "POST",
        Some("gpt-5.4-mini"),
        None,
        Some("https://upstream.example/v1/chat/completions"),
        Some(200),
        sample_usage(None),
        None,
        Some(250),
    );

    let logs = storage
        .list_request_logs_paginated(None, None, None, None, 0, 10)
        .expect("list request logs");
    assert_eq!(logs.len(), 1);
    assert_eq!(
        logs[0].initial_aggregate_api_id.as_deref(),
        Some("ag_cost_multiplier")
    );
    assert_close(
        logs[0].estimated_cost_usd.expect("estimated cost"),
        0.0071625,
    );
}

#[test]
fn write_request_log_uses_priority_price_for_fast_service_tier() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    write_request_log(
        &storage,
        RequestLogTraceContext {
            service_tier: Some("fast"),
            ..Default::default()
        },
        Some("gk_fast_price"),
        None,
        "/v1/chat/completions",
        "POST",
        Some("gpt-5.4-mini"),
        None,
        Some("https://api.openai.com/v1/chat/completions"),
        Some(200),
        sample_usage(None),
        None,
        Some(250),
    );

    let logs = storage
        .list_request_logs_paginated(None, None, None, None, 0, 10)
        .expect("list request logs");
    assert_eq!(logs.len(), 1);
    assert_close(logs[0].estimated_cost_usd.expect("estimated cost"), 0.00573);
}

#[test]
fn write_request_log_uses_effective_priority_price_with_multiplier() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    write_request_log(
        &storage,
        RequestLogTraceContext {
            service_tier: Some("standard"),
            effective_service_tier: Some("priority"),
            cost_multiplier: Some(2.5),
            ..Default::default()
        },
        Some("gk_effective_priority_price"),
        None,
        "/v1/chat/completions",
        "POST",
        Some("gpt-5.4-mini"),
        None,
        Some("https://api.openai.com/v1/chat/completions"),
        Some(200),
        sample_usage(None),
        None,
        Some(250),
    );

    let logs = storage
        .list_request_logs_paginated(None, None, None, None, 0, 10)
        .expect("list request logs");
    assert_eq!(logs.len(), 1);
    assert_close(
        logs[0].estimated_cost_usd.expect("estimated cost"),
        0.014325,
    );
}

#[test]
fn write_request_log_prices_cache_write_tokens_with_the_effective_priority_tier() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    write_request_log(
        &storage,
        RequestLogTraceContext {
            effective_service_tier: Some("priority"),
            ..Default::default()
        },
        Some("gk_priority_cache_write"),
        None,
        "/v1/responses",
        "POST",
        Some("gpt-5.6-sol"),
        None,
        Some("https://api.openai.com/v1/responses"),
        Some(200),
        sample_usage(Some(100)),
        None,
        Some(250),
    );

    let logs = storage
        .list_request_logs_paginated(None, None, None, None, 0, 10)
        .expect("list request logs");
    assert_eq!(logs.len(), 1);
    // plain=700*10/M, cache-read=200*1/M, cache-write=100*12.5/M, output=500*60/M.
    assert_close(logs[0].estimated_cost_usd.expect("estimated cost"), 0.03845);

    let usage = storage
        .summarize_request_token_stats_total()
        .expect("summarize request token stats");
    assert_eq!(usage.cache_write_input_tokens, 100);
}

#[test]
fn write_request_log_persists_long_context_pricing_snapshot() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    write_request_log(
        &storage,
        RequestLogTraceContext {
            trace_id: Some("trc_long_context_snapshot"),
            ..Default::default()
        },
        Some("gk_long_context"),
        None,
        "/v1/responses",
        "POST",
        Some("gpt-5.6-sol"),
        None,
        Some("https://api.openai.com/v1/responses"),
        Some(200),
        RequestLogUsage {
            input_tokens: Some(300_000),
            cached_input_tokens: Some(200_000),
            cache_write_input_tokens: Some(10_000),
            output_tokens: Some(1_000),
            total_tokens: Some(301_000),
            reasoning_output_tokens: None,
            first_response_ms: None,
        },
        None,
        Some(250),
    );

    let snapshots = storage
        .list_request_pricing_snapshots_for_trace_ids(&["trc_long_context_snapshot".to_string()])
        .expect("list snapshots");
    assert_eq!(snapshots.len(), 1);
    let (_, snapshot) = &snapshots[0];
    assert_eq!(snapshot.context_band, "long");
    assert_eq!(snapshot.billing_mode, "standard");
    assert_eq!(snapshot.long_context_threshold_tokens, Some(272_000));
    assert_eq!(snapshot.matched_pattern.as_deref(), Some("gpt-5.6-sol"));
    assert_close(snapshot.total_cost_usd.expect("total"), 1.27);
    assert_close(snapshot.long_context_uplift_usd.expect("uplift"), 0.6275);
}
