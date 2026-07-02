use super::*;
use codexmanager_core::storage::UsageSnapshotRecord;

fn insert_reasoning_guard_test_account(storage: &Storage, id: &str, sort: i64, now: i64) {
    storage
        .insert_account(&Account {
            id: id.to_string(),
            label: id.to_string(),
            issuer: "https://auth.openai.com".to_string(),
            chatgpt_account_id: Some(format!("chatgpt_{id}")),
            workspace_id: None,
            group_name: None,
            sort,
            status: "active".to_string(),
            created_at: now + sort,
            updated_at: now + sort,
        })
        .expect("insert account");
    storage
        .insert_token(&Token {
            account_id: id.to_string(),
            id_token: String::new(),
            access_token: format!("access_{id}"),
            refresh_token: String::new(),
            api_key_access_token: Some(format!("api_access_{id}")),
            last_refresh: now,
        })
        .expect("insert token");
    storage
        .insert_usage_snapshot(&UsageSnapshotRecord {
            account_id: id.to_string(),
            used_percent: Some(10.0),
            window_minutes: Some(300),
            resets_at: None,
            secondary_used_percent: None,
            secondary_window_minutes: None,
            secondary_resets_at: None,
            credits_json: None,
            captured_at: now,
        })
        .expect("insert snapshot");
}

fn insert_reasoning_guard_test_key(storage: &Storage, key_id: &str, platform_key: &str, now: i64) {
    storage
        .insert_api_key(&ApiKey {
            id: key_id.to_string(),
            name: Some(key_id.to_string()),
            model_slug: Some("gpt-5.3-codex".to_string()),
            reasoning_effort: None,
            service_tier: None,
            rotation_strategy: "account_rotation".to_string(),
            aggregate_api_id: None,
            account_plan_filter: None,
            aggregate_api_url: None,
            client_type: "codex".to_string(),
            protocol_type: "openai_compat".to_string(),
            auth_scheme: "authorization_bearer".to_string(),
            upstream_base_url: None,
            static_headers_json: None,
            key_hash: hash_platform_key_for_test(platform_key),
            status: "active".to_string(),
            created_at: now,
            last_used_at: None,
        })
        .expect("insert api key");
}

fn guard_non_stream_response(id: &str) -> String {
    serde_json::json!({
        "id": id,
        "model": "gpt-5.3-codex",
        "output": [{
            "type": "message",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": "guarded output" }]
        }],
        "usage": {
            "input_tokens": 3,
            "output_tokens": 1,
            "total_tokens": 4,
            "output_tokens_details": { "reasoning_tokens": 516 }
        }
    })
    .to_string()
}

fn stream_response_with_reasoning_tokens(reasoning_tokens: i64, delta: &str) -> String {
    format!(
        "data: {{\"type\":\"response.output_text.delta\",\"delta\":\"{delta}\"}}\n\n\
         event: response.completed\n\
         data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"resp_stream\",\"model\":\"gpt-5.3-codex\",\"usage\":{{\"input_tokens\":3,\"output_tokens\":1,\"total_tokens\":4,\"output_tokens_details\":{{\"reasoning_tokens\":{reasoning_tokens}}}}}}}}}\n\n\
         data: [DONE]\n\n"
    )
}

fn reasoning_guard_test_env(
    enabled: bool,
    retry_attempts: usize,
    bypass_after_consecutive: usize,
) -> Vec<EnvGuard> {
    let retry_attempts = retry_attempts.to_string();
    let bypass_after_consecutive = bypass_after_consecutive.to_string();
    vec![
        EnvGuard::set(
            "CODEXMANAGER_REASONING_GUARD_ENABLED",
            if enabled { "1" } else { "0" },
        ),
        EnvGuard::set("CODEXMANAGER_REASONING_GUARD_TARGETS", "516,1034,1552"),
        EnvGuard::set("CODEXMANAGER_REASONING_GUARD_INTERCEPT_STREAMING", "1"),
        EnvGuard::set("CODEXMANAGER_REASONING_GUARD_INTERCEPT_NON_STREAMING", "1"),
        EnvGuard::set(
            "CODEXMANAGER_REASONING_GUARD_RETRY_ATTEMPTS",
            retry_attempts.as_str(),
        ),
        EnvGuard::set(
            "CODEXMANAGER_REASONING_GUARD_BYPASS_AFTER_CONSECUTIVE",
            bypass_after_consecutive.as_str(),
        ),
    ]
}

