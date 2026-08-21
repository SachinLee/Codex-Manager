use codexmanager_core::storage::{now_ts, AggregateApi, Storage};

use super::{
    build_anthropic_bridge_aggregate_api_request, build_upstream_url, effective_action_path,
    next_aggregate_api_capacity_retry, proxy_aggregate_request,
    resolve_aggregate_api_rotation_candidates, resolve_passthrough_sse_protocol,
    responses_to_anthropic_messages_action_path, rewrite_body_model_override,
    schedule_aggregate_api_capacity_retry, should_bridge_responses_to_anthropic,
    AggregateApiCapacityRetryAction, AggregateProxyOutcome, AggregateProxyRequest,
    AGGREGATE_API_CAPACITY_RETRY_ATTEMPTS, AGGREGATE_API_CAPACITY_RETRY_BACKOFF_BASE,
    AGGREGATE_API_CAPACITY_RETRY_BACKOFF_CAP,
};
use crate::aggregate_api::{
    AGGREGATE_API_AUTH_APIKEY, AGGREGATE_API_PROVIDER_CLAUDE, AGGREGATE_API_PROVIDER_CODEX,
    AGGREGATE_API_PROVIDER_COMPATIBLE, AGGREGATE_API_PROVIDER_GEMINI,
};
use crate::gateway::upstream::support::payload_rewrite::build_continuation_recovery_body;
use crate::gateway::{PassthroughSseProtocol, ResponseAdapter};
use bytes::Bytes;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Read;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tiny_http::{Response, Server, StatusCode};

#[test]
fn aggregate_api_capacity_retry_budget_allows_exactly_two_replays() {
    let mut remaining = AGGREGATE_API_CAPACITY_RETRY_ATTEMPTS;

    let AggregateApiCapacityRetryAction::Retry {
        retry_attempt: first_attempt,
        delay: first_delay,
    } = next_aggregate_api_capacity_retry(&mut remaining)
    else {
        panic!("first capacity retry should be available");
    };
    assert_eq!(first_attempt, 1);
    assert!(first_delay <= AGGREGATE_API_CAPACITY_RETRY_BACKOFF_BASE);
    assert_eq!(remaining, 1);

    let AggregateApiCapacityRetryAction::Retry {
        retry_attempt: second_attempt,
        delay: second_delay,
    } = next_aggregate_api_capacity_retry(&mut remaining)
    else {
        panic!("second capacity retry should be available");
    };
    assert_eq!(second_attempt, 2);
    assert!(second_delay <= AGGREGATE_API_CAPACITY_RETRY_BACKOFF_CAP);
    assert_eq!(remaining, 0);

    assert_eq!(
        next_aggregate_api_capacity_retry(&mut remaining),
        AggregateApiCapacityRetryAction::Exhausted
    );
}

#[test]
fn aggregate_api_capacity_retry_does_not_wait_past_request_deadline() {
    let mut remaining = AGGREGATE_API_CAPACITY_RETRY_ATTEMPTS;
    let action = schedule_aggregate_api_capacity_retry(
        &mut remaining,
        Some(Instant::now() - Duration::from_millis(1)),
        "trc-deadline",
        "agg-deadline",
        None,
        false,
        Some("2"),
    );
    assert_eq!(action, AggregateApiCapacityRetryAction::DeadlineExceeded);
}

#[test]
fn capacity_recovery_metrics_are_exported() {
    let metrics = crate::gateway::gateway_metrics_prometheus();
    assert!(metrics.contains("codexmanager_gateway_upstream_capacity_errors_total"));
    assert!(metrics.contains("codexmanager_gateway_upstream_capacity_exhausted_total"));
}

fn aggregate_api_with_action(action: Option<&str>) -> AggregateApi {
    AggregateApi {
        id: "agg-path-test".to_string(),
        provider_type: "claude".to_string(),
        supplier_name: Some("test".to_string()),
        sort: 0,
        url: "https://open.bigmodel.cn/api/anthropic".to_string(),
        auth_type: "apikey".to_string(),
        auth_params_json: None,
        action: action.map(str::to_string),
        model_override: None,
        cost_multiplier: 1.0,
        daily_spend_limit_usd: None,
        status: "active".to_string(),
        created_at: 0,
        updated_at: 0,
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
        upstream_protocol: None,
    }
}

#[test]
fn empty_custom_action_uses_base_url_without_original_path() {
    let api = aggregate_api_with_action(Some(""));
    let path = effective_action_path(&api, "/v1/messages?beta=true");
    assert_eq!(path, "");
}

#[test]
fn messages_passthrough_uses_anthropic_native_terminal_rules_without_provider_gate() {
    let protocol =
        resolve_passthrough_sse_protocol("/v1/messages?beta=true", ResponseAdapter::Passthrough);
    assert_eq!(protocol, Some(PassthroughSseProtocol::AnthropicNative));
}

#[test]
fn messages_passthrough_protocol_still_requires_passthrough_adapter() {
    let protocol = resolve_passthrough_sse_protocol(
        "/v1/messages?beta=true",
        ResponseAdapter::AnthropicMessagesFromResponses,
    );
    assert_eq!(protocol, None);
}

#[test]
fn build_upstream_url_preserves_base_path_prefix() {
    let url = build_upstream_url(
        "https://open.bigmodel.cn/api/anthropic",
        "/v1/messages?beta=true",
    )
    .expect("build upstream url");
    assert_eq!(
        url.as_str(),
        "https://open.bigmodel.cn/api/anthropic/v1/messages?beta=true"
    );
}

