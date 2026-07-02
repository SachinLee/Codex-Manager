use super::{RequestLog, RequestTokenStat, Storage};
use crate::storage::GatewayReasoningGuardEvent;

/// 函数 `collect_query_plan_details`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - storage: 参数 storage
/// - sql: 参数 sql
///
/// # 返回
/// 返回函数执行结果
fn collect_query_plan_details(storage: &Storage, sql: &str) -> Vec<String> {
    let mut stmt = storage.conn.prepare(sql).expect("prepare explain");
    let mut rows = stmt.query([]).expect("query explain");
    let mut details = Vec::new();
    while let Some(row) = rows.next().expect("next explain row") {
        let detail: String = row.get(3).expect("detail");
        details.push(detail.to_ascii_lowercase());
    }
    details
}

/// 函数 `method_exact_query_matches_composite_index`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn method_exact_query_matches_composite_index() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let details = collect_query_plan_details(
        &storage,
        "EXPLAIN QUERY PLAN
         SELECT key_id, account_id, request_path, method, model, reasoning_effort, upstream_url, status_code, error, created_at
         FROM request_logs
         WHERE method = 'POST'
         ORDER BY created_at DESC, id DESC
         LIMIT 100",
    );
    assert!(details
        .iter()
        .any(|detail| detail.contains("idx_request_logs_method_created_at")));
}

/// 函数 `key_exact_query_matches_composite_index`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn key_exact_query_matches_composite_index() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let details = collect_query_plan_details(
        &storage,
        "EXPLAIN QUERY PLAN
         SELECT key_id, account_id, request_path, method, model, reasoning_effort, upstream_url, status_code, error, created_at
         FROM request_logs
         WHERE key_id = 'gk_1'
         ORDER BY created_at DESC, id DESC
         LIMIT 100",
    );
    assert!(details
        .iter()
        .any(|detail| detail.contains("idx_request_logs_key_id_created_at")));
}