fn metric_value(metrics: &str, name: &str, labels: &str) -> usize {
    let prefix = if labels.is_empty() {
        format!("{name} ")
    } else {
        format!("{name}{{{labels}}} ")
    };
    metrics
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(0)
}

/// 当上游用 200 + SSE `data:` 正文夹带 "You've hit your usage limit" 回应时，
/// 网关不能在同一次请求里重试（流已经吐给客户端），但必须把该请求内部标记成 failover：
/// - 客户端侧 HTTP status 仍保持 200（原样透传上游响应）
/// - request_log 的 status_code 应为 502（failover 记账，用于观察/冷却）
///
/// 这条链路覆盖：PassthroughSseUsageReader 扫描 data 正文（Fix A）→
/// bridge.stream_terminal_error → response_finalize 的 failover 分支。
#[test]
fn gateway_usage_limit_in_sse_marks_request_as_failover() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-usage-limit-sse-failover");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let usage_limit_sse = concat!(
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"You've hit your usage limit. To get more access now, send a request to your admin or try again at 7:44 PM.\"}\n\n",
        "data: [DONE]\n\n"
    );

    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_sequence_lenient_with_content_types(
            vec![(
                200,
                usage_limit_sse.to_string(),
                "text/event-stream".to_string(),
            )],
            Duration::from_secs(3),
        );
    let upstream_base = format!("http://{upstream_addr}/backend-api/codex");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    seed_model_catalog_models(&storage, &["gpt-5.3-codex"]);
    let now = now_ts();

    // 两个候选账号都健康（10%），以保证 has_more_candidates=true 让
    // should_failover_for_gateway_error 返回 true，走 failover 标记分支。
    for (id, sort) in [("acc_primary", 0_i64), ("acc_secondary", 1_i64)] {
        storage
            .insert_account(&Account {
                id: id.to_string(),
                label: id.to_string(),
                issuer: "https://auth.openai.com".to_string(),
                chatgpt_account_id: Some(format!("chatgpt_{id}")),
                workspace_id: None,
                group_name: None,
                sort,
                status: "active".to_string(),
                created_at: now + sort,
                updated_at: now + sort,
            })
            .expect("insert account");
        storage
            .insert_token(&Token {
                account_id: id.to_string(),
                id_token: String::new(),
                access_token: format!("access_{id}"),
                refresh_token: String::new(),
                api_key_access_token: Some(format!("api_access_{id}")),
                last_refresh: now,
            })
            .expect("insert token");
        storage
            .insert_usage_snapshot(&UsageSnapshotRecord {
                account_id: id.to_string(),
                used_percent: Some(10.0),
                window_minutes: Some(300),
                resets_at: None,
                secondary_used_percent: None,
                secondary_window_minutes: None,
                secondary_resets_at: None,
                credits_json: None,
                captured_at: now,
            })
            .expect("insert snapshot");
    }

    let platform_key = "pk_usage_limit_failover_marker";
    storage
        .insert_api_key(&ApiKey {
            id: "gk_usage_limit_failover_marker".to_string(),
            name: Some("usage-limit-failover-marker".to_string()),
            model_slug: Some("gpt-5.3-codex".to_string()),
            reasoning_effort: None,
            service_tier: None,
            rotation_strategy: "account_rotation".to_string(),
            aggregate_api_id: None,
            account_plan_filter: None,
            aggregate_api_url: None,
            client_type: "codex".to_string(),
            protocol_type: "openai_compat".to_string(),
            auth_scheme: "authorization_bearer".to_string(),
            upstream_base_url: None,
            static_headers_json: None,
            key_hash: hash_platform_key_for_test(platform_key),
            status: "active".to_string(),
            created_at: now,
            last_used_at: None,
        })
        .expect("insert api key");

    let server = codexmanager_service::start_one_shot_server().expect("start server");
    let req_body_json = serde_json::json!({
        "model": "gpt-5.3-codex",
        "input": "hello",
        "stream": true
    });
    let req_body = serde_json::to_string(&req_body_json).expect("serialize request");
    let (status, _body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        &req_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();
    assert_eq!(status, 200, "客户端看到的 HTTP status 应原样透传 200");

    let captured = upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive upstream request");
    upstream_join.join().expect("join mock upstream");
    let auth = captured
        .headers
        .get("authorization")
        .map(String::as_str)
        .unwrap_or_default();
    assert!(
        auth.contains("access_acc_primary"),
        "应命中 sort=0 的 primary 账号，实际 auth 头：{auth}"
    );

    // 等 request log 异步落盘。
    let mut log = None;
    for _ in 0..40 {
        let logs = storage
            .list_request_logs(Some("key:=gk_usage_limit_failover_marker"), 20)
            .expect("list request logs");
        log = logs
            .into_iter()
            .find(|item| item.request_path == "/v1/responses");
        if log.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let log = log.expect("request log should be recorded");
    assert_eq!(
        log.status_code,
        Some(502),
        "usage-limit 在 SSE 正文里时应触发 failover 记账（status_for_log=502），实际 {:?}",
        log.status_code
    );
    assert_eq!(
        log.account_id.as_deref(),
        Some("acc_primary"),
        "failover 记录应记在命中 usage-limit 的 primary 账号下"
    );
}

#[test]
fn gateway_reasoning_guard_non_stream_returns_502_without_next_candidate() {
    let _lock = test_env_guard();
    let _guard_env = reasoning_guard_test_env(true, 0, 0);
    let dir = new_test_dir("codexmanager-gateway-reasoning-guard-non-stream-no-failover");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_sequence_lenient_with_content_types(
            vec![(
                200,
                guard_non_stream_response("resp_guarded"),
                "application/json".to_string(),
            )],
            Duration::from_secs(3),
        );
    let upstream_base = format!("http://{upstream_addr}/backend-api/codex");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    seed_model_catalog_models(&storage, &["gpt-5.3-codex"]);
    let now = now_ts();
    insert_reasoning_guard_test_account(&storage, "acc_guard_primary", 0, now);
    insert_reasoning_guard_test_account(&storage, "acc_guard_secondary", 1, now);
    let platform_key = "pk_reasoning_guard_non_stream";
    insert_reasoning_guard_test_key(&storage, "gk_reasoning_guard_non_stream", platform_key, now);

    let server = codexmanager_service::start_one_shot_server().expect("start server");
    let request_body = r#"{"model":"gpt-5.3-codex","input":"hello","stream":false}"#;
    let (status, gateway_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        request_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();

    assert_eq!(status, 502, "gateway response: {gateway_body}");
    assert!(
        gateway_body.contains("reasoning_tokens=516"),
        "502 should explain the guard trigger: {gateway_body}"
    );
    assert!(
        !gateway_body.contains("guarded output"),
        "guarded first response must not leak to the client: {gateway_body}"
    );

    let first = upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive first upstream request");
    upstream_join.join().expect("join mock upstream");
    assert!(
        first
            .headers
            .get("authorization")
            .is_some_and(|auth| auth.contains("access_acc_guard_primary")),
        "first attempt should use primary account"
    );
    assert!(
        upstream_rx.try_recv().is_err(),
        "516 must not try the next account"
    );

    let primary = storage
        .find_account_by_id("acc_guard_primary")
        .expect("find primary")
        .expect("primary account exists");
    assert_eq!(
        primary.status, "active",
        "reasoning guard must not mark the account unavailable"
    );

    let mut log = None;
    for _ in 0..40 {
        let logs = storage
            .list_request_logs(Some("key:=gk_reasoning_guard_non_stream"), 20)
            .expect("list request logs");
        log = logs
            .into_iter()
            .find(|item| item.request_path == "/v1/responses");
        if log.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let log = log.expect("request log should be recorded");
    assert_eq!(log.status_code, Some(502));
    assert_eq!(log.reasoning_output_tokens, Some(516));
    assert!(
        log.error
            .as_deref()
            .is_some_and(|error| error.contains("reasoning_tokens=516")),
        "request log should classify the converted 502: {:?}",
        log.error
    );
}

#[test]
fn gateway_reasoning_guard_non_stream_retries_same_candidate_before_blocking() {
    let _lock = test_env_guard();
    let _guard_env = reasoning_guard_test_env(true, 1, 0);
    codexmanager_service::set_gateway_background_tasks(
        codexmanager_service::BackgroundTasksInput {
            usage_polling_enabled: Some(false),
            gateway_keepalive_enabled: Some(false),
            token_refresh_polling_enabled: Some(false),
            ..Default::default()
        },
    )
    .expect("disable background tasks for reasoning guard retry test");
    let dir = new_test_dir("codexmanager-gateway-reasoning-guard-internal-retry");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let ok_response = serde_json::json!({
        "id": "resp_retry_ok",
        "model": "gpt-5.3-codex",
        "output": [{
            "type": "message",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": "retry recovered" }]
        }],
        "usage": {
            "input_tokens": 3,
            "output_tokens": 2,
            "total_tokens": 5,
            "output_tokens_details": { "reasoning_tokens": 128 }
        }
    })
    .to_string();

    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_sequence_lenient_with_content_types(
            vec![
                (
                    200,
                    guard_non_stream_response("resp_guard_retry_first")
                        .replace("\"reasoning_tokens\":516", "\"reasoning_tokens\":1034"),
                    "application/json".to_string(),
                ),
                (200, ok_response, "application/json".to_string()),
                (
                    200,
                    r#"{"object":"list","data":[{"id":"gpt-5.3-codex","object":"model"}]}"#
                        .to_string(),
                    "application/json".to_string(),
                ),
            ],
            Duration::from_secs(3),
        );
    let upstream_base = format!("http://{upstream_addr}/backend-api/codex");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    seed_model_catalog_models(&storage, &["gpt-5.3-codex"]);
    let now = now_ts();
    insert_reasoning_guard_test_account(&storage, "acc_guard_retry_primary", 0, now);
    insert_reasoning_guard_test_account(&storage, "acc_guard_retry_secondary", 1, now);
    let platform_key = "pk_reasoning_guard_retry";
    insert_reasoning_guard_test_key(&storage, "gk_reasoning_guard_retry", platform_key, now);

    let server = TestServer::start();
    let (_, metrics_before) = get_http_raw(&server.addr, "/metrics");
    let matches_before = metric_value(
        &metrics_before,
        "codexmanager_gateway_reasoning_guard_matches_total",
        "mode=\"non_stream\"",
    );
    let retries_before = metric_value(
        &metrics_before,
        "codexmanager_gateway_reasoning_guard_internal_retries_total",
        "mode=\"non_stream\"",
    );
    let blocks_before = metric_value(
        &metrics_before,
        "codexmanager_gateway_reasoning_guard_blocks_total",
        "mode=\"non_stream\"",
    );
    let failovers_before = metric_value(
        &metrics_before,
        "codexmanager_gateway_failover_attempts_total",
        "",
    );

    let request_body = r#"{"model":"gpt-5.3-codex","input":"hello","stream":false}"#;
    let (status, gateway_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        request_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );

    assert_eq!(status, 200, "gateway response: {gateway_body}");
    assert!(
        gateway_body.contains("retry recovered"),
        "successful retry should return the second upstream response: {gateway_body}"
    );
    assert!(
        !gateway_body.contains("guarded output") && !gateway_body.contains("reasoning_guard"),
        "guarded first response must not leak to the client: {gateway_body}"
    );

    let first = upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive first upstream request");
    let second = upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive retry upstream request");
    drop(server);
    upstream_join.join().expect("join mock upstream");

    for captured in [&first, &second] {
        assert!(
            captured
                .headers
                .get("authorization")
                .is_some_and(|auth| auth.contains("access_acc_guard_retry_primary")),
            "internal retry should stay on the same primary account"
        );
    }
    while let Ok(extra) = upstream_rx.try_recv() {
        let auth = extra
            .headers
            .get("authorization")
            .map(String::as_str)
            .unwrap_or_default();
        assert!(
            !extra.path.contains("/responses"),
            "reasoning guard retry should not make an extra responses request; extra path={} auth={auth}",
            extra.path
        );
    }

    let primary = storage
        .find_account_by_id("acc_guard_retry_primary")
        .expect("find primary")
        .expect("primary account exists");
    let primary_reason = storage
        .latest_account_status_reasons(&["acc_guard_retry_primary".to_string()])
        .expect("read primary account status reason")
        .get("acc_guard_retry_primary")
        .cloned();
    assert_eq!(
        primary.status, "active",
        "internal retry must not mark the account unavailable; reason={primary_reason:?}"
    );

    let mut log = None;
    for _ in 0..40 {
        let logs = storage
            .list_request_logs(Some("key:=gk_reasoning_guard_retry"), 20)
            .expect("list request logs");
        log = logs
            .into_iter()
            .find(|item| item.request_path == "/v1/responses" && item.status_code == Some(200));
        if log.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let log = log.expect("successful retried request log should be recorded");
    assert_eq!(log.account_id.as_deref(), Some("acc_guard_retry_primary"));
    assert_eq!(log.reasoning_output_tokens, Some(128));

    let server = TestServer::start();
    let (_, metrics_after) = get_http_raw(&server.addr, "/metrics");
    drop(server);
    assert_eq!(
        metric_value(
            &metrics_after,
            "codexmanager_gateway_reasoning_guard_matches_total",
            "mode=\"non_stream\"",
        ),
        matches_before + 1
    );
    assert_eq!(
        metric_value(
            &metrics_after,
            "codexmanager_gateway_reasoning_guard_internal_retries_total",
            "mode=\"non_stream\"",
        ),
        retries_before + 1
    );
    assert_eq!(
        metric_value(
            &metrics_after,
            "codexmanager_gateway_reasoning_guard_blocks_total",
            "mode=\"non_stream\"",
        ),
        blocks_before
    );
    assert_eq!(
        metric_value(
            &metrics_after,
            "codexmanager_gateway_failover_attempts_total",
            "",
        ),
        failovers_before
    );
}

#[test]
fn gateway_reasoning_guard_disabled_allows_non_stream_516_response() {
    let _lock = test_env_guard();
    let _guard_env = reasoning_guard_test_env(false, 0, 0);
    let dir = new_test_dir("codexmanager-gateway-reasoning-guard-disabled");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_sequence_lenient_with_content_types(
            vec![(
                200,
                guard_non_stream_response("resp_guard_disabled"),
                "application/json".to_string(),
            )],
            Duration::from_secs(3),
        );
    let upstream_base = format!("http://{upstream_addr}/backend-api/codex");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    seed_model_catalog_models(&storage, &["gpt-5.3-codex"]);
    let now = now_ts();
    insert_reasoning_guard_test_account(&storage, "acc_guard_disabled", 0, now);
    let platform_key = "pk_reasoning_guard_disabled";
    insert_reasoning_guard_test_key(&storage, "gk_reasoning_guard_disabled", platform_key, now);

    let server = codexmanager_service::start_one_shot_server().expect("start server");
    let request_body = r#"{"model":"gpt-5.3-codex","input":"hello","stream":false}"#;
    let (status, gateway_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        request_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();

    assert_eq!(status, 200, "gateway response: {gateway_body}");
    assert!(
        gateway_body.contains("guarded output"),
        "disabled guard should pass through upstream body: {gateway_body}"
    );
    assert!(
        !gateway_body.contains("reasoning_guard"),
        "disabled guard must not synthesize guard error: {gateway_body}"
    );

    upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive upstream request");
    upstream_join.join().expect("join mock upstream");
}

#[test]
fn gateway_reasoning_guard_bypasses_after_configured_consecutive_hits() {
    let _lock = test_env_guard();
    let _guard_env = reasoning_guard_test_env(true, 0, 2);
    let dir = new_test_dir("codexmanager-gateway-reasoning-guard-threshold");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_sequence_lenient_with_content_types(
            vec![
                (
                    200,
                    guard_non_stream_response("resp_guard_threshold_first"),
                    "application/json".to_string(),
                ),
                (
                    200,
                    guard_non_stream_response("resp_guard_threshold_second"),
                    "application/json".to_string(),
                ),
            ],
            Duration::from_secs(3),
        );
    let upstream_base = format!("http://{upstream_addr}/backend-api/codex");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    seed_model_catalog_models(&storage, &["gpt-5.3-codex"]);
    let now = now_ts();
    insert_reasoning_guard_test_account(&storage, "acc_guard_threshold", 0, now);
    let platform_key = "pk_reasoning_guard_threshold";
    insert_reasoning_guard_test_key(&storage, "gk_reasoning_guard_threshold", platform_key, now);

    let server = TestServer::start();
    let request_body = r#"{"model":"gpt-5.3-codex","input":"hello","stream":false}"#;
    let headers = [
        ("Content-Type", "application/json"),
        ("Authorization", &format!("Bearer {platform_key}")),
    ];
    let (first_status, first_body) =
        post_http_raw(&server.addr, "/v1/responses", request_body, &headers);
    assert_eq!(first_status, 502, "first gateway response: {first_body}");
    assert!(
        first_body.contains("reasoning_tokens=516"),
        "first response should be blocked by guard: {first_body}"
    );

    let (second_status, second_body) =
        post_http_raw(&server.addr, "/v1/responses", request_body, &headers);
    assert_eq!(second_status, 200, "second gateway response: {second_body}");
    assert!(
        second_body.contains("guarded output"),
        "second consecutive 516 should be passed through after threshold: {second_body}"
    );

    upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive first upstream request");
    upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive second upstream request");
    drop(server);
    upstream_join.join().expect("join mock upstream");
}

#[test]
fn gateway_reasoning_guard_stream_returns_502_without_leaking_buffered_delta() {
    let _lock = test_env_guard();
    let _guard_env = reasoning_guard_test_env(true, 0, 0);
    let dir = new_test_dir("codexmanager-gateway-reasoning-guard-stream-last");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_sequence_lenient_with_content_types(
            vec![(
                200,
                stream_response_with_reasoning_tokens(516, "must-not-leak"),
                "text/event-stream".to_string(),
            )],
            Duration::from_secs(3),
        );
    let upstream_base = format!("http://{upstream_addr}/backend-api/codex");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    seed_model_catalog_models(&storage, &["gpt-5.3-codex"]);
    let now = now_ts();
    insert_reasoning_guard_test_account(&storage, "acc_guard_stream_last", 0, now);
    let platform_key = "pk_reasoning_guard_stream_last";
    insert_reasoning_guard_test_key(
        &storage,
        "gk_reasoning_guard_stream_last",
        platform_key,
        now,
    );

    let server = codexmanager_service::start_one_shot_server().expect("start server");
    let request_body = r#"{"model":"gpt-5.3-codex","input":"hello","stream":true}"#;
    let (status, gateway_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        request_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();

    assert_eq!(status, 502, "gateway response: {gateway_body}");
    assert!(
        gateway_body.contains("reasoning_tokens=516"),
        "502 should explain the guard trigger: {gateway_body}"
    );
    assert!(
        !gateway_body.contains("must-not-leak") && !gateway_body.contains("[DONE]"),
        "strict guard must not stream buffered upstream content after detecting 516: {gateway_body}"
    );

    let captured = upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive upstream request");
    upstream_join.join().expect("join mock upstream");
    assert!(
        captured
            .headers
            .get("authorization")
            .is_some_and(|auth| auth.contains("access_acc_guard_stream_last")),
        "single attempt should use the configured account"
    );
}

#[test]
fn gateway_reasoning_guard_stream_allows_non_516_reasoning_tokens() {
    let _lock = test_env_guard();
    let _guard_env = reasoning_guard_test_env(true, 0, 0);
    let dir = new_test_dir("codexmanager-gateway-reasoning-guard-stream-ok");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_sequence_lenient_with_content_types(
            vec![(
                200,
                stream_response_with_reasoning_tokens(128, "normal-delta"),
                "text/event-stream".to_string(),
            )],
            Duration::from_secs(3),
        );
    let upstream_base = format!("http://{upstream_addr}/backend-api/codex");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    seed_model_catalog_models(&storage, &["gpt-5.3-codex"]);
    let now = now_ts();
    insert_reasoning_guard_test_account(&storage, "acc_guard_stream_ok", 0, now);
    let platform_key = "pk_reasoning_guard_stream_ok";
    insert_reasoning_guard_test_key(&storage, "gk_reasoning_guard_stream_ok", platform_key, now);

    let server = codexmanager_service::start_one_shot_server().expect("start server");
    let request_body = r#"{"model":"gpt-5.3-codex","input":"hello","stream":true}"#;
    let (status, gateway_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        request_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();

    assert_eq!(status, 200, "gateway response: {gateway_body}");
    assert!(
        gateway_body.contains("normal-delta") && gateway_body.contains("[DONE]"),
        "normal reasoning usage should be replayed as SSE: {gateway_body}"
    );
    assert!(
        !gateway_body.contains("reasoning_guard"),
        "normal reasoning usage must not trigger guard response: {gateway_body}"
    );

    let captured = upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive upstream request");
    upstream_join.join().expect("join mock upstream");
    assert!(
        captured
            .headers
            .get("authorization")
            .is_some_and(|auth| auth.contains("access_acc_guard_stream_ok")),
        "single attempt should use the configured account"
    );
}

/// Fix B 端到端：快要耗尽的账号（99% used）即使 sort 排前，也应被降权到候选尾部，
/// 首个请求直接命中健康账号，不必经历失败-重试流程。
#[test]
fn gateway_low_quota_account_is_skipped_on_first_request() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-low-quota-skip");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let ok_sse = concat!(
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_lowq_ok\",\"model\":\"gpt-5.3-codex\",\"usage\":{\"input_tokens\":3,\"output_tokens\":1,\"total_tokens\":4}}}\n\n",
        "data: [DONE]\n\n"
    );

    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_sequence_lenient_with_content_types(
            vec![(200, ok_sse.to_string(), "text/event-stream".to_string())],
            Duration::from_secs(3),
        );
    let upstream_base = format!("http://{upstream_addr}/backend-api/codex");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    seed_model_catalog_models(&storage, &["gpt-5.3-codex"]);
    let now = now_ts();

    // sort=0 的账号快照 99%（快耗尽），sort=1 的健康（10%）。
    // Fix B 应把 exhausted 排到尾部，实际请求只打 healthy。
    let rows: Vec<(&str, i64, f64)> = vec![("acc_exhausted", 0, 99.0), ("acc_healthy", 1, 10.0)];
    for (id, sort, used_pct) in &rows {
        storage
            .insert_account(&Account {
                id: (*id).to_string(),
                label: (*id).to_string(),
                issuer: "https://auth.openai.com".to_string(),
                chatgpt_account_id: Some(format!("chatgpt_{id}")),
                workspace_id: None,
                group_name: None,
                sort: *sort,
                status: "active".to_string(),
                created_at: now + *sort,
                updated_at: now + *sort,
            })
            .expect("insert account");
        storage
            .insert_token(&Token {
                account_id: (*id).to_string(),
                id_token: String::new(),
                access_token: format!("access_{id}"),
                refresh_token: String::new(),
                api_key_access_token: Some(format!("api_access_{id}")),
                last_refresh: now,
            })
            .expect("insert token");
        storage
            .insert_usage_snapshot(&UsageSnapshotRecord {
                account_id: (*id).to_string(),
                used_percent: Some(*used_pct),
                window_minutes: Some(300),
                resets_at: None,
                secondary_used_percent: None,
                secondary_window_minutes: None,
                secondary_resets_at: None,
                credits_json: None,
                captured_at: now,
            })
            .expect("insert snapshot");
    }

    let platform_key = "pk_low_quota_skip";
    storage
        .insert_api_key(&ApiKey {
            id: "gk_low_quota_skip".to_string(),
            name: Some("low-quota-skip".to_string()),
            model_slug: Some("gpt-5.3-codex".to_string()),
            reasoning_effort: None,
            service_tier: None,
            rotation_strategy: "account_rotation".to_string(),
            aggregate_api_id: None,
            account_plan_filter: None,
            aggregate_api_url: None,
            client_type: "codex".to_string(),
            protocol_type: "openai_compat".to_string(),
            auth_scheme: "authorization_bearer".to_string(),
            upstream_base_url: None,
            static_headers_json: None,
            key_hash: hash_platform_key_for_test(platform_key),
            status: "active".to_string(),
            created_at: now,
            last_used_at: None,
        })
        .expect("insert api key");

    let server = codexmanager_service::start_one_shot_server().expect("start server");
    let request_body = serde_json::json!({
        "model": "gpt-5.3-codex",
        "input": "hello",
        "stream": true
    });
    let request_body = serde_json::to_string(&request_body).expect("serialize request");
    let (status, gateway_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        &request_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();
    assert_eq!(status, 200, "gateway response: {gateway_body}");

    let captured = upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive upstream request");
    upstream_join.join().expect("join mock upstream");

    let auth = captured
        .headers
        .get("authorization")
        .map(String::as_str)
        .unwrap_or_default();
    assert!(
        auth.contains("access_acc_healthy"),
        "即便 sort=0 的账号排在前，99% used 的账号也应该被降到尾部；实际 auth 头：{auth}"
    );
    assert!(
        upstream_rx
            .recv_timeout(Duration::from_millis(300))
            .is_err(),
        "低配额账号应被直接跳过，不应再有第二次上游请求"
    );
}