#[test]
fn build_upstream_url_keeps_root_base_behavior() {
    let url = build_upstream_url("https://api.example.com", "/v1/messages?beta=true")
        .expect("build upstream url");
    assert_eq!(
        url.as_str(),
        "https://api.example.com/v1/messages?beta=true"
    );
}

#[test]
fn build_upstream_url_deduplicates_v1_base_path() {
    let url = build_upstream_url("https://api.minimax.io/v1", "/v1/responses")
        .expect("build upstream url");

    assert_eq!(url.as_str(), "https://api.minimax.io/v1/responses");
}

#[test]
fn responses_bridge_uses_messages_suffix_for_anthropic_v1_base_url() {
    let mut api = aggregate_api_with_action(None);
    api.url = "https://api.anthropic.com/v1".to_string();

    let path = responses_to_anthropic_messages_action_path(&api, "/v1/responses");
    let url = build_upstream_url(api.url.as_str(), path.as_str()).expect("build upstream url");

    assert_eq!(url.as_str(), "https://api.anthropic.com/v1/messages");
}

#[test]
fn responses_bridge_keeps_v1_messages_for_deepseek_anthropic_base_url() {
    let mut api = aggregate_api_with_action(None);
    api.url = "https://api.deepseek.com/anthropic".to_string();

    let path = responses_to_anthropic_messages_action_path(&api, "/v1/responses");
    let url = build_upstream_url(api.url.as_str(), path.as_str()).expect("build upstream url");

    assert_eq!(
        url.as_str(),
        "https://api.deepseek.com/anthropic/v1/messages"
    );
}

#[test]
fn responses_bridge_respects_custom_action_path() {
    let mut api = aggregate_api_with_action(Some("/messages?beta=true"));
    api.url = "https://api.anthropic.com/v1".to_string();

    let path = responses_to_anthropic_messages_action_path(&api, "/v1/responses");
    let url = build_upstream_url(api.url.as_str(), path.as_str()).expect("build upstream url");

    assert_eq!(
        path.as_str(),
        "/messages?beta=true",
        "custom action should remain the upstream bridge action"
    );
    assert_eq!(
        url.as_str(),
        "https://api.anthropic.com/v1/messages?beta=true"
    );
}

#[test]
fn anthropic_bridge_request_adds_required_messages_headers_with_default_auth() {
    let request: tiny_http::Request = tiny_http::TestRequest::new()
        .with_header(
            tiny_http::Header::from_bytes("Authorization", "Bearer client-key")
                .expect("auth header"),
        )
        .into();
    let client = reqwest::blocking::Client::new();
    let builder = build_anthropic_bridge_aggregate_api_request(
        &client,
        &request,
        &reqwest::Method::POST,
        reqwest::Url::parse("https://api.anthropic.com/v1/messages").expect("url"),
        &Bytes::from_static(br#"{"model":"claude-sonnet","messages":[]}"#),
        "sk-ant-test",
        &crate::gateway::upstream::protocol::aggregate_api::AggregateApiAuthConfig::ApiKeyDefaultBearer,
        &std::collections::HashSet::new(),
        None,
        true,
    )
    .expect("build request")
    .build()
    .expect("finalize request");

    assert_eq!(
        builder
            .headers()
            .get("authorization")
            .and_then(|value| value.to_str().ok()),
        Some("Bearer sk-ant-test")
    );
    assert_eq!(
        builder
            .headers()
            .get("x-api-key")
            .and_then(|value| value.to_str().ok()),
        Some("sk-ant-test")
    );
    assert_eq!(
        builder
            .headers()
            .get("anthropic-version")
            .and_then(|value| value.to_str().ok()),
        Some("2023-06-01")
    );
    assert_eq!(
        builder
            .headers()
            .get("accept")
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
}

#[test]
fn rewrite_body_model_override_replaces_json_model() {
    let body = Bytes::from_static(br#"{"model":"claude-sonnet","messages":[]}"#);

    let rewritten = rewrite_body_model_override(&body, Some("qwen3.5-plus"));

    let value: serde_json::Value =
        serde_json::from_slice(rewritten.as_ref()).expect("parse rewritten body");
    assert_eq!(value["model"], "qwen3.5-plus");
    assert_eq!(value["messages"].as_array().map(Vec::len), Some(0));
}

#[test]
fn gemini_native_candidates_resolve_to_gemini_provider_only() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let now = now_ts();
    for (id, provider_type) in [
        ("agg-codex", AGGREGATE_API_PROVIDER_CODEX),
        ("agg-claude", AGGREGATE_API_PROVIDER_CLAUDE),
        ("agg-gemini", AGGREGATE_API_PROVIDER_GEMINI),
    ] {
        storage
            .insert_aggregate_api(&AggregateApi {
                id: id.to_string(),
                provider_type: provider_type.to_string(),
                supplier_name: Some(id.to_string()),
                sort: 0,
                url: format!("https://{id}.example.com"),
                auth_type: AGGREGATE_API_AUTH_APIKEY.to_string(),
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
                upstream_protocol: None,
            })
            .expect("insert aggregate api");
    }

    let candidates = resolve_aggregate_api_rotation_candidates(&storage, "gemini_native", None)
        .expect("resolve gemini candidates");
    let candidate_ids = candidates
        .iter()
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(candidate_ids, vec!["agg-gemini"]);
}

#[test]
fn zero_balance_blocked_candidates_are_excluded_without_reordering() {
    let mut first = aggregate_api_with_action(None);
    first.id = "agg-blocked".to_string();
    first.sort = 0;
    let mut second = aggregate_api_with_action(None);
    second.id = "agg-available".to_string();
    second.sort = 10;
    let blocked_ids = std::collections::HashSet::from([first.id.clone()]);

    let candidates = super::filter_zero_balance_blocked_candidates(
        vec![first, second],
        &blocked_ids,
        "test-trace",
    );

    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.id.as_str())
            .collect::<Vec<_>>(),
        vec!["agg-available"]
    );
}