/// 函数 `insert_request_log_with_token_stat_is_visible_via_join`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn insert_request_log_with_token_stat_is_visible_via_join() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let created_at = 123456_i64;
    let log = RequestLog {
        trace_id: Some("trc-1".to_string()),
        key_id: Some("gk_1".to_string()),
        account_id: Some("acc_1".to_string()),
        initial_account_id: Some("acc_1".to_string()),
        attempted_account_ids_json: Some(r#"["acc_1"]"#.to_string()),
        request_path: "/v1/responses".to_string(),
        original_path: Some("/v1/chat/completions".to_string()),
        adapted_path: Some("/v1/responses".to_string()),
        method: "POST".to_string(),
        request_type: Some("http".to_string()),
        route_strategy: Some("balanced".to_string()),
        route_source: Some("conversation_bound".to_string()),
        client_model: Some("gpt-5-client".to_string()),
        model: Some("gpt-5".to_string()),
        model_source: Some("gateway_override".to_string()),
        upstream_model: Some("gpt-provider-5".to_string()),
        actual_source_kind: Some("openai_account".to_string()),
        actual_source_id: Some("acc_1".to_string()),
        client_reasoning_effort: Some("low".to_string()),
        reasoning_effort: Some("medium".to_string()),
        reasoning_source: Some("api_key_profile".to_string()),
        service_tier: Some("fast".to_string()),
        effective_service_tier: Some("priority".to_string()),
        service_tier_source: Some("gateway_override".to_string()),
        response_adapter: Some("OpenAIChatCompletionsJson".to_string()),
        upstream_url: Some("https://example.test".to_string()),
        aggregate_api_supplier_name: None,
        aggregate_api_url: None,
        status_code: Some(200),
        duration_ms: Some(1234),
        first_response_ms: Some(456),
        input_tokens: None,
        cached_input_tokens: None,
        output_tokens: None,
        total_tokens: None,
        reasoning_output_tokens: None,
        estimated_cost_usd: None,
        error: None,
        created_at,
        ..Default::default()
    };

    let stat = RequestTokenStat {
        request_log_id: 0,
        key_id: log.key_id.clone(),
        account_id: log.account_id.clone(),
        model: log.model.clone(),
        input_tokens: Some(10),
        cached_input_tokens: Some(1),
        output_tokens: Some(2),
        total_tokens: Some(12),
        reasoning_output_tokens: Some(3),
        estimated_cost_usd: Some(0.123),
        created_at,
        ..Default::default()
    };

    let (_request_log_id, token_err) = storage
        .insert_request_log_with_token_stat(&log, &stat)
        .expect("insert request log with token stat");
    assert!(token_err.is_none(), "token stat should insert");

    let logs = storage
        .list_request_logs(None, 10)
        .expect("list request logs");
    assert_eq!(logs.len(), 1);
    let row = &logs[0];
    assert_eq!(row.trace_id.as_deref(), Some("trc-1"));
    assert_eq!(row.initial_account_id.as_deref(), Some("acc_1"));
    assert_eq!(
        row.attempted_account_ids_json.as_deref(),
        Some(r#"["acc_1"]"#)
    );
    assert_eq!(row.request_path, log.request_path);
    assert_eq!(row.original_path.as_deref(), Some("/v1/chat/completions"));
    assert_eq!(row.adapted_path.as_deref(), Some("/v1/responses"));
    assert_eq!(row.request_type.as_deref(), Some("http"));
    assert_eq!(row.route_strategy.as_deref(), Some("balanced"));
    assert_eq!(row.route_source.as_deref(), Some("conversation_bound"));
    assert_eq!(row.client_model.as_deref(), Some("gpt-5-client"));
    assert_eq!(row.model.as_deref(), Some("gpt-5"));
    assert_eq!(row.model_source.as_deref(), Some("gateway_override"));
    assert_eq!(row.upstream_model.as_deref(), Some("gpt-provider-5"));
    assert_eq!(row.actual_source_kind.as_deref(), Some("openai_account"));
    assert_eq!(row.actual_source_id.as_deref(), Some("acc_1"));
    assert_eq!(row.client_reasoning_effort.as_deref(), Some("low"));
    assert_eq!(row.reasoning_effort.as_deref(), Some("medium"));
    assert_eq!(row.reasoning_source.as_deref(), Some("api_key_profile"));
    assert_eq!(row.service_tier.as_deref(), Some("fast"));
    assert_eq!(row.effective_service_tier.as_deref(), Some("priority"));
    assert_eq!(row.service_tier_source.as_deref(), Some("gateway_override"));
    assert_eq!(row.first_response_ms, Some(456));
    assert_eq!(
        row.response_adapter.as_deref(),
        Some("OpenAIChatCompletionsJson")
    );
    assert_eq!(row.input_tokens, Some(10));
    assert_eq!(row.cached_input_tokens, Some(1));
    assert_eq!(row.output_tokens, Some(2));
    assert_eq!(row.total_tokens, Some(12));
    assert_eq!(row.reasoning_output_tokens, Some(3));
    assert_eq!(row.estimated_cost_usd, Some(0.123));
}

/// 函数 `token_stat_failure_still_commits_request_log`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn token_stat_failure_still_commits_request_log() {
    let storage = Storage::open_in_memory().expect("open");
    // Only create request_logs table, so request_token_stats insert fails.
    storage
        .ensure_request_logs_table()
        .expect("ensure logs table");

    let created_at = 42_i64;
    let log = RequestLog {
        trace_id: Some("trc-2".to_string()),
        key_id: Some("gk_1".to_string()),
        account_id: Some("acc_1".to_string()),
        initial_account_id: Some("acc_1".to_string()),
        attempted_account_ids_json: Some(r#"["acc_1"]"#.to_string()),
        request_path: "/v1/responses".to_string(),
        original_path: Some("/v1/responses".to_string()),
        adapted_path: Some("/v1/responses".to_string()),
        method: "POST".to_string(),
        model: Some("gpt-5".to_string()),
        reasoning_effort: None,
        response_adapter: Some("Passthrough".to_string()),
        upstream_url: None,
        aggregate_api_supplier_name: None,
        aggregate_api_url: None,
        status_code: Some(200),
        duration_ms: None,
        first_response_ms: None,
        input_tokens: None,
        cached_input_tokens: None,
        output_tokens: None,
        total_tokens: None,
        reasoning_output_tokens: None,
        estimated_cost_usd: None,
        error: None,
        created_at,
        ..Default::default()
    };

    let stat = RequestTokenStat {
        request_log_id: 0,
        key_id: log.key_id.clone(),
        account_id: log.account_id.clone(),
        model: log.model.clone(),
        input_tokens: Some(1),
        cached_input_tokens: None,
        output_tokens: None,
        total_tokens: None,
        reasoning_output_tokens: None,
        estimated_cost_usd: None,
        created_at,
        ..Default::default()
    };

    let (_request_log_id, token_err) = storage
        .insert_request_log_with_token_stat(&log, &stat)
        .expect("insert request log with token stat");
    assert!(token_err.is_some(), "token stat insert should fail");

    let count: i64 = storage
        .conn
        .query_row("SELECT COUNT(1) FROM request_logs", [], |row| row.get(0))
        .expect("count request_logs");
    assert_eq!(count, 1);
}

/// 函数 `request_logs_support_backend_pagination_and_status_filters`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn request_logs_support_backend_pagination_and_status_filters() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    for index in 0..5_i64 {
        let created_at = 1_000 + index;
        let status_code = match index {
            0 | 1 => Some(200),
            2 => Some(404),
            _ => Some(502),
        };
        let error = if status_code.unwrap_or_default() >= 500 {
            Some("upstream interrupted".to_string())
        } else {
            None
        };
        let request_log_id = storage
            .insert_request_log(&RequestLog {
                trace_id: Some(format!("trc-{index}")),
                key_id: Some("gk-log".to_string()),
                account_id: Some("acc-log".to_string()),
                initial_account_id: Some("acc-log".to_string()),
                attempted_account_ids_json: Some(r#"["acc-log"]"#.to_string()),
                request_path: format!("/v1/responses/{index}"),
                original_path: Some("/v1/responses".to_string()),
                adapted_path: Some("/v1/responses".to_string()),
                method: "POST".to_string(),
                model: Some("gpt-5".to_string()),
                reasoning_effort: Some("high".to_string()),
                response_adapter: Some("Passthrough".to_string()),
                upstream_url: Some("https://chatgpt.com/backend-api/codex/responses".to_string()),
                aggregate_api_supplier_name: None,
                aggregate_api_url: None,
                status_code,
                duration_ms: Some(200 + index),
                first_response_ms: None,
                input_tokens: None,
                cached_input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                reasoning_output_tokens: None,
                estimated_cost_usd: None,
                error,
                created_at,
                ..Default::default()
            })
            .expect("insert request log");
        storage
            .insert_request_token_stat(&RequestTokenStat {
                request_log_id,
                key_id: Some("gk-log".to_string()),
                account_id: Some("acc-log".to_string()),
                model: Some("gpt-5".to_string()),
                input_tokens: Some(10 + index),
                cached_input_tokens: Some(1),
                output_tokens: Some(2),
                total_tokens: Some(20 + index),
                reasoning_output_tokens: Some(0),
                estimated_cost_usd: Some(0.01),
                created_at,
                ..Default::default()
            })
            .expect("insert token stat");
    }

    let page = storage
        .list_request_logs_paginated(None, Some("5xx"), None, None, 0, 1)
        .expect("list paginated logs");
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].trace_id.as_deref(), Some("trc-4"));

    let total_5xx = storage
        .count_request_logs(None, Some("5xx"), None, None)
        .expect("count 5xx logs");
    assert_eq!(total_5xx, 2);
}

