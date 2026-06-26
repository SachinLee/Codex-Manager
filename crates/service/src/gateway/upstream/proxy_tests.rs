use super::{
    exhausted_gateway_error_for_log, hybrid_route_error_message, model_route_error,
    provider_upstream_hint, request_deadline_for_path, resolve_aggregate_candidates_for_route,
    resolve_upstream_is_stream, respond_when_account_candidates_empty,
    should_fallback_to_aggregate_after_account_exhaustion,
    should_try_provider_executor_aggregate_route,
};
use crate::gateway::upstream::executor::{
    GatewayUpstreamExecutionPlan, GatewayUpstreamExecutorKind, GatewayUpstreamRouteKind,
};
use codexmanager_core::rpc::types::{
    ManagedModelCatalogEntry, ManagedModelCatalogResult, ModelInfo,
};
use codexmanager_core::storage::{now_ts, Account, AggregateApi, ModelSourceMapping, Storage};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

fn execution_plan(route_kind: GatewayUpstreamRouteKind) -> GatewayUpstreamExecutionPlan {
    GatewayUpstreamExecutionPlan {
        executor_kind: GatewayUpstreamExecutorKind::CodexResponses,
        route_kind,
    }
}

fn insert_test_aggregate_api(storage: &Storage, id: &str) {
    insert_test_aggregate_api_with_provider(storage, id, "codex");
}

fn insert_test_aggregate_api_with_provider(storage: &Storage, id: &str, provider_type: &str) {
    let now = now_ts();
    storage
        .insert_aggregate_api(&AggregateApi {
            id: id.to_string(),
            provider_type: provider_type.to_string(),
            supplier_name: Some(id.to_string()),
            sort: 0,
            url: format!("https://{id}.example/v1"),
            auth_type: "apikey".to_string(),
            auth_params_json: None,
            action: None,
            model_override: None,
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
        })
        .expect("insert aggregate api");
}

fn seed_platform_catalog(storage: &Storage, slug: &str) {
    crate::apikey_models::save_managed_model_catalog_with_storage(
        storage,
        &ManagedModelCatalogResult {
            items: vec![ManagedModelCatalogEntry {
                model: ModelInfo {
                    slug: slug.to_string(),
                    display_name: slug.to_string(),
                    supported_in_api: true,
                    visibility: Some("list".to_string()),
                    ..Default::default()
                },
                source_kind: "remote".to_string(),
                user_edited: false,
                sort_index: 0,
                updated_at: now_ts(),
            }],
            extra: BTreeMap::new(),
        },
    )
    .expect("seed platform catalog");
}

/// 函数 `exhausted_gateway_error_includes_attempts_skips_and_last_error`
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
fn exhausted_gateway_error_includes_attempts_skips_and_last_error() {
    let message = exhausted_gateway_error_for_log(
        &["acc-a".to_string(), "acc-b".to_string()],
        2,
        1,
        Some("upstream challenge blocked"),
    );

    assert!(message.contains("no available account"));
    assert!(message.contains("kind=no_available_account_exhausted"));
    assert!(message.contains("attempted=acc-a,acc-b"));
    assert!(message.contains("skipped(cooldown=2, inflight=1)"));
    assert!(message.contains("last_attempt=upstream challenge blocked"));
}

/// 函数 `exhausted_gateway_error_marks_cooldown_only_skip_kind`
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
fn exhausted_gateway_error_marks_cooldown_only_skip_kind() {
    let message = exhausted_gateway_error_for_log(&[], 2, 0, None);

    assert!(message.contains("kind=no_available_account_cooldown"));
}

#[test]
fn resolve_upstream_is_stream_keeps_non_compact_responses_on_sse_upstream() {
    assert!(resolve_upstream_is_stream(false, "/v1/responses"));
    assert!(resolve_upstream_is_stream(
        false,
        "/v1/responses?stream=false"
    ));
    assert!(!resolve_upstream_is_stream(false, "/v1/responses/compact"));
    assert!(!resolve_upstream_is_stream(false, "/v1/chat/completions"));
    assert!(resolve_upstream_is_stream(true, "/v1/chat/completions"));
}