#[test]
fn compatible_candidate_resolves_for_codex_and_anthropic_without_protocol_bridge() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let now = now_ts();
    let mut compatible = aggregate_api_with_action(None);
    compatible.id = "agg-compatible".to_string();
    compatible.provider_type = AGGREGATE_API_PROVIDER_COMPATIBLE.to_string();
    compatible.url = "https://multi-protocol.example.com".to_string();
    compatible.created_at = now;
    compatible.updated_at = now;
    storage
        .insert_aggregate_api(&compatible)
        .expect("insert compatible aggregate api");

    for protocol_type in ["openai", "anthropic_native"] {
        let candidates = resolve_aggregate_api_rotation_candidates(&storage, protocol_type, None)
            .expect("resolve compatible candidate");
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.id.as_str())
                .collect::<Vec<_>>(),
            vec!["agg-compatible"]
        );
    }
    assert!(resolve_aggregate_api_rotation_candidates(&storage, "gemini_native", None).is_err());
    assert!(!should_bridge_responses_to_anthropic(
        &compatible,
        "/v1/responses"
    ));
}

#[test]
fn explicit_aggregate_api_id_promotes_matching_active_provider_candidate_only() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let now = now_ts();
    for (id, provider_type, sort) in [
        ("agg-first", AGGREGATE_API_PROVIDER_CODEX, 0),
        ("agg-preferred", AGGREGATE_API_PROVIDER_CODEX, 10),
        ("agg-claude", AGGREGATE_API_PROVIDER_CLAUDE, -1),
    ] {
        storage
            .insert_aggregate_api(&AggregateApi {
                id: id.to_string(),
                provider_type: provider_type.to_string(),
                supplier_name: Some(id.to_string()),
                sort,
                url: format!("https://{id}.example.com"),
                auth_type: AGGREGATE_API_AUTH_APIKEY.to_string(),
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
                upstream_protocol: None,
            })
            .expect("insert aggregate api");
    }

    let candidates =
        resolve_aggregate_api_rotation_candidates(&storage, "openai", Some("agg-preferred"))
            .expect("resolve codex candidates");
    let candidate_ids = candidates
        .iter()
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(candidate_ids, vec!["agg-preferred", "agg-first"]);

    let candidates =
        resolve_aggregate_api_rotation_candidates(&storage, "openai", Some("agg-claude"))
            .expect("resolve codex candidates with mismatched preferred");
    let candidate_ids = candidates
        .iter()
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(candidate_ids, vec!["agg-first", "agg-preferred"]);
}

/// 函数 `final_error_promotes_success_status_to_bad_gateway`
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
fn final_error_promotes_success_status_to_bad_gateway() {
    let status_code = bridge_status_code(Some(200), true, Some("unsupported model"));
    assert_eq!(status_code, 502);
}

/// 函数 `successful_bridge_keeps_success_status`
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
fn successful_bridge_keeps_success_status() {
    let status_code = bridge_status_code(Some(200), true, None);
    assert_eq!(status_code, 200);
}

/// 函数 `incomplete_bridge_without_status_defaults_to_bad_gateway`
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
fn incomplete_bridge_without_status_defaults_to_bad_gateway() {
    let status_code = bridge_status_code(None, false, None);
    assert_eq!(status_code, 502);
}

#[test]
fn aggregate_continuation_recovery_rebuilds_each_retry_from_the_original_candidate_body() {
    let original = br#"{
        "stream": false,
        "previous_response_id": "resp_stale",
        "input": [{"role":"user","content":"hello"}],
        "include": ["reasoning.encrypted_content"]
    }"#;

    let first = build_continuation_recovery_body(original, "continue safely")
        .expect("build first continuation body");
    let second = build_continuation_recovery_body(original, "continue safely")
        .expect("rebuild second continuation body from original body");
    let first: serde_json::Value = serde_json::from_slice(&first).expect("parse first body");
    let second: serde_json::Value = serde_json::from_slice(&second).expect("parse second body");

    assert_eq!(
        first, second,
        "continuation retries must not accumulate markers"
    );
    assert!(first.get("previous_response_id").is_none());
    assert_eq!(
        first["input"]
            .as_array()
            .expect("sanitized input")
            .iter()
            .filter(|item| item["phase"] == "commentary")
            .count(),
        1,
        "each continuation request contains exactly one commentary marker"
    );
}

/// 函数 `bridge_status_code`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - delivered_status_code: 参数 delivered_status_code
/// - bridge_ok: 参数 bridge_ok
/// - final_error: 参数 final_error
///
/// # 返回
/// 返回函数执行结果
fn bridge_status_code(
    delivered_status_code: Option<u16>,
    bridge_ok: bool,
    final_error: Option<&str>,
) -> u16 {
    let status_code = delivered_status_code.unwrap_or_else(|| if bridge_ok { 200 } else { 502 });
    if final_error.is_some() && status_code < 400 {
        502
    } else {
        status_code
    }
}

const CAPACITY_ERROR_JSON: &str = r#"{"type":"error","error":{"message":"Selected model is at capacity. Please try a different model","code":"capacity"}}"#;
const CAPACITY_ERROR_PLAIN: &str = "Selected model is at capacity. Please try a different model";
const CAPACITY_OK_JSON: &str =
    r#"{"id":"resp_agg_ok","object":"response","status":"completed","output":[]}"#;