/// 函数 `request_logs_filtered_summary_aggregates_counts_and_tokens`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn request_logs_filtered_summary_aggregates_counts_and_tokens() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    for (index, status_code, total_tokens, error) in [
        (0_i64, Some(200_i64), Some(30_i64), None),
        (1_i64, Some(200_i64), Some(50_i64), None),
        (2_i64, Some(502_i64), Some(70_i64), Some("upstream error")),
    ] {
        let created_at = 2_000 + index;
        let request_log_id = storage
            .insert_request_log(&RequestLog {
                trace_id: Some(format!("trc-sum-{index}")),
                key_id: Some("gk-sum".to_string()),
                account_id: Some("acc-sum".to_string()),
                initial_account_id: Some("acc-sum".to_string()),
                attempted_account_ids_json: Some(r#"["acc-sum"]"#.to_string()),
                request_path: "/v1/responses".to_string(),
                original_path: Some("/v1/responses".to_string()),
                adapted_path: Some("/v1/responses".to_string()),
                method: "POST".to_string(),
                model: Some("gpt-5".to_string()),
                reasoning_effort: Some("medium".to_string()),
                response_adapter: Some("Passthrough".to_string()),
                upstream_url: Some("https://chatgpt.com/backend-api/codex/responses".to_string()),
                aggregate_api_supplier_name: None,
                aggregate_api_url: None,
                status_code,
                duration_ms: Some(900),
                first_response_ms: None,
                input_tokens: None,
                cached_input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                reasoning_output_tokens: None,
                estimated_cost_usd: None,
                error: error.map(|value| value.to_string()),
                created_at,
                ..Default::default()
            })
            .expect("insert request log");
        storage
            .insert_request_token_stat(&RequestTokenStat {
                request_log_id,
                key_id: Some("gk-sum".to_string()),
                account_id: Some("acc-sum".to_string()),
                model: Some("gpt-5".to_string()),
                input_tokens: None,
                cached_input_tokens: None,
                output_tokens: None,
                total_tokens,
                reasoning_output_tokens: Some(0),
                estimated_cost_usd: Some(0.01),
                created_at,
                ..Default::default()
            })
            .expect("insert token stat");
    }

    let summary = storage
        .summarize_request_logs_filtered(None, Some("all"), None, None)
        .expect("summarize filtered logs");
    assert_eq!(summary.count, 3);
    assert_eq!(summary.success_count, 2);
    assert_eq!(summary.error_count, 1);
    assert_eq!(summary.total_tokens, 150);
    assert_eq!(summary.estimated_cost_usd, 0.03);
}