#[test]
fn request_deadline_for_responses_uses_upstream_stream_semantics() {
    let _guard = crate::test_env_guard();
    let previous_total = crate::gateway::current_upstream_total_timeout_ms();
    let previous_stream = crate::gateway::current_upstream_stream_timeout_ms();

    crate::gateway::set_upstream_total_timeout_ms(120_000);
    crate::gateway::set_upstream_stream_timeout_ms(300_000);

    let started_at = Instant::now();
    let deadline =
        request_deadline_for_path(started_at, false, "/v1/responses").expect("responses deadline");
    let timeout = deadline
        .checked_duration_since(started_at)
        .expect("deadline should be after start");

    crate::gateway::set_upstream_total_timeout_ms(previous_total);
    crate::gateway::set_upstream_stream_timeout_ms(previous_stream);

    assert!(timeout > Duration::from_secs(250));
    assert!(timeout <= Duration::from_secs(300));
}

#[test]
fn only_explicit_aggregate_route_uses_aggregate_candidates() {
    assert!(should_try_provider_executor_aggregate_route(
        GatewayUpstreamExecutionPlan {
            executor_kind: GatewayUpstreamExecutorKind::Claude,
            route_kind: GatewayUpstreamRouteKind::AggregateApi,
        }
    ));
    assert!(should_try_provider_executor_aggregate_route(
        GatewayUpstreamExecutionPlan {
            executor_kind: GatewayUpstreamExecutorKind::Gemini,
            route_kind: GatewayUpstreamRouteKind::AggregateApi,
        }
    ));
    assert!(!should_try_provider_executor_aggregate_route(
        GatewayUpstreamExecutionPlan {
            executor_kind: GatewayUpstreamExecutorKind::Claude,
            route_kind: GatewayUpstreamRouteKind::AccountRotation,
        }
    ));
    assert!(should_try_provider_executor_aggregate_route(
        GatewayUpstreamExecutionPlan {
            executor_kind: GatewayUpstreamExecutorKind::CodexResponses,
            route_kind: GatewayUpstreamRouteKind::AggregateApi,
        }
    ));
    assert!(!should_try_provider_executor_aggregate_route(
        GatewayUpstreamExecutionPlan {
            executor_kind: GatewayUpstreamExecutorKind::Gemini,
            route_kind: GatewayUpstreamRouteKind::AccountRotation,
        }
    ));
    assert!(!should_try_provider_executor_aggregate_route(
        GatewayUpstreamExecutionPlan {
            executor_kind: GatewayUpstreamExecutorKind::CodexResponses,
            route_kind: GatewayUpstreamRouteKind::AccountRotation,
        }
    ));
}

#[test]
fn hybrid_account_first_keeps_account_empty_for_aggregate_fallback() {
    let hybrid = GatewayUpstreamExecutionPlan {
        executor_kind: GatewayUpstreamExecutorKind::CodexResponses,
        route_kind: GatewayUpstreamRouteKind::HybridAccountFirst,
    };
    let account_only = GatewayUpstreamExecutionPlan {
        executor_kind: GatewayUpstreamExecutorKind::CodexResponses,
        route_kind: GatewayUpstreamRouteKind::AccountRotation,
    };
    let aggregate_only = GatewayUpstreamExecutionPlan {
        executor_kind: GatewayUpstreamExecutorKind::CodexResponses,
        route_kind: GatewayUpstreamRouteKind::AggregateApi,
    };

    assert!(!respond_when_account_candidates_empty(hybrid));
    assert!(respond_when_account_candidates_empty(account_only));
    assert!(respond_when_account_candidates_empty(aggregate_only));
}