/// 构造指向 mock 上游的聚合 API 候选。
fn aggregate_capacity_test_candidate(id: &str, url: &str) -> AggregateApi {
    let now = now_ts();
    AggregateApi {
        id: id.to_string(),
        provider_type: AGGREGATE_API_PROVIDER_CODEX.to_string(),
        supplier_name: Some(format!("supplier-{id}")),
        sort: 0,
        url: url.to_string(),
        auth_type: AGGREGATE_API_AUTH_APIKEY.to_string(),
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
        upstream_protocol: None,
    }
}

/// 运行一次聚合 API 代理场景：mock 上游按 `responses` 顺序应答，
/// 返回 (storage, outcome, 上游实际请求数)。
#[allow(clippy::type_complexity)]
fn run_aggregate_capacity_scenario(
    test_name: &str,
    responses: Vec<(u16, &'static str, Option<&'static str>)>,
    candidates: Vec<AggregateApi>,
    allow_model_fallback: bool,
    deadline_after_secs: Option<u64>,
) -> (Storage, AggregateProxyOutcome, usize) {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let server = Server::http("127.0.0.1:0").expect("start server");
    let addr = format!("http://{}", server.server_addr());
    let candidate_url = format!("{addr}/v1/responses");
    let mut candidates = candidates;
    for candidate in &mut candidates {
        candidate.url = candidate_url.clone();
    }
    for candidate in &candidates {
        storage
            .insert_aggregate_api(candidate)
            .expect("insert aggregate api");
        storage
            .upsert_aggregate_api_secret(candidate.id.as_str(), "test-secret")
            .expect("insert aggregate api secret");
    }

    let hit_count = Arc::new(AtomicUsize::new(0));
    let hit_count_thread = Arc::clone(&hit_count);
    let join = thread::spawn(move || {
        for (status, body, retry_after) in responses {
            let mut request = server
                .recv_timeout(Duration::from_secs(2))
                .expect("receive aggregate upstream request")
                .expect("aggregate upstream request present");
            hit_count_thread.fetch_add(1, Ordering::SeqCst);
            let mut response =
                Response::from_string(body.to_string()).with_status_code(StatusCode(status));
            if let Some(retry_after) = retry_after {
                response = response.with_header(
                    tiny_http::Header::from_bytes("Retry-After", retry_after)
                        .expect("retry-after header"),
                );
            }
            request
                .respond(response)
                .expect("respond aggregate request");
        }
    });

    let request: tiny_http::Request = tiny_http::TestRequest::new()
        .with_method(tiny_http::Method::Post)
        .with_path("/v1/responses")
        .into();
    let body = Bytes::from_static(br#"{"model":"gpt-5.4","input":"hello","stream":false}"#);
    let trace_id = format!("trc-{test_name}");
    let key_id = format!("key-{test_name}");
    let path = "/v1/responses".to_string();
    let request_method = "POST".to_string();
    let method = reqwest::Method::POST;
    let started_at = Instant::now();
    // 相对 deadline 在发起代理调用前一刻计算，避免测试套件并行负载下
    // 的 setUp 耗时把绝对 deadline 提前耗尽（flaky）。
    let request_deadline = deadline_after_secs.map(|secs| Instant::now() + Duration::from_secs(secs));
    let outcome = proxy_aggregate_request(AggregateProxyRequest {
        request,
        storage: &storage,
        trace_id: &trace_id,
        key_id: &key_id,
        original_path: &path,
        path: &path,
        request_method: &request_method,
        method: &method,
        body: &body,
        is_stream: false,
        response_adapter: ResponseAdapter::Passthrough,
        tool_name_restore_map: &BTreeMap::new(),
        gateway_mode_for_log: None,
        route_strategy_for_log: Some("account_rotation"),
        route_source_for_log: Some("test"),
        client_model_for_log: None,
        model_for_log: None,
        model_source_for_log: None,
        client_reasoning_for_log: None,
        reasoning_for_log: None,
        reasoning_source_for_log: None,
        service_tier_for_log: None,
        effective_service_tier_for_log: None,
        service_tier_source_for_log: None,
        session_id_for_log: None,
        conversation_anchor_for_log: None,
        aggregate_api_candidates: candidates,
        allow_model_fallback,
        request_deadline,
        started_at,
    })
    .expect("proxy aggregate request");

    join.join().expect("join aggregate upstream server");
    let request_count = hit_count.load(Ordering::SeqCst);
    (storage, outcome, request_count)
}

fn terminal_request_log_status(storage: &Storage, trace_id: &str) -> Option<i64> {
    let logs = storage
        .list_request_logs(None, 10)
        .expect("list request logs");
    logs.iter()
        .find(|log| log.trace_id.as_deref() == Some(trace_id))
        .map(|log| log.status_code)
        .flatten()
}

/// 429 容量错误：同候选重放两次后以 502 终态结束（客户端不再无限重试）。
#[test]
fn aggregate_api_capacity_429_replays_twice_then_terminates_502() {
    let (storage, outcome, request_count) = run_aggregate_capacity_scenario(
        "capacity-429",
        vec![
            (429, CAPACITY_ERROR_JSON, Some("0")),
            (429, CAPACITY_ERROR_JSON, Some("0")),
            (429, CAPACITY_ERROR_JSON, Some("0")),
        ],
        vec![aggregate_capacity_test_candidate("agg-capacity-429", "")],
        false,
        None,
    );
    assert_eq!(
        request_count, 3,
        "initial request + two same-candidate replays"
    );
    assert!(matches!(outcome, AggregateProxyOutcome::Handled));
    assert_eq!(
        terminal_request_log_status(&storage, "trc-capacity-429"),
        Some(502),
        "client-visible terminal status must be 502, not 429"
    );
}

/// 503 纯文本容量错误：同样重放两次后以 502 终态结束。
#[test]
fn aggregate_api_capacity_503_plain_text_replays_twice_then_terminates_502() {
    let (storage, outcome, request_count) = run_aggregate_capacity_scenario(
        "capacity-503",
        vec![
            (503, CAPACITY_ERROR_PLAIN, Some("0")),
            (503, CAPACITY_ERROR_PLAIN, Some("0")),
            (503, CAPACITY_ERROR_PLAIN, Some("0")),
        ],
        vec![aggregate_capacity_test_candidate("agg-capacity-503", "")],
        false,
        None,
    );
    assert_eq!(
        request_count, 3,
        "initial request + two same-candidate replays"
    );
    assert!(matches!(outcome, AggregateProxyOutcome::Handled));
    assert_eq!(
        terminal_request_log_status(&storage, "trc-capacity-503"),
        Some(502)
    );
}

/// 上游 Retry-After 超过 request deadline 时，等待被拒绝并立即终态结束；
/// 网关本地超时收敛为 502（与聚合 API 失败语义一致，避免客户端对 504 无限重试）。
#[test]
fn aggregate_api_capacity_honors_upstream_retry_after_but_not_past_deadline() {
    let (storage, outcome, request_count) = run_aggregate_capacity_scenario(
        "capacity-deadline",
        vec![(429, CAPACITY_ERROR_JSON, Some("2"))],
        vec![aggregate_capacity_test_candidate(
            "agg-capacity-deadline",
            "",
        )],
        false,
        Some(1),
    );
    assert_eq!(
        request_count, 1,
        "waiting 2s past a 1s deadline must not replay"
    );
    assert!(matches!(outcome, AggregateProxyOutcome::Handled));
    assert_eq!(
        terminal_request_log_status(&storage, "trc-capacity-deadline"),
        Some(502),
        "deadline takes priority over Retry-After; client sees 502, not 504"
    );
}

/// 重放恢复：429、429 后第三次请求成功，客户端收到上游成功响应。
#[test]
fn aggregate_api_capacity_recovers_on_second_replay() {
    let (storage, outcome, request_count) = run_aggregate_capacity_scenario(
        "capacity-recover",
        vec![
            (429, CAPACITY_ERROR_JSON, Some("0")),
            (429, CAPACITY_ERROR_JSON, Some("0")),
            (200, CAPACITY_OK_JSON, None),
        ],
        vec![aggregate_capacity_test_candidate(
            "agg-capacity-recover",
            "",
        )],
        false,
        None,
    );
    assert_eq!(
        request_count, 3,
        "two capacity errors then a successful replay"
    );
    assert!(matches!(outcome, AggregateProxyOutcome::Handled));
    assert_eq!(
        terminal_request_log_status(&storage, "trc-capacity-recover"),
        Some(200)
    );
}

/// 无可用候选：不发起任何上游请求，直接以 502 终态结束。
#[test]
fn aggregate_api_empty_candidates_terminate_502_without_upstream_traffic() {
    let (storage, outcome, request_count) =
        run_aggregate_capacity_scenario("capacity-empty", vec![], vec![], false, None);
    assert_eq!(request_count, 0, "no candidate means no upstream traffic");
    assert!(matches!(outcome, AggregateProxyOutcome::Handled));
    assert!(terminal_request_log_status(&storage, "trc-capacity-empty").is_none());
}

// ---------------------------------------------------------------------------
// Phase 2/3 集成：chat_completions 上游走真实 mock 服务器。
// ---------------------------------------------------------------------------

/// mock 上游收到的一次请求记录。
#[derive(Debug, Clone)]
struct MockUpstreamHit {
    path: String,
    body: String,
}

const CHAT_OK_JSON: &str = r#"{
  "id": "chatcmpl-mock-1",
  "object": "chat.completion",
  "created": 1710000000,
  "model": "gpt-4o-mini",
  "choices": [
    {
      "index": 0,
      "message": { "role": "assistant", "content": "hi from chat" },
      "finish_reason": "stop"
    }
  ],
  "usage": { "prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7 }
}"#;

const ANTHROPIC_OK_JSON: &str = r#"{
  "id": "msg_mock_1",
  "type": "message",
  "role": "assistant",
  "model": "claude-3-5-sonnet",
  "content": [{ "type": "text", "text": "hi from anthropic" }],
  "stop_reason": "end_turn",
  "usage": { "input_tokens": 5, "output_tokens": 2 }
}"#;