#[test]
fn request_logs_filtered_summary_includes_guard_retry_billable_usage() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let request_log_id = storage
        .insert_request_log(&RequestLog {
            trace_id: Some("trc-guard-summary".to_string()),
            key_id: Some("gk-guard-summary".to_string()),
            account_id: Some("acc-guard-summary".to_string()),
            request_path: "/v1/responses".to_string(),
            method: "POST".to_string(),
            status_code: Some(200),
            created_at: 2_100,
            ..Default::default()
        })
        .expect("insert request log");
    storage
        .insert_request_token_stat(&RequestTokenStat {
            request_log_id,
            key_id: Some("gk-guard-summary".to_string()),
            account_id: Some("acc-guard-summary".to_string()),
            total_tokens: Some(100),
            estimated_cost_usd: Some(0.10),
            created_at: 2_100,
            ..Default::default()
        })
        .expect("insert token stat");
    storage
        .insert_gateway_reasoning_guard_event(&GatewayReasoningGuardEvent {
            trace_id: Some("trc-guard-summary".to_string()),
            mode: "non_stream".to_string(),
            action: "internal_retry".to_string(),
            source_kind: Some("openai_account".to_string()),
            source_id: Some("acc-guard-summary".to_string()),
            total_tokens: Some(40),
            estimated_cost_usd: Some(0.04),
            created_at: 2_101,
            ..Default::default()
        })
        .expect("insert guard retry event");
    storage
        .insert_gateway_reasoning_guard_event(&GatewayReasoningGuardEvent {
            trace_id: Some("trc-guard-summary".to_string()),
            mode: "non_stream".to_string(),
            action: "block".to_string(),
            source_kind: Some("openai_account".to_string()),
            source_id: Some("acc-guard-summary".to_string()),
            total_tokens: Some(999),
            estimated_cost_usd: Some(9.99),
            created_at: 2_102,
            ..Default::default()
        })
        .expect("insert guard block event");

    let summary = storage
        .summarize_request_logs_filtered(None, Some("all"), Some(2_000), Some(2_200))
        .expect("summarize filtered logs");
    assert_eq!(summary.count, 1);
    assert_eq!(summary.total_tokens, 140);
    assert!((summary.estimated_cost_usd - 0.14).abs() < f64::EPSILON);
    assert_eq!(summary.guard_retry_total_tokens, 40);
    assert!((summary.guard_retry_estimated_cost_usd - 0.04).abs() < f64::EPSILON);
}

#[test]
fn request_token_stats_total_includes_current_rows_and_rollups() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    for (index, status_code, total_tokens, cost) in [
        (0_i64, Some(200_i64), Some(30_i64), Some(0.03_f64)),
        (1_i64, Some(502_i64), Some(70_i64), Some(0.07_f64)),
    ] {
        let created_at = 2_000 + index;
        let request_log_id = storage
            .insert_request_log(&RequestLog {
                trace_id: Some(format!("trc-total-{index}")),
                key_id: Some("gk-total".to_string()),
                account_id: Some("acc-total".to_string()),
                request_path: "/v1/responses".to_string(),
                method: "POST".to_string(),
                status_code,
                created_at,
                ..Default::default()
            })
            .expect("insert request log");
        storage
            .insert_request_token_stat(&RequestTokenStat {
                request_log_id,
                key_id: Some("gk-total".to_string()),
                account_id: Some("acc-total".to_string()),
                model: Some("gpt-5".to_string()),
                input_tokens: Some(10),
                cached_input_tokens: Some(2),
                output_tokens: Some(5),
                total_tokens,
                estimated_cost_usd: cost,
                created_at,
                ..Default::default()
            })
            .expect("insert token stat");
    }

    let before_rollup = storage
        .summarize_request_token_stats_total()
        .expect("summarize current total");
    assert_eq!(before_rollup.total_tokens, 100);
    assert_eq!(before_rollup.request_count, 2);
    assert_eq!(before_rollup.success_count, 1);
    assert_eq!(before_rollup.error_count, 1);
    assert_eq!(before_rollup.estimated_cost_usd, 0.10);

    storage
        .rollup_all_request_token_stats()
        .expect("rollup token stats");

    let after_rollup = storage
        .summarize_request_token_stats_total()
        .expect("summarize rolled total");
    assert_eq!(after_rollup.total_tokens, 100);
    assert_eq!(after_rollup.request_count, 2);
    assert_eq!(after_rollup.success_count, 1);
    assert_eq!(after_rollup.error_count, 1);
    assert_eq!(after_rollup.estimated_cost_usd, 0.10);
}

