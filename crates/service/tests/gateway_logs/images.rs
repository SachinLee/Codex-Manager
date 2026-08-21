use super::*;
use codexmanager_core::storage::AggregateApi;

fn aggregate_api_for_test(id: &str, sort: i64, url: String, now: i64) -> AggregateApi {
    AggregateApi {
        id: id.to_string(),
        provider_type: "codex".to_string(),
        supplier_name: Some(id.to_string()),
        sort,
        url,
        auth_type: "apikey".to_string(),
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

#[test]
fn aggregate_api_body_too_large_does_not_retry_or_failover() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-aggregate-body-too-large-terminal");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let upstream_error = serde_json::json!({
        "error": {
            "message": "Request body too large (max 20971520 bytes)",
            "type": "invalid_request_error"
        }
    });
    let (upstream_addr, upstream_rx, upstream_join) = start_mock_upstream_sequence_lenient(
        vec![(
            400,
            serde_json::to_string(&upstream_error).expect("serialize upstream error"),
        )],
        Duration::from_secs(3),
    );

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    seed_model_catalog_models(&storage, &["gpt-5.5"]);
    let now = now_ts();
    for (id, sort) in [("agg_large_body_primary", 0), ("agg_large_body_backup", 1)] {
        storage
            .insert_aggregate_api(&aggregate_api_for_test(
                id,
                sort,
                format!("http://{upstream_addr}"),
                now,
            ))
            .expect("insert aggregate api");
        storage
            .upsert_aggregate_api_secret(id, "upstream-secret")
            .expect("insert aggregate secret");
        // The runtime resolves aggregate candidates from model catalog V2.
        // Keep the fixture aligned with that production-only routing source;
        // the legacy source records below continue to cover their own storage
        // compatibility behavior.
        seed_model_catalog_route(&storage, "gpt-5.5", "aggregate_api", id, "gpt-5.5", sort);
        storage
            .upsert_model_source_model(&ModelSourceModel {
                source_kind: "aggregate_api".to_string(),
                source_id: id.to_string(),
                upstream_model: "gpt-5.5".to_string(),
                display_name: Some("GPT 5.5".to_string()),
                status: "available".to_string(),
                discovery_kind: "manual".to_string(),
                last_synced_at: Some(now),
                extra_json: "{}".to_string(),
                created_at: now,
                updated_at: now,
            })
            .expect("insert aggregate source model");
        storage
            .upsert_model_source_mapping(&ModelSourceMapping {
                id: format!("mapping_{id}"),
                platform_model_slug: "gpt-5.5".to_string(),
                source_kind: "aggregate_api".to_string(),
                source_id: id.to_string(),
                upstream_model: "gpt-5.5".to_string(),
                enabled: true,
                priority: sort,
                weight: 1,
                billing_model_slug: None,
                created_at: now,
                updated_at: now,
            })
            .expect("insert aggregate source mapping");
    }

    let platform_key = "pk_aggregate_body_too_large";
    storage
        .insert_api_key(&ApiKey {
            id: "gk_aggregate_body_too_large".to_string(),
            name: Some("aggregate-body-too-large".to_string()),
            model_slug: Some("gpt-5.5".to_string()),
            reasoning_effort: None,
            service_tier: None,
            rotation_strategy: "aggregate_api_rotation".to_string(),
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
    let req_body = serde_json::json!({
        "model": "gpt-5.5",
        "input": [{
            "role": "user",
            "content": [
                { "type": "input_text", "text": "analyze these images" },
                { "type": "input_image", "image_url": "data:image/png;base64,aGVsbG8=" },
                { "type": "input_image", "image_url": "data:image/png;base64,d29ybGQ=" }
            ]
        }],
        "stream": true
    })
    .to_string();
    let (status, response_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        &req_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();

    assert_eq!(status, 413, "gateway response: {response_body}");
    assert!(
        response_body.contains("Request body too large"),
        "gateway should return upstream payload-size message, got {response_body}"
    );

    let captured = upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive first upstream request");
    assert_eq!(captured.path, "/v1/responses");
    assert!(
        upstream_rx
            .recv_timeout(Duration::from_millis(700))
            .is_err(),
        "body-too-large must not retry the same supplier or fail over to the backup"
    );
    upstream_join.join().expect("join mock upstream");
}

#[test]
fn aggregate_api_optional_image_capability_retries_once_without_the_tool() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-aggregate-image-capability-retry");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());
    let _mode_guard = EnvGuard::set("CODEXMANAGER_CAPABILITY_ROUTING_MODE", "enforce");

    let capability_error = serde_json::json!({
        "error": {
            "message": "Image generation is not enabled for this group",
            "type": "permission_error"
        }
    });
    let success = serde_json::json!({
        "id": "resp_capability_retry",
        "object": "response",
        "status": "completed",
        "model": "grok-4.5",
        "output": [],
        "usage": { "input_tokens": 1, "output_tokens": 1, "total_tokens": 2 }
    });
    let (upstream_addr, upstream_rx, upstream_join) = start_mock_upstream_sequence_lenient(
        vec![
            (502, capability_error.to_string()),
            (200, success.to_string()),
        ],
        Duration::from_secs(3),
    );

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    seed_model_catalog_models(&storage, &["grok-4.5"]);
    let now = now_ts();
    let aggregate_id = "agg_image_capability_retry";
    storage
        .insert_aggregate_api(&aggregate_api_for_test(
            aggregate_id,
            0,
            format!("http://{upstream_addr}"),
            now,
        ))
        .expect("insert aggregate api");
    storage
        .upsert_aggregate_api_secret(aggregate_id, "upstream-secret")
        .expect("insert aggregate secret");
    seed_model_catalog_route(
        &storage,
        "grok-4.5",
        "aggregate_api",
        aggregate_id,
        "grok-4.5",
        0,
    );
    storage
        .upsert_model_source_model(&ModelSourceModel {
            source_kind: "aggregate_api".to_string(),
            source_id: aggregate_id.to_string(),
            upstream_model: "grok-4.5".to_string(),
            display_name: Some("Grok 4.5".to_string()),
            status: "available".to_string(),
            discovery_kind: "manual".to_string(),
            last_synced_at: Some(now),
            extra_json: "{}".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("insert aggregate source model");
    storage
        .upsert_model_source_mapping(&ModelSourceMapping {
            id: "mapping_image_capability_retry".to_string(),
            platform_model_slug: "grok-4.5".to_string(),
            source_kind: "aggregate_api".to_string(),
            source_id: aggregate_id.to_string(),
            upstream_model: "grok-4.5".to_string(),
            enabled: true,
            priority: 0,
            weight: 1,
            billing_model_slug: None,
            created_at: now,
            updated_at: now,
        })
        .expect("insert aggregate source mapping");

    let platform_key = "pk_image_capability_retry";
    storage
        .insert_api_key(&ApiKey {
            id: "gk_image_capability_retry".to_string(),
            name: Some("image-capability-retry".to_string()),
            model_slug: Some("grok-4.5".to_string()),
            reasoning_effort: None,
            service_tier: None,
            rotation_strategy: "aggregate_api_rotation".to_string(),
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
        "model": "grok-4.5",
        "input": [{ "role": "user", "content": "hello" }],
        "tools": [{ "type": "image_generation" }],
        "tool_choice": "auto",
        "stream": false
    })
    .to_string();
    let (status, response_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        &request_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();
    assert_eq!(status, 200, "gateway response: {response_body}");

    let first = upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive native request");
    let second = upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive downgraded retry");
    upstream_join.join().expect("join mock upstream");
    let first_body: serde_json::Value =
        serde_json::from_slice(&decode_upstream_request_body(&first)).expect("native body json");
    let second_body: serde_json::Value =
        serde_json::from_slice(&decode_upstream_request_body(&second)).expect("retry body json");
    assert_eq!(first_body["tools"][0]["type"], "image_generation");
    assert_eq!(second_body["tools"].as_array().map(Vec::len), Some(0));

    let observations = storage
        .list_gateway_capability_observations("aggregate_api", aggregate_id, now)
        .expect("list observations");
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].state, "unsupported");
    assert_eq!(observations[0].observation_source, "runtime");
    let mut attempts = Vec::new();
    for _ in 0..40 {
        attempts = storage
            .list_gateway_upstream_attempt_events("aggregate_api", aggregate_id, 10)
            .expect("list capability attempts");
        if attempts.len() >= 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    attempts.sort_by_key(|item| item.attempt_index);
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].phase, "native");
    assert_eq!(attempts[0].outcome, "rejected");
    assert_eq!(attempts[1].phase, "downgrade");
    assert!(attempts[1]
        .transform_codes_json
        .contains("drop_optional_image_generation"));
    let request_logs = storage
        .list_request_logs(Some("key:=gk_image_capability_retry"), 20)
        .expect("list request logs");
    assert_eq!(
        request_logs.len(),
        1,
        "same-candidate capability retry must not create a second final request log"
    );
}

#[test]
fn gateway_images_generation_wraps_codex_sse_as_openai_images_json() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-images-generation");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let codex_sse = concat!(
        "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"id\":\"ig_test\",\"type\":\"image_generation_call\",\"status\":\"completed\",\"result\":\"aGVsbG8=\",\"revised_prompt\":\"a small cat\",\"output_format\":\"png\",\"size\":\"1024x1024\",\"quality\":\"high\",\"background\":\"transparent\"}}\n\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_image_test\",\"model\":\"gpt-5.4-mini\",\"created_at\":1772000000,\"usage\":{\"input_tokens\":3,\"output_tokens\":1,\"total_tokens\":4}}}\n\n",
        "data: [DONE]\n\n"
    );
    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_sequence_lenient_with_content_types(
            vec![(200, codex_sse.to_string(), "text/event-stream".to_string())],
            Duration::from_secs(3),
        );
    let upstream_base = format!("http://{upstream_addr}/backend-api/codex");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    let _rules_guard = GatewayModelForwardRulesResetGuard::reset();
    seed_model_catalog_models(&storage, &["gpt-5.4-mini", "gpt-5.4", "gpt-image-2"]);
    let now = now_ts();
    storage
        .insert_account(&Account {
            id: "acc_images_generation".to_string(),
            label: "images-generation".to_string(),
            issuer: "https://auth.openai.com".to_string(),
            chatgpt_account_id: Some("chatgpt_images_generation".to_string()),
            workspace_id: None,
            group_name: None,
            sort: 0,
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("insert account");
    storage
        .insert_token(&Token {
            account_id: "acc_images_generation".to_string(),
            id_token: String::new(),
            access_token: "access_images_generation".to_string(),
            refresh_token: String::new(),
            api_key_access_token: Some("api_access_images_generation".to_string()),
            last_refresh: now,
        })
        .expect("insert token");

    let platform_key = "pk_images_generation";
    storage
        .insert_api_key(&ApiKey {
            id: "gk_images_generation".to_string(),
            name: Some("images-generation".to_string()),
            model_slug: None,
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
    let req_body = serde_json::json!({
        "prompt": "draw a small cat",
        "model": "gpt-image-2",
        "size": "1024x1024",
        "quality": "high",
        "background": "transparent",
        "output_format": "png",
        "response_format": "b64_json",
        "stream": false
    })
    .to_string();
    let (status, response_body) = post_http_raw(
        &server.addr,
        "/v1/images/generations",
        &req_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    server.join();
    assert_eq!(status, 200, "gateway response: {response_body}");

    let captured = upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive upstream request");
    upstream_join.join().expect("join mock upstream");
    assert_eq!(captured.path, "/backend-api/codex/responses");
    let upstream_body: serde_json::Value =
        serde_json::from_slice(&decode_upstream_request_body(&captured)).expect("upstream json");
    assert_eq!(upstream_body["model"], "gpt-5.4-mini");
    assert_eq!(upstream_body["tools"][0]["type"], "image_generation");
    assert_eq!(upstream_body["tools"][0]["model"], "gpt-image-2");
    assert_eq!(upstream_body["tool_choice"]["type"], "image_generation");

    let value: serde_json::Value =
        serde_json::from_str(&response_body).expect("images response json");
    assert_eq!(value["created"], 1772000000);
    assert_eq!(value["data"][0]["b64_json"], "aGVsbG8=");
    assert_eq!(value["data"][0]["revised_prompt"], "a small cat");
    assert_eq!(value["output_format"], "png");
    assert_eq!(value["size"], "1024x1024");
    assert_eq!(value["quality"], "high");
    assert_eq!(value["background"], "transparent");
    assert_eq!(value["usage"]["total_tokens"], 4);
}

#[test]
fn native_codex_responses_does_not_inject_image_generation_tool() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-native-codex-no-image-generation-injection");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let result_b64 = "QUJDREVGR0g=";
    let codex_sse = format!(
        concat!(
            "event: response.output_item.done\n",
            "data: {{\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{{\"id\":\"ig_auto\",\"type\":\"image_generation_call\",\"status\":\"completed\",\"result\":\"{result_b64}\",\"output_format\":\"png\"}}}}\n\n",
            "event: response.completed\n",
            "data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"resp_auto_image\",\"model\":\"gpt-5.4\",\"output\":[{{\"id\":\"ig_auto\",\"type\":\"image_generation_call\",\"status\":\"completed\",\"result\":\"{result_b64}\",\"output_format\":\"png\"}}],\"usage\":{{\"input_tokens\":5,\"output_tokens\":2,\"total_tokens\":7}}}}}}\n\n",
            "data: [DONE]\n\n"
        ),
        result_b64 = result_b64
    );
    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_sequence_lenient_with_content_types(
            vec![(200, codex_sse, "text/event-stream".to_string())],
            Duration::from_secs(3),
        );
    let upstream_base = format!("http://{upstream_addr}/backend-api/codex");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    let _rules_guard = GatewayModelForwardRulesResetGuard::reset();
    seed_model_catalog_models(&storage, &["gpt-5.4-mini", "gpt-5.4"]);
    let now = now_ts();
    storage
        .insert_account(&Account {
            id: "acc_native_image_auto".to_string(),
            label: "native-image-auto".to_string(),
            issuer: "https://auth.openai.com".to_string(),
            chatgpt_account_id: Some("chatgpt_native_image_auto".to_string()),
            workspace_id: None,
            group_name: None,
            sort: 0,
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("insert account");
    storage
        .insert_token(&Token {
            account_id: "acc_native_image_auto".to_string(),
            id_token: String::new(),
            access_token: "access_native_image_auto".to_string(),
            refresh_token: String::new(),
            api_key_access_token: Some("api_access_native_image_auto".to_string()),
            last_refresh: now,
        })
        .expect("insert token");

    let platform_key = "pk_native_image_auto";
    storage
        .insert_api_key(&ApiKey {
            id: "gk_native_image_auto".to_string(),
            name: Some("native-image-auto".to_string()),
            model_slug: None,
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
    let req_body = serde_json::json!({
        "model": "gpt-5.4",
        "instructions": "Generate the requested image when useful.",
        "input": [{
            "type": "message",
            "role": "user",
            "content": [{
                "type": "input_text",
                "text": "帮我生成一个现场作业中台 logo，透明背景，不要文字"
            }]
        }],
        "tool_choice": "auto",
        "stream": true,
        "prompt_cache_key": "thread-native-image-auto",
        "client_metadata": {
            "x-codex-installation-id": "install-native-image-auto"
        }
    })
    .to_string();
    let (status, response_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        &req_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
            ("User-Agent", "codex_cli_rs/0.999.0 (Windows 11; x86_64)"),
            ("originator", "codex_cli_rs"),
        ],
    );
    server.join();
    assert_eq!(status, 200, "gateway response: {response_body}");

    let captured = upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive upstream request");
    upstream_join.join().expect("join mock upstream");
    assert_eq!(captured.path, "/backend-api/codex/responses");

    let upstream_body: serde_json::Value =
        serde_json::from_slice(&decode_upstream_request_body(&captured)).expect("upstream json");
    assert!(upstream_body.get("tools").is_none());
    assert_eq!(upstream_body["tool_choice"], "auto");
    assert_eq!(upstream_body["model"], "gpt-5.4");
    assert!(response_body.contains("event: response.output_item.done"));
    assert!(response_body.contains(result_b64));
    assert!(response_body.contains("data: [DONE]"));
}

#[test]
fn native_codex_image_generation_responses_request_passthroughs_tool_and_sse() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-native-codex-image-generation");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let partial_b64 = "cGFydGlhbF9pbWFnZV9jaHVuaw==";
    let result_b64 = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo=";
    let codex_sse = format!(
        concat!(
            "event: response.output_item.added\n",
            "data: {{\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{{\"id\":\"ig_native\",\"type\":\"image_generation_call\",\"status\":\"in_progress\"}}}}\n\n",
            "event: response.image_generation_call.partial_image\n",
            "data: {{\"type\":\"response.image_generation_call.partial_image\",\"item_id\":\"ig_native\",\"partial_image_index\":0,\"partial_image_b64\":\"{partial_b64}\",\"output_format\":\"png\"}}\n\n",
            "event: response.output_item.done\n",
            "data: {{\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{{\"id\":\"ig_native\",\"type\":\"image_generation_call\",\"status\":\"completed\",\"result\":\"{result_b64}\",\"output_format\":\"png\"}}}}\n\n",
            "event: response.completed\n",
            "data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"resp_native_image\",\"model\":\"gpt-5.4\",\"output\":[{{\"id\":\"ig_native\",\"type\":\"image_generation_call\",\"status\":\"completed\",\"result\":\"{result_b64}\",\"output_format\":\"png\"}}],\"usage\":{{\"input_tokens\":5,\"output_tokens\":2,\"total_tokens\":7}}}}}}\n\n",
            "data: [DONE]\n\n"
        ),
        partial_b64 = partial_b64,
        result_b64 = result_b64
    );
    let (upstream_addr, upstream_rx, upstream_join) =
        start_mock_upstream_sequence_lenient_with_content_types(
            vec![(200, codex_sse, "text/event-stream".to_string())],
            Duration::from_secs(3),
        );
    let upstream_base = format!("http://{upstream_addr}/backend-api/codex");
    let _upstream_guard = EnvGuard::set("CODEXMANAGER_UPSTREAM_BASE_URL", &upstream_base);

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    let _rules_guard = GatewayModelForwardRulesResetGuard::reset();
    seed_model_catalog_models(&storage, &["gpt-5.4-mini", "gpt-5.4"]);
    let now = now_ts();
    storage
        .insert_account(&Account {
            id: "acc_native_image_generation".to_string(),
            label: "native-image-generation".to_string(),
            issuer: "https://auth.openai.com".to_string(),
            chatgpt_account_id: Some("chatgpt_native_image_generation".to_string()),
            workspace_id: None,
            group_name: None,
            sort: 0,
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("insert account");
    storage
        .insert_token(&Token {
            account_id: "acc_native_image_generation".to_string(),
            id_token: String::new(),
            access_token: "access_native_image_generation".to_string(),
            refresh_token: String::new(),
            api_key_access_token: Some("api_access_native_image_generation".to_string()),
            last_refresh: now,
        })
        .expect("insert token");

    let platform_key = "pk_native_image_generation";
    storage
        .insert_api_key(&ApiKey {
            id: "gk_native_image_generation".to_string(),
            name: Some("native-image-generation".to_string()),
            model_slug: None,
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
    let req_body = serde_json::json!({
        "model": "gpt-5.4",
        "instructions": "Generate the requested image.",
        "input": [
            {
                "type": "message",
                "role": "user",
                "content": [{
                    "type": "input_text",
                    "text": "draw a small cat as a clean icon"
                }]
            },
            {
                "id": "ig_previous",
                "type": "image_generation_call",
                "status": "completed",
                "result": "cHJldmlvdXNfaW1hZ2U=",
                "output_format": "png"
            }
        ],
        "tools": [{
            "type": "image_generation",
            "output_format": "png"
        }],
        "tool_choice": "auto",
        "parallel_tool_calls": true,
        "stream": true,
        "store": false,
        "prompt_cache_key": "thread-native-image",
        "client_metadata": {
            "x-codex-installation-id": "install-native-image",
            "turn_id": "turn-native-image"
        }
    })
    .to_string();
    let (status, response_body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        &req_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
            ("User-Agent", "codex_cli_rs/0.999.0 (Windows 11; x86_64)"),
            ("originator", "codex_cli_rs"),
            ("x-codex-window-id", "win-native-image"),
        ],
    );
    server.join();
    assert_eq!(status, 200, "gateway response: {response_body}");

    let captured = upstream_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("receive upstream request");
    upstream_join.join().expect("join mock upstream");
    assert_eq!(captured.path, "/backend-api/codex/responses");
    assert!(captured
        .headers
        .get("user-agent")
        .is_some_and(|value| value.contains("codex_cli_rs")));

    let upstream_body: serde_json::Value =
        serde_json::from_slice(&decode_upstream_request_body(&captured)).expect("upstream json");
    assert_eq!(upstream_body["model"], "gpt-5.4");
    assert_eq!(upstream_body["tools"][0]["type"], "image_generation");
    assert_eq!(upstream_body["tools"][0]["output_format"], "png");
    assert!(upstream_body["tools"][0].get("model").is_none());
    assert_eq!(upstream_body["tool_choice"], "auto");
    assert_eq!(upstream_body["stream"], true);
    assert_eq!(upstream_body["prompt_cache_key"], "thread-native-image");
    assert_eq!(
        upstream_body["client_metadata"]["turn_id"],
        "turn-native-image"
    );
    assert!(upstream_body["input"]
        .as_array()
        .expect("input array")
        .iter()
        .any(|item| item.get("type").and_then(serde_json::Value::as_str)
            == Some("image_generation_call")
            && item.get("result").and_then(serde_json::Value::as_str)
                == Some("cHJldmlvdXNfaW1hZ2U=")));

    assert!(response_body.contains("event: response.output_item.added"));
    assert!(response_body.contains("event: response.image_generation_call.partial_image"));
    assert!(response_body.contains(partial_b64));
    assert!(response_body.contains(result_b64));
    assert!(response_body.contains("\"result\":\"QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo=\""));
    assert!(response_body.contains("data: [DONE]"));
}
