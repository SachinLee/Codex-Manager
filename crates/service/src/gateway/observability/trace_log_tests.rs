use super::{
    clear_trace_error, clear_trace_state, gateway_trace_stdout_enabled,
    gateway_trace_success_enabled, has_error_text, log_failed_request,
    log_request_affinity_profile, mark_trace_has_error, sanitize_text, should_flush_success_trace,
    trace_has_error, trace_pending_lines, trace_queue_capacity, RequestAffinityProfileLog,
};

/// 函数 `has_error_text_ignores_empty_and_dash`
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
fn has_error_text_ignores_empty_and_dash() {
    assert!(!has_error_text(None));
    assert!(!has_error_text(Some("")));
    assert!(!has_error_text(Some(" - ")));
    assert!(has_error_text(Some("upstream failed")));
}

/// 函数 `trace_error_state_can_mark_and_clear`
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
fn trace_error_state_can_mark_and_clear() {
    let trace_id = "trc_trace_log_unit";
    clear_trace_error(trace_id);
    assert!(!trace_has_error(trace_id));
    mark_trace_has_error(trace_id);
    assert!(trace_has_error(trace_id));
    clear_trace_error(trace_id);
    assert!(!trace_has_error(trace_id));
}

/// 函数 `request_record_ignores_success_without_error`
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
fn request_record_ignores_success_without_error() {
    log_failed_request(super::FailedRequestLog {
        ts: 1_772_000_000,
        trace_id: Some("trc_success"),
        key_id: Some("gk_success"),
        account_id: Some("acc_success"),
        method: "POST",
        request_path: "/v1/responses",
        original_path: Some("/v1/responses"),
        adapted_path: Some("/v1/responses"),
        request_type: Some("http"),
        model: Some("gpt-5.4"),
        reasoning_effort: Some("high"),
        service_tier: Some("fast"),
        upstream_url: Some("https://chatgpt.com/backend-api/codex/responses"),
        status_code: Some(200),
        error: None,
        duration_ms: Some(18),
    });
}

#[test]
fn trace_queue_capacity_zero_means_unbounded() {
    let _guard = crate::test_env_guard();
    std::env::set_var("CODEXMANAGER_TRACE_QUEUE_CAPACITY", "0");

    assert_eq!(trace_queue_capacity(), 0);
}

#[test]
fn gateway_trace_stdout_flag_accepts_error_values() {
    let _guard = crate::test_env_guard();
    std::env::remove_var("CODEXMANAGER_GATEWAY_TRACE_STDOUT");
    std::env::remove_var("CODEXMANAGER_GATEWAY_TRACE_STDOUT_SLOW_MS");
    assert!(!gateway_trace_stdout_enabled());

    std::env::set_var("CODEXMANAGER_GATEWAY_TRACE_STDOUT", "error_only");
    assert!(gateway_trace_stdout_enabled());
    std::env::remove_var("CODEXMANAGER_GATEWAY_TRACE_STDOUT");
}

#[test]
fn success_trace_flush_requires_stdout_and_slow_threshold() {
    let _guard = crate::test_env_guard();
    std::env::remove_var("CODEXMANAGER_GATEWAY_TRACE_SUCCESS");
    std::env::remove_var("CODEXMANAGER_GATEWAY_TRACE_STDOUT");
    std::env::set_var("CODEXMANAGER_GATEWAY_TRACE_STDOUT_SLOW_MS", "1000");
    assert!(!should_flush_success_trace(1500));

    std::env::set_var("CODEXMANAGER_GATEWAY_TRACE_STDOUT", "1");
    assert!(!should_flush_success_trace(999));
    assert!(should_flush_success_trace(1000));
    std::env::remove_var("CODEXMANAGER_GATEWAY_TRACE_STDOUT");
    std::env::remove_var("CODEXMANAGER_GATEWAY_TRACE_STDOUT_SLOW_MS");
}

#[test]
fn success_trace_flush_can_be_forced_without_slow_threshold() {
    let _guard = crate::test_env_guard();
    std::env::remove_var("CODEXMANAGER_GATEWAY_TRACE_STDOUT");
    std::env::remove_var("CODEXMANAGER_GATEWAY_TRACE_STDOUT_SLOW_MS");
    std::env::remove_var("CODEXMANAGER_GATEWAY_TRACE_SUCCESS");
    assert!(!should_flush_success_trace(0));

    std::env::set_var("CODEXMANAGER_GATEWAY_TRACE_SUCCESS", "1");
    assert!(gateway_trace_success_enabled());
    assert!(should_flush_success_trace(0));

    std::env::remove_var("CODEXMANAGER_GATEWAY_TRACE_SUCCESS");
    assert!(!should_flush_success_trace(0));
}

#[test]
fn affinity_profile_line_carries_fingerprint_not_raw_cache_key() {
    let _guard = crate::test_env_guard();
    let trace_id = "trc_affinity_profile_unit";
    clear_trace_state(trace_id);
    let raw_key = "01a004d1-7931-7000-be28-6a5415a2baa6";
    log_request_affinity_profile(RequestAffinityProfileLog {
        trace_id,
        key_hash: "kh_unit",
        protocol_type: "openai_compat",
        path: "/v1/responses",
        anchor_mode: "cache_affinity",
        prompt_cache_key: Some(raw_key),
        has_incoming_session: true,
        has_incoming_turn_state: false,
        has_incoming_conversation: false,
        request_shape: Some("fp=abcd;k=input+model"),
    });
    let line = trace_pending_lines()
        .lock()
        .ok()
        .and_then(|pending| {
            pending
                .get(trace_id)
                .and_then(|lines| lines.last().cloned())
        })
        .expect("affinity profile line must be buffered");
    clear_trace_state(trace_id);

    assert!(line.contains("event=REQUEST_AFFINITY_PROFILE"));
    assert!(line.contains("anchor_mode=cache_affinity"));
    assert!(line.contains("incoming_session=true"));
    assert!(line.contains("request_shape=fp=abcd;k=input+model"));
    assert!(
        !line.contains(raw_key),
        "raw prompt_cache_key must never be logged"
    );
}

#[test]
fn sanitize_text_redacts_base64_image_data_urls() {
    let sanitized = sanitize_text(
        "payload=data:image/png;base64,aGVsbG8= next=data:text/plain;base64,dmFsdWU=",
    );

    assert!(sanitized.contains("data:image/png;base64,<base64 image omitted>"));
    assert!(!sanitized.contains("aGVsbG8="));
    assert!(sanitized.contains("data:text/plain;base64,dmFsdWU="));
}

#[test]
fn sanitize_text_redacts_image_generation_result_payloads() {
    let sanitized = sanitize_text(
        "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"image_generation_call\",\"result\":\"QUJDREVGRw==\"}}\n\
         data: {\"type\":\"response.image_generation_call.partial_image\",\"partial_image_b64\":\"cGFydA==\"}",
    );

    assert!(sanitized.contains("\"result\":\"<base64 image omitted>\""));
    assert!(sanitized.contains("\"partial_image_b64\":\"<base64 image omitted>\""));
    assert!(!sanitized.contains("QUJDREVGRw=="));
    assert!(!sanitized.contains("cGFydA=="));
}