/// 运行一次 chat/anthropic 上游聚合场景：每个候选使用自己的 base URL，
/// mock 服务器按 `responses` 顺序应答并记录 (path, body)。
fn run_upstream_scenario(
    test_name: &str,
    candidates: Vec<AggregateApi>,
    responses: Vec<(u16, &'static str)>,
    client_body: &'static [u8],
    allow_model_fallback: bool,
) -> (Storage, AggregateProxyOutcome, Vec<MockUpstreamHit>) {
    run_upstream_scenario_with_stream(
        test_name,
        candidates,
        responses,
        client_body,
        allow_model_fallback,
        false,
        true,
    )
}
fn run_upstream_scenario_with_stream(
    test_name: &str,
    candidates: Vec<AggregateApi>,
    responses: Vec<(u16, &'static str)>,
    client_body: &'static [u8],
    allow_model_fallback: bool,
    is_stream: bool,
    capture_body: bool,
) -> (Storage, AggregateProxyOutcome, Vec<MockUpstreamHit>) {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let server = Server::http("127.0.0.1:0").expect("start server");
    let addr = format!("http://{}", server.server_addr());
    let mut candidates = candidates;
    for candidate in &mut candidates {
        candidate.url = format!("{addr}{}", candidate.url);
    }
    for candidate in &candidates {
        storage
            .insert_aggregate_api(candidate)
            .expect("insert aggregate api");
        storage
            .upsert_aggregate_api_secret(candidate.id.as_str(), "test-secret")
            .expect("insert aggregate api secret");
    }

    let hits = Arc::new(std::sync::Mutex::new(Vec::new()));
    let hits_thread = Arc::clone(&hits);
    let join = thread::spawn(move || {
        for (status, body) in responses {
            let Some(mut request) = server
                .recv_timeout(Duration::from_millis(250))
                .expect("receive aggregate upstream request")
            else {
                break;
            };
            let body_text = if capture_body {
                let mut body_bytes = Vec::with_capacity(1024);
                let _ = request
                    .as_reader()
                    .take(16 * 1024)
                    .read_to_end(&mut body_bytes);
                String::from_utf8_lossy(&body_bytes).to_string()
            } else {
                String::new()
            };
            hits_thread.lock().expect("hits lock").push(MockUpstreamHit {
                path: request.url().to_string(),
                body: body_text,
            });
            let mut response =
                Response::from_string(body.to_string()).with_status_code(StatusCode(status));
            if is_stream {
                response = response.with_header(
                    tiny_http::Header::from_bytes("Content-Type", "text/event-stream")
                        .expect("content-type header"),
                );
            }
            request
                .respond(response)
                .expect("respond aggregate request");
        }
    });

    let request: tiny_http::Request = tiny_http::TestRequest::new()
        .with_method(tiny_http::Method::Post)
        .with_path("/v1/responses")
        .into();
    let body = Bytes::from_static(client_body);
    let trace_id = format!("trc-{test_name}");
    let key_id = format!("key-{test_name}");
    let path = "/v1/responses".to_string();
    let request_method = "POST".to_string();
    let method = reqwest::Method::POST;
    let started_at = Instant::now();
    let outcome = proxy_aggregate_request(AggregateProxyRequest {
        request,
        storage: &storage,
        trace_id: &trace_id,
        key_id: &key_id,
        original_path: &path,
        path: &path,
        request_method: &request_method,
        method: &method,
        body: &body,
        is_stream,
        response_adapter: ResponseAdapter::Passthrough,
        tool_name_restore_map: &BTreeMap::new(),
        gateway_mode_for_log: None,
        route_strategy_for_log: Some("account_rotation"),
        route_source_for_log: Some("test"),
        client_model_for_log: None,
        model_for_log: None,
        model_source_for_log: None,
        client_reasoning_for_log: None,
        reasoning_for_log: None,
        reasoning_source_for_log: None,
        service_tier_for_log: None,
        effective_service_tier_for_log: None,
        service_tier_source_for_log: None,
        session_id_for_log: None,
        conversation_anchor_for_log: None,
        aggregate_api_candidates: candidates,
        allow_model_fallback,
        request_deadline: None,
        started_at,
    })
    .expect("proxy aggregate request");

    join.join().expect("join aggregate upstream server");
    let collected = hits.lock().expect("hits lock").clone();
    (storage, outcome, collected)
}

const RESPONSES_STREAM_OK_SSE: &str = concat!(
    "event: response.output_text.delta\n",
    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n",
    "event: response.completed\n",
    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp-stream-ok\",\"model\":\"gpt-5.4\",\"output\":[]}}\n\n",
    "data: [DONE]\n\n"
);

const EMPTY_STREAM_BODY: &str = "";

fn chat_test_candidate(id: &str, base_url: &str) -> AggregateApi {
    let mut candidate = aggregate_capacity_test_candidate(id, base_url);
    candidate.upstream_protocol = Some("chat_completions".to_string());
    candidate
}

fn request_log_for(storage: &Storage, trace_id: &str) -> codexmanager_core::storage::RequestLog {
    storage
        .list_request_logs(None, 10)
        .expect("list request logs")
        .into_iter()
        .find(|log| log.trace_id.as_deref() == Some(trace_id))
        .expect("request log present")
}

/// 默认动作路径：chat 候选（无自定义 action）应命中 `/v1/chat/completions`，
/// 请求体为 Chat 形状，响应经本地转换后以 200 记日志。
#[test]
fn chat_upstream_default_action_path_serves_responses_request() {
    let (storage, outcome, hits) = run_upstream_scenario(
        "chat-default-action",
        vec![chat_test_candidate("agg-chat-default", "")],
        vec![(200, CHAT_OK_JSON)],
        br#"{"model":"gpt-5.4","input":"hello","stream":false}"#,
        false,
    );
    assert!(matches!(outcome, AggregateProxyOutcome::Handled));
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].path, "/v1/chat/completions",
        "default action derives /v1/chat/completions for a bare host"
    );
    let chat_body: Value = serde_json::from_str(&hits[0].body).expect("chat body json");
    assert_eq!(chat_body["model"], "gpt-5.4");
    assert!(chat_body.get("messages").is_some(), "input mapped to messages");
    assert!(chat_body.get("input").is_none(), "no raw responses input");
    assert_eq!(chat_body["stream"], false);

    let log = request_log_for(&storage, "trc-chat-default-action");
    assert_eq!(log.status_code, Some(200));
    assert_eq!(
        log.upstream_protocol.as_deref(),
        Some("chat_completions"),
        "request log persists the declared upstream protocol"
    );
    assert_eq!(log.input_tokens, Some(5));
    assert_eq!(log.output_tokens, Some(2));
}