#[test]
fn only_hybrid_falls_back_to_aggregate_after_account_exhaustion() {
    assert!(should_fallback_to_aggregate_after_account_exhaustion(
        GatewayUpstreamExecutionPlan {
            executor_kind: GatewayUpstreamExecutorKind::CodexResponses,
            route_kind: GatewayUpstreamRouteKind::HybridAccountFirst,
        }
    ));
    assert!(!should_fallback_to_aggregate_after_account_exhaustion(
        GatewayUpstreamExecutionPlan {
            executor_kind: GatewayUpstreamExecutorKind::CodexResponses,
            route_kind: GatewayUpstreamRouteKind::AccountRotation,
        }
    ));
    assert!(!should_fallback_to_aggregate_after_account_exhaustion(
        GatewayUpstreamExecutionPlan {
            executor_kind: GatewayUpstreamExecutorKind::CodexResponses,
            route_kind: GatewayUpstreamRouteKind::AggregateApi,
        }
    ));
}

#[test]
fn hybrid_route_error_mentions_both_pools() {
    let message = hybrid_route_error_message(
        Some("无可用账号(no available account)"),
        "aggregate api not found for provider codex",
    );

    assert!(message.contains("账号池与聚合 API 均不可用"));
    assert!(message.contains("no available account"));
    assert!(message.contains("aggregate api not found for provider codex"));
}

#[test]
fn provider_upstream_hint_reports_expected_aggregate_provider_type() {
    assert_eq!(
        provider_upstream_hint(GatewayUpstreamExecutorKind::Claude),
        Some(("Claude", "claude"))
    );
    assert_eq!(
        provider_upstream_hint(GatewayUpstreamExecutorKind::Gemini),
        Some(("Gemini", "gemini"))
    );
    assert_eq!(
        provider_upstream_hint(GatewayUpstreamExecutorKind::CodexResponses),
        None
    );
}

#[test]
fn aggregate_route_model_validation_bootstraps_aggregate_source() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_test_aggregate_api(&storage, "agg-route");
    storage
        .upsert_discovered_model_source_models(
            "aggregate_api",
            "agg-route",
            &["vendor-route".to_string()],
            "synced",
        )
        .expect("seed aggregate source model");

    model_route_error(
        &storage,
        "key-route",
        Some("vendor-route"),
        execution_plan(GatewayUpstreamRouteKind::AggregateApi),
    )
    .expect("aggregate route should bootstrap source mapping");

    let mappings = storage
        .list_enabled_model_source_mappings_for_platform("vendor-route")
        .expect("list mappings");
    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[0].source_kind, "aggregate_api");
    assert_eq!(mappings[0].source_id, "agg-route");
}

#[test]
fn aggregate_route_model_filter_uses_batched_source_mappings() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_test_aggregate_api(&storage, "agg-with-model");
    insert_test_aggregate_api(&storage, "agg-without-model");
    let now = now_ts();
    for (id, source_id, upstream_model, priority) in [
        ("map-low", "agg-with-model", "vendor-low", 0),
        ("map-top", "agg-with-model", "vendor-top", 5),
    ] {
        storage
            .upsert_model_source_mapping(&ModelSourceMapping {
                id: id.to_string(),
                platform_model_slug: "vendor-batched".to_string(),
                source_kind: "aggregate_api".to_string(),
                source_id: source_id.to_string(),
                upstream_model: upstream_model.to_string(),
                enabled: true,
                priority,
                weight: 1,
                billing_model_slug: None,
                created_at: now,
                updated_at: now,
            })
            .expect("seed aggregate mapping");
    }

    let candidates = resolve_aggregate_candidates_for_route(
        &storage,
        "openai_responses",
        None,
        Some("vendor-batched"),
    )
    .expect("resolve aggregate candidates");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id, "agg-with-model");
    assert_eq!(candidates[0].model_override.as_deref(), Some("vendor-top"));
}