#[test]
fn request_token_activity_daily_survives_stats_rollup() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    for (index, created_at, total_tokens) in [
        (0_i64, 1_700_000_000_i64, 30_i64),
        (1_i64, 1_700_086_900_i64, 70_i64),
    ] {
        let request_log_id = storage
            .insert_request_log(&RequestLog {
                trace_id: Some(format!("trc-activity-{index}")),
                key_id: Some("gk-activity".to_string()),
                account_id: Some("acc-activity".to_string()),
                request_path: "/v1/responses".to_string(),
                method: "POST".to_string(),
                status_code: Some(200),
                duration_ms: Some(1000 + index),
                created_at,
                ..Default::default()
            })
            .expect("insert request log");
        storage
            .insert_request_token_stat(&RequestTokenStat {
                request_log_id,
                key_id: Some("gk-activity".to_string()),
                account_id: Some("acc-activity".to_string()),
                model: Some("gpt-5".to_string()),
                input_tokens: Some(total_tokens),
                output_tokens: Some(0),
                total_tokens: Some(total_tokens),
                estimated_cost_usd: Some(total_tokens as f64 / 1000.0),
                created_at,
                ..Default::default()
            })
            .expect("insert token stat");
    }
    storage
        .insert_request_token_stat(&RequestTokenStat {
            request_log_id: 98_001,
            key_id: Some("gk-activity".to_string()),
            account_id: Some("acc-activity".to_string()),
            model: Some("gpt-5".to_string()),
            input_tokens: Some(5),
            output_tokens: Some(0),
            total_tokens: Some(5),
            estimated_cost_usd: Some(0.005),
            created_at: 1_700_000_120,
            ..Default::default()
        })
        .expect("insert orphan token stat");

    storage
        .rollup_all_request_token_stats()
        .expect("rollup token stats");

    let remaining_current_rows: i64 = storage
        .conn
        .query_row("SELECT COUNT(1) FROM request_token_stats", [], |row| {
            row.get(0)
        })
        .expect("count current stats");
    assert_eq!(remaining_current_rows, 0);

    let days = storage
        .summarize_request_token_activity_daily(1_699_900_000, 1_700_200_000, 86_400)
        .expect("summarize token activity");
    let totals = days
        .iter()
        .map(|item| item.usage.total_tokens)
        .collect::<Vec<_>>();
    assert_eq!(totals, vec![35, 70]);
    assert_eq!(days[0].usage.request_count, 1);
    assert_eq!(days[1].usage.request_count, 1);
}

#[test]
fn request_logs_support_time_range_filters() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    for (index, created_at) in [1_000_i64, 1_900_i64, 3_100_i64].into_iter().enumerate() {
        let request_log_id = storage
            .insert_request_log(&RequestLog {
                trace_id: Some(format!("trc-time-{index}")),
                key_id: Some("gk-time".to_string()),
                account_id: Some("acc-time".to_string()),
                request_path: "/v1/responses".to_string(),
                method: "POST".to_string(),
                status_code: Some(200),
                created_at,
                ..Default::default()
            })
            .expect("insert request log");
        storage
            .insert_request_token_stat(&RequestTokenStat {
                request_log_id,
                key_id: Some("gk-time".to_string()),
                account_id: Some("acc-time".to_string()),
                model: Some("gpt-5".to_string()),
                total_tokens: Some(10),
                estimated_cost_usd: Some(0.01),
                created_at,
                ..Default::default()
            })
            .expect("insert token stat");
    }

    let page = storage
        .list_request_logs_paginated(None, None, Some(1_500), Some(3_000), 0, 10)
        .expect("list paginated logs");
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].trace_id.as_deref(), Some("trc-time-1"));

    let total = storage
        .count_request_logs(None, None, Some(1_500), Some(3_000))
        .expect("count logs");
    assert_eq!(total, 1);

    let summary = storage
        .summarize_request_logs_filtered(None, None, Some(900), Some(2_000))
        .expect("summarize time range");
    assert_eq!(summary.count, 2);
    assert_eq!(summary.total_tokens, 20);
}