/// 自定义 action：chat 候选显式 action 优先于默认路径推导。
#[test]
fn chat_upstream_custom_action_path_is_used() {
    let (_, outcome, hits) = run_upstream_scenario(
        "chat-custom-action",
        vec![{
            let mut candidate = chat_test_candidate("agg-chat-custom", "");
            candidate.action = Some("/v1/custom/chat".to_string());
            candidate
        }],
        vec![(200, CHAT_OK_JSON)],
        br#"{"model":"gpt-5.4","input":"hello","stream":false}"#,
        false,
    );
    assert!(matches!(outcome, AggregateProxyOutcome::Handled));
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "/v1/custom/chat");
}

/// 本地不兼容（design §10：健康中性跳过）：chat 候选无法表示 `store` 语义时
/// 跳到下一个 Responses 候选，后者正常承接请求。
#[test]
fn chat_upstream_incompatible_skips_to_responses_candidate() {
    let (storage, outcome, hits) = run_upstream_scenario(
        "chat-incompatible-skip",
        vec![
            chat_test_candidate("agg-chat-incompatible", ""),
            {
                let mut candidate =
                    aggregate_capacity_test_candidate("agg-responses", "");
                candidate.upstream_protocol = Some("responses".to_string());
                candidate
            },
        ],
        vec![(200, CAPACITY_OK_JSON)],
        br#"{"model":"gpt-5.4","input":"hello","store":true,"stream":false}"#,
        false,
    );
    assert!(matches!(outcome, AggregateProxyOutcome::Handled));
    assert_eq!(
        hits.len(),
        1,
        "incompatible chat candidate must not send an upstream request"
    );
    assert_eq!(hits[0].path, "/v1/responses");
    let log = request_log_for(&storage, "trc-chat-incompatible-skip");
    assert_eq!(log.status_code, Some(200));
    assert_eq!(
        log.upstream_protocol.as_deref(),
        Some("responses"),
        "terminal log reflects the serving candidate"
    );
}