#[test]
fn explicit_aggregate_route_candidate_precedes_provider_candidates() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_test_aggregate_api_with_provider(&storage, "agg-codex-explicit", "codex");
    insert_test_aggregate_api_with_provider(&storage, "agg-claude-explicit", "claude");
    let now = now_ts();
    for (id, source_id, upstream_model) in [
        ("map-codex-explicit", "agg-codex-explicit", "vendor-codex"),
        (
            "map-claude-explicit",
            "agg-claude-explicit",
            "vendor-claude",
        ),
    ] {
        storage
            .upsert_model_source_mapping(&ModelSourceMapping {
                id: id.to_string(),
                platform_model_slug: "vendor-cross-provider".to_string(),
                source_kind: "aggregate_api".to_string(),
                source_id: source_id.to_string(),
                upstream_model: upstream_model.to_string(),
                enabled: true,
                priority: 0,
                weight: 1,
                billing_model_slug: None,
                created_at: now,
                updated_at: now,
            })
            .expect("seed aggregate mapping");
    }

    let openai_candidates = resolve_aggregate_candidates_for_route(
        &storage,
        "openai_responses",
        Some("agg-claude-explicit"),
        Some("vendor-cross-provider"),
    )
    .expect("resolve openai candidates with explicit claude aggregate");
    let openai_candidate_ids = openai_candidates
        .iter()
        .map(|candidate| candidate.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        openai_candidate_ids,
        vec!["agg-claude-explicit", "agg-codex-explicit"]
    );
    assert_eq!(
        openai_candidates[0].model_override.as_deref(),
        Some("vendor-claude")
    );

    let anthropic_candidates = resolve_aggregate_candidates_for_route(
        &storage,
        "anthropic_native",
        Some("agg-codex-explicit"),
        Some("vendor-cross-provider"),
    )
    .expect("resolve anthropic candidates with explicit codex aggregate");
    let anthropic_candidate_ids = anthropic_candidates
        .iter()
        .map(|candidate| candidate.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        anthropic_candidate_ids,
        vec!["agg-codex-explicit", "agg-claude-explicit"]
    );
    assert_eq!(
        anthropic_candidates[0].model_override.as_deref(),
        Some("vendor-codex")
    );
}

#[test]
fn account_route_model_validation_ignores_aggregate_only_mapping() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    seed_platform_catalog(&storage, "vendor-account-route");
    let now = now_ts();
    storage
        .upsert_model_source_mapping(&ModelSourceMapping {
            id: "mapping-aggregate-only".to_string(),
            platform_model_slug: "vendor-account-route".to_string(),
            source_kind: "aggregate_api".to_string(),
            source_id: "agg-only".to_string(),
            upstream_model: "vendor-account-route".to_string(),
            enabled: true,
            priority: 0,
            weight: 1,
            billing_model_slug: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed aggregate mapping");

    let err = model_route_error(
        &storage,
        "key-route",
        Some("vendor-account-route"),
        execution_plan(GatewayUpstreamRouteKind::AccountRotation),
    )
    .expect_err("account route should require an account mapping");

    assert_eq!(err.0, 503);
    assert!(err.1.contains("model_unavailable"));
}

#[test]
fn hybrid_model_validation_accepts_aggregate_mapping() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_test_aggregate_api(&storage, "agg-hybrid");
    storage
        .upsert_discovered_model_source_models(
            "aggregate_api",
            "agg-hybrid",
            &["vendor-hybrid".to_string()],
            "synced",
        )
        .expect("seed aggregate source model");

    model_route_error(
        &storage,
        "key-route",
        Some("vendor-hybrid"),
        execution_plan(GatewayUpstreamRouteKind::HybridAccountFirst),
    )
    .expect("hybrid route should accept aggregate mapping");
}

#[test]
fn account_route_model_validation_accepts_direct_upstream_source_model() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let now = now_ts();
    storage
        .insert_account(&Account {
            id: "acc-direct-route".to_string(),
            label: "acc-direct-route".to_string(),
            issuer: "issuer".to_string(),
            chatgpt_account_id: None,
            workspace_id: None,
            group_name: None,
            sort: 0,
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("insert account");
    storage
        .upsert_discovered_model_source_models(
            "openai_account",
            "acc-direct-route",
            &["gpt-5.4-mini".to_string()],
            "manual",
        )
        .expect("seed direct upstream source model");

    model_route_error(
        &storage,
        "key-route",
        Some("gpt-5.4-mini"),
        execution_plan(GatewayUpstreamRouteKind::AccountRotation),
    )
    .expect("account route should accept direct upstream source model");
}