#[test]
fn summarizes_daily_usage_by_account_from_token_stats() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    for (request_log_id, account_id, input, cached, output, total, cost, created_at) in [
        (
            1_i64,
            Some("acc-a"),
            Some(100_i64),
            Some(40_i64),
            Some(20_i64),
            Some(120_i64),
            Some(0.10_f64),
            1_100_i64,
        ),
        (
            2_i64,
            Some("acc-a"),
            Some(50_i64),
            Some(10_i64),
            Some(5_i64),
            None,
            Some(0.02_f64),
            1_200_i64,
        ),
        (
            3_i64,
            Some("acc-b"),
            Some(30_i64),
            Some(0_i64),
            Some(10_i64),
            Some(40_i64),
            Some(0.03_f64),
            1_300_i64,
        ),
        (
            4_i64,
            Some("acc-a"),
            Some(999_i64),
            Some(0_i64),
            Some(0_i64),
            None,
            Some(9.99),
            2_500_i64,
        ),
        (
            5_i64,
            Some("  "),
            Some(10_i64),
            Some(5_i64),
            Some(2_i64),
            None,
            Some(0.01),
            1_400_i64,
        ),
    ] {
        storage
            .insert_request_token_stat(&RequestTokenStat {
                request_log_id,
                account_id: account_id.map(str::to_string),
                model: Some("gpt-5".to_string()),
                input_tokens: input,
                cached_input_tokens: cached,
                output_tokens: output,
                total_tokens: total,
                reasoning_output_tokens: Some(0),
                estimated_cost_usd: cost,
                created_at,
                ..Default::default()
            })
            .expect("insert token stat");
    }

    storage.clear_request_logs().expect("clear logs");

    let summaries = storage
        .summarize_request_token_stats_by_account_between(1_000, 2_000)
        .expect("summarize account daily usage");
    assert_eq!(summaries.len(), 2);

    let first = &summaries[0];
    assert_eq!(first.account_id, "acc-a");
    assert_eq!(first.request_count, 2);
    assert_eq!(first.input_tokens, 150);
    assert_eq!(first.cached_input_tokens, 50);
    assert_eq!(first.billable_input_tokens, 100);
    assert_eq!(first.output_tokens, 25);
    assert_eq!(first.total_tokens, 165);
    assert!((first.estimated_cost_usd - 0.12).abs() < f64::EPSILON);
    assert!((first.cache_hit_rate - (50.0 / 150.0)).abs() < f64::EPSILON);

    let second = &summaries[1];
    assert_eq!(second.account_id, "acc-b");
    assert_eq!(second.request_count, 1);
}