/// 全部候选均为 chat 且请求对 chat 不可表示：不发起上游流量，
/// 以 502 终态返回 Unavailable，供外层模型降级跳板先行消费（fallback-first）。
#[test]
fn chat_upstream_all_incompatible_returns_unavailable_for_fallback() {
    let (_storage, outcome, hits) = run_upstream_scenario(
        "chat-all-incompatible",
        vec![chat_test_candidate("agg-chat-only", "")],
        vec![],
        br#"{"model":"gpt-5.4","input":"hello","store":true,"stream":false}"#,
        true,
    );
    assert!(hits.is_empty(), "no upstream traffic on local incompatibility");
    match outcome {
        AggregateProxyOutcome::Unavailable {
            status_code, message, ..
        } => {
            assert_eq!(
                status_code, 502,
                "502 >= 500 keeps the model-fallback signal available"
            );
            assert!(
                message.contains("incompatible"),
                "error message names the local incompatibility: {message}"
            );
        }
        _ => panic!("expected Unavailable for model fallback"),
    }
}

/// 模型降级未启用时，本地 Chat 不兼容走最终 502，并保留最后候选协议标签。
#[test]
fn chat_upstream_terminal_failure_logs_protocol() {
    let (storage, outcome, hits) = run_upstream_scenario(
        "chat-terminal-protocol",
        vec![chat_test_candidate("agg-chat-terminal", "")],
        vec![],
        br#"{"model":"gpt-5.4","input":"hello","store":true,"stream":false}"#,
        false,
    );
    assert!(matches!(outcome, AggregateProxyOutcome::Handled));
    assert!(hits.is_empty(), "local incompatibility must not reach upstream");
    let log = request_log_for(&storage, "trc-chat-terminal-protocol");
    assert_eq!(log.status_code, Some(502));
    assert_eq!(log.upstream_protocol.as_deref(), Some("chat_completions"));
}

/// 旧行为保留：claude 候选 + NULL upstream_protocol 走 Anthropic 桥，
/// 命中 `/v1/messages` 且请求日志标记 `anthropic_messages`。
#[test]
fn claude_bridge_legacy_null_protocol_logs_anthropic_messages() {
    let mut candidate = aggregate_capacity_test_candidate("agg-claude-bridge", "");
    candidate.provider_type = AGGREGATE_API_PROVIDER_CLAUDE.to_string();
    let (storage, outcome, hits) = run_upstream_scenario(
        "claude-bridge",
        vec![candidate],
        vec![(200, ANTHROPIC_OK_JSON)],
        br#"{"model":"claude-3-5-sonnet","input":"hello","stream":false}"#,
        false,
    );
    assert!(matches!(outcome, AggregateProxyOutcome::Handled));
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].path, "/v1/messages",
        "legacy NULL protocol keeps the Anthropic bridge action path"
    );
    let log = request_log_for(&storage, "trc-claude-bridge");
    assert_eq!(log.status_code, Some(200));
    assert_eq!(
        log.upstream_protocol.as_deref(),
        Some("anthropic_messages")
    );
    assert_eq!(log.input_tokens, Some(5));
    assert_eq!(log.output_tokens, Some(2));
}