#[test]
fn summarizes_daily_usage_by_aggregate_api_from_token_stats() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    for (request_log_id, api_id, supplier, url, input, cached, output, total, cost, created_at) in [
        (
            11_i64,
            Some("ag-a"),
            Some("Supplier A"),
            Some("https://a.example/v1"),
            Some(100_i64),
            Some(25_i64),
            Some(15_i64),
            Some(115_i64),
            Some(0.11_f64),
            1_100_i64,
        ),
        (
            12_i64,
            Some("ag-a"),
            Some("Supplier A"),
            Some("https://a.example/v1"),
            Some(80_i64),
            Some(20_i64),
            Some(10_i64),
            None,
            Some(0.08_f64),
            1_200_i64,
        ),
        (
            13_i64,
            Some("ag-b"),
            Some("Supplier B"),
            Some("https://b.example/v1"),
            Some(50_i64),
            Some(0_i64),
            Some(5_i64),
            Some(55_i64),
            Some(0.05_f64),
            1_300_i64,
        ),
        (
            14_i64,
            Some("ag-a"),
            Some("Supplier A"),
            Some("https://a.example/v1"),
            Some(999_i64),
            Some(0_i64),
            Some(0_i64),
            None,
            Some(9.99),
            2_500_i64,
        ),
        (
            15_i64,
            Some(""),
            Some("Blank"),
            Some("https://blank.example/v1"),
            Some(10_i64),
            Some(1_i64),
            Some(1_i64),
            None,
            Some(0.01),
            1_400_i64,
        ),
    ] {
        storage
            .insert_request_token_stat(&RequestTokenStat {
                request_log_id,
                aggregate_api_id: api_id.map(str::to_string),
                aggregate_api_supplier_name: supplier.map(str::to_string),
                aggregate_api_url: url.map(str::to_string),
                model: Some("gpt-5".to_string()),
                input_tokens: input,
                cached_input_tokens: cached,
                output_tokens: output,
                total_tokens: total,
                reasoning_output_tokens: Some(0),
                estimated_cost_usd: cost,
                created_at,
                ..Default::default()
            })
            .expect("insert token stat");
    }

    storage.clear_request_logs().expect("clear logs");
    storage
        .insert_gateway_reasoning_guard_event(&GatewayReasoningGuardEvent {
            trace_id: Some("trc-ag-a-guard".to_string()),
            mode: "non_stream".to_string(),
            action: "internal_retry".to_string(),
            source_kind: Some("aggregate_api".to_string()),
            source_id: Some("ag-a".to_string()),
            supplier_name: Some("Supplier A".to_string()),
            total_tokens: Some(40),
            estimated_cost_usd: Some(0.04),
            created_at: 1_250,
            ..Default::default()
        })
        .expect("insert aggregate guard retry event");
    storage
        .insert_gateway_reasoning_guard_event(&GatewayReasoningGuardEvent {
            trace_id: Some("trc-ag-a-block".to_string()),
            mode: "non_stream".to_string(),
            action: "block".to_string(),
            source_kind: Some("aggregate_api".to_string()),
            source_id: Some("ag-a".to_string()),
            supplier_name: Some("Supplier A".to_string()),
            total_tokens: Some(999),
            estimated_cost_usd: Some(9.99),
            created_at: 1_260,
            ..Default::default()
        })
        .expect("insert aggregate guard block event");

    let summaries = storage
        .summarize_request_token_stats_by_aggregate_api_between(1_000, 2_000)
        .expect("summarize aggregate api daily usage");
    assert_eq!(summaries.len(), 2);

    let first = &summaries[0];
    assert_eq!(first.aggregate_api_id, "ag-a");
    assert_eq!(
        first.aggregate_api_supplier_name.as_deref(),
        Some("Supplier A")
    );
    assert_eq!(
        first.aggregate_api_url.as_deref(),
        Some("https://a.example/v1")
    );
    assert_eq!(first.request_count, 2);
    assert_eq!(first.input_tokens, 180);
    assert_eq!(first.cached_input_tokens, 45);
    assert_eq!(first.billable_input_tokens, 135);
    assert_eq!(first.output_tokens, 25);
    assert_eq!(first.total_tokens, 185);
    assert!((first.estimated_cost_usd - 0.19).abs() < f64::EPSILON);
    assert_eq!(first.guard_retry_total_tokens, 40);
    assert!((first.guard_retry_estimated_cost_usd - 0.04).abs() < f64::EPSILON);
    assert_eq!(first.billable_total_tokens, 225);
    assert!((first.billable_estimated_cost_usd - 0.23).abs() < f64::EPSILON);
    assert!((first.cache_hit_rate - 0.25).abs() < f64::EPSILON);

    let second = &summaries[1];
    assert_eq!(second.aggregate_api_id, "ag-b");
    assert_eq!(second.request_count, 1);
}

#[test]
fn token_stats_aggregate_api_backfill_updates_existing_rows_to_final_attempt() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let request_log_id = storage
        .insert_request_log(&RequestLog {
            trace_id: Some("trc-ag-final".to_string()),
            key_id: Some("gk-ag".to_string()),
            account_id: Some("acc-ag".to_string()),
            initial_aggregate_api_id: Some("ag-initial".to_string()),
            attempted_aggregate_api_ids_json: Some(
                r#"["ag-initial","ag-failover","ag-final"]"#.to_string(),
            ),
            request_path: "/v1/responses".to_string(),
            method: "POST".to_string(),
            aggregate_api_supplier_name: Some("Supplier Final".to_string()),
            aggregate_api_url: Some("https://final.example/v1".to_string()),
            status_code: Some(200),
            created_at: 1_100,
            ..Default::default()
        })
        .expect("insert request log");
    storage
        .insert_request_token_stat(&RequestTokenStat {
            request_log_id,
            aggregate_api_id: Some("ag-initial".to_string()),
            input_tokens: Some(100),
            cached_input_tokens: Some(20),
            output_tokens: Some(10),
            total_tokens: Some(110),
            estimated_cost_usd: Some(0.10),
            created_at: 1_100,
            ..Default::default()
        })
        .expect("insert token stat");

    storage
        .ensure_request_token_stats_table()
        .expect("ensure request token stats table");

    let summaries = storage
        .summarize_request_token_stats_by_aggregate_api_between(1_000, 2_000)
        .expect("summarize aggregate api daily usage");
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].aggregate_api_id, "ag-final");
    assert_eq!(
        summaries[0].aggregate_api_supplier_name.as_deref(),
        Some("Supplier Final")
    );
    assert_eq!(
        summaries[0].aggregate_api_url.as_deref(),
        Some("https://final.example/v1")
    );
}

#[test]
fn token_stats_aggregate_api_backfill_falls_back_to_initial_when_attempts_invalid() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let request_log_id = storage
        .insert_request_log(&RequestLog {
            trace_id: Some("trc-ag-initial".to_string()),
            key_id: Some("gk-ag".to_string()),
            account_id: Some("acc-ag".to_string()),
            initial_aggregate_api_id: Some("ag-initial".to_string()),
            attempted_aggregate_api_ids_json: Some("not-json".to_string()),
            request_path: "/v1/responses".to_string(),
            method: "POST".to_string(),
            status_code: Some(200),
            created_at: 1_200,
            ..Default::default()
        })
        .expect("insert request log");
    storage
        .insert_request_token_stat(&RequestTokenStat {
            request_log_id,
            input_tokens: Some(50),
            output_tokens: Some(5),
            total_tokens: Some(55),
            estimated_cost_usd: Some(0.05),
            created_at: 1_200,
            ..Default::default()
        })
        .expect("insert token stat");

    storage
        .ensure_request_token_stats_table()
        .expect("ensure request token stats table");

    let summaries = storage
        .summarize_request_token_stats_by_aggregate_api_between(1_000, 2_000)
        .expect("summarize aggregate api daily usage");
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].aggregate_api_id, "ag-initial");
}

#[test]
fn request_logs_for_empty_key_sets_return_empty_results() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let empty_keys = vec![" ".to_string(), String::new()];

    let logs = storage
        .list_request_logs_paginated_for_keys(None, None, None, None, 0, 20, &empty_keys)
        .expect("list logs for empty keys");
    assert!(logs.is_empty());

    let total = storage
        .count_request_logs_for_keys(None, None, None, None, &empty_keys)
        .expect("count logs for empty keys");
    assert_eq!(total, 0);

    let filtered = storage
        .summarize_request_logs_filtered_for_keys(None, None, None, None, &empty_keys)
        .expect("summarize logs for empty keys");
    assert_eq!(filtered.count, 0);
    assert_eq!(filtered.total_tokens, 0);

    let today = storage
        .summarize_request_logs_between_for_keys(0, 10_000, &empty_keys)
        .expect("summarize today for empty keys");
    assert_eq!(today.input_tokens, 0);
    assert_eq!(today.estimated_cost_usd, 0.0);
}

#[test]
fn request_logs_for_large_key_sets_use_temp_filter() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let request_log_id = storage
        .insert_request_log(&RequestLog {
            trace_id: Some("trc-large-key-filter".to_string()),
            key_id: Some("key-0949".to_string()),
            account_id: Some("acc-large-key-filter".to_string()),
            request_path: "/v1/responses".to_string(),
            method: "POST".to_string(),
            status_code: Some(200),
            created_at: 5_000,
            ..Default::default()
        })
        .expect("insert request log");
    storage
        .insert_request_token_stat(&RequestTokenStat {
            request_log_id,
            key_id: Some("key-0949".to_string()),
            account_id: Some("acc-large-key-filter".to_string()),
            model: Some("gpt-5".to_string()),
            input_tokens: Some(30),
            cached_input_tokens: Some(5),
            output_tokens: Some(10),
            total_tokens: Some(40),
            reasoning_output_tokens: Some(2),
            estimated_cost_usd: Some(0.04),
            created_at: 5_000,
            ..Default::default()
        })
        .expect("insert token stat");

    let key_ids = (0..950)
        .map(|index| format!("key-{index:04}"))
        .collect::<Vec<_>>();

    let total = storage
        .count_request_logs_for_keys(None, None, None, None, &key_ids)
        .expect("count logs for large key set");
    assert_eq!(total, 1);

    let logs = storage
        .list_request_logs_paginated_for_keys(None, None, None, None, 0, 20, &key_ids)
        .expect("list logs for large key set");
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].trace_id.as_deref(), Some("trc-large-key-filter"));

    let summary = storage
        .summarize_request_logs_between_for_keys(4_000, 6_000, &key_ids)
        .expect("summarize today for large key set");
    assert_eq!(summary.input_tokens, 30);
    assert_eq!(summary.output_tokens, 10);
    assert_eq!(summary.estimated_cost_usd, 0.04);
}