// ---------------------------------------------------------------------------
/// 交付前空 Chat 流必须把控制权交给后续候选；测试响应保持极小，避免构造大缓冲。
#[test]
fn chat_preflight_empty_stream_fails_over_to_later_candidate() {
    let mut empty_chat = chat_test_candidate("agg-chat-empty", "");
    empty_chat.sort = -1;
    let mut responses = aggregate_capacity_test_candidate("agg-responses-fallback", "");
    responses.upstream_protocol = Some("responses".to_string());
    let (storage, outcome, hits) = run_upstream_scenario_with_stream(
        "chat-empty-stream-failover",
        vec![empty_chat, responses],
        vec![(200, EMPTY_STREAM_BODY), (200, RESPONSES_STREAM_OK_SSE)],
        br#"{"model":"gpt-5.4","input":"hello","stream":true}"#,
        false,
        true,
        false,
    );

    assert!(matches!(outcome, AggregateProxyOutcome::Handled));
    assert_eq!(hits.len(), 2, "empty preflight stream must try the fallback");
    assert_eq!(hits[0].path, "/v1/chat/completions");
    assert_eq!(hits[1].path, "/v1/responses");
    let log = request_log_for(&storage, "trc-chat-empty-stream-failover");
    assert_eq!(log.status_code, Some(200));
    assert_eq!(log.upstream_protocol.as_deref(), Some("responses"));
    let first_health = storage
        .aggregate_api_health_state("agg-chat-empty", Some("gpt-5.4"), None)
        .expect("read first candidate health")
        .expect("first candidate health exists");
    assert_eq!(first_health.consecutive_failures, 1);
}

#[test]
fn chat_preflight_semantic_delta_does_not_replay_to_later_candidate() {
    let mut first_chat = chat_test_candidate("agg-chat-semantic", "");
    first_chat.sort = -1;
    let fallback = aggregate_capacity_test_candidate("agg-responses-never-used", "");
    let semantic_chat = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"},\"finish_reason\":null}]}\n\n",
        "data: [DONE]\n\n"
    );
    let (_storage, outcome, hits) = run_upstream_scenario_with_stream(
        "chat-semantic-no-replay",
        vec![first_chat, fallback],
        vec![(200, semantic_chat), (200, CAPACITY_OK_JSON)],
        br#"{"model":"gpt-5.4","input":"hello","stream":true}"#,
        false,
        true,
        false,
    );

    assert!(matches!(outcome, AggregateProxyOutcome::Handled));
    assert_eq!(hits.len(), 1, "semantic Chat output must prevent replay");
    assert_eq!(hits[0].path, "/v1/chat/completions");
}

// Chat 流式 preflight 分类器：交付前 failover 边界（review gate）。

#[test]
fn chat_nonstream_malformed_response_fails_over_to_later_candidate() {
    let mut malformed_chat = chat_test_candidate("agg-chat-malformed", "");
    malformed_chat.sort = -1;
    let mut responses = aggregate_capacity_test_candidate("agg-responses-after-malformed", "");
    responses.upstream_protocol = Some("responses".to_string());
    let (storage, outcome, hits) = run_upstream_scenario_with_stream(
        "chat-malformed-response-failover",
        vec![malformed_chat, responses],
        vec![(200, "not-json"), (200, CAPACITY_OK_JSON)],
        br#"{"model":"gpt-5.4","input":"hello","stream":false}"#,
        false,
        false,
        false,
    );

    assert!(matches!(outcome, AggregateProxyOutcome::Handled));
    assert_eq!(hits.len(), 2, "malformed Chat response must try the fallback");
    assert_eq!(hits[0].path, "/v1/chat/completions");
    assert_eq!(hits[1].path, "/v1/responses");
    let log = request_log_for(&storage, "trc-chat-malformed-response-failover");
    assert_eq!(log.status_code, Some(200));
    assert_eq!(log.upstream_protocol.as_deref(), Some("responses"));
}

#[test]
fn chat_preflight_delivers_on_semantic_delta_and_done() {
    use super::classify_chat_preflight_prefix;
    use super::ChatPrefixDecision;
    let delta =
        b"data: {\"choices\":[{\"delta\":{\"content\":\"hel\"},\"finish_reason\":null}]}\n\n";
    assert!(matches!(
        classify_chat_preflight_prefix(delta),
        ChatPrefixDecision::Deliver
    ));
    assert!(matches!(
        classify_chat_preflight_prefix(b"data: [DONE]\n\n"),
        ChatPrefixDecision::Deliver
    ));
}

#[test]
fn chat_preflight_fails_over_on_error_and_content_filter() {
    use super::classify_chat_preflight_prefix;
    use super::ChatPrefixDecision;
    let error = b"data: {\"error\":{\"message\":\"boom\"}}\n\n";
    match classify_chat_preflight_prefix(error) {
        ChatPrefixDecision::Failover(_) => {}
        ChatPrefixDecision::Deliver => panic!("expected failover for error frame, got Deliver"),
        ChatPrefixDecision::NeedMore => {
            panic!("expected failover for error frame, got NeedMore")
        }
    }
    let filtered = b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"content_filter\"}]}\n\n";
    match classify_chat_preflight_prefix(filtered) {
        ChatPrefixDecision::Failover(_) => {}
        ChatPrefixDecision::Deliver => {
            panic!("expected failover for content_filter, got Deliver")
        }
        ChatPrefixDecision::NeedMore => {
            panic!("expected failover for content_filter, got NeedMore")
        }
    }
}

#[test]
fn chat_preflight_needs_more_on_metadata_and_truncated_frames() {
    use super::classify_chat_preflight_prefix;
    use super::ChatPrefixDecision;
    assert!(matches!(
        classify_chat_preflight_prefix(b""),
        ChatPrefixDecision::NeedMore
    ));
    assert!(matches!(
        classify_chat_preflight_prefix(b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":null}]}\n\n"),
        ChatPrefixDecision::NeedMore
    ));
    assert!(matches!(
        classify_chat_preflight_prefix(b"data: {\"choices\":[{\"delta\":{\"content\":\"hel\""),
        ChatPrefixDecision::NeedMore
    ));
}
