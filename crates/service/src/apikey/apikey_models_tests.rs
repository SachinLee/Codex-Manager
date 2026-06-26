use std::collections::BTreeMap;

use codexmanager_core::storage::{
    now_ts, Account, AggregateApi, ModelCatalogModelRecord, ModelGroupModel, ModelPriceRule,
    ModelSourceMapping, ModelSourceModel, Storage,
};
use serde_json::{json, Value};

use super::{
    active_openai_account_sources, auto_associate_aggregate_api_source_models,
    auto_associate_source_models, auto_platform_model_from_source_model,
    bootstrap_account_pool_model_routes, bootstrap_aggregate_api_model_routes,
    delete_model_catalog_entry, managed_catalog_to_models_response, merge_managed_model_catalog,
    merge_models_response, normalize_managed_model_catalog, normalize_models_response,
    prune_unedited_remote_model_catalog_entries_missing_from_remote,
    read_managed_model_catalog_from_storage, read_managed_model_routing_from_storage,
    read_model_options_from_storage, save_managed_model_catalog_with_storage,
    save_model_options_with_storage, sync_aggregate_api_source_models,
    sync_aggregate_api_source_models_with_discovery, MODEL_SOURCE_KIND_CUSTOM,
    MODEL_SOURCE_KIND_REMOTE, PREF_UNLINKED, ROUTING_SOURCE_KIND_AGGREGATE_API,
    ROUTING_SOURCE_KIND_OPENAI_ACCOUNT,
};
use codexmanager_core::rpc::types::{
    ManagedModelCatalogEntry, ManagedModelCatalogResult, ModelInfo, ModelsResponse,
};

fn insert_test_account(storage: &Storage, id: &str) {
    let now = now_ts();
    storage
        .insert_account(&Account {
            id: id.to_string(),
            label: id.to_string(),
            issuer: "issuer".to_string(),
            chatgpt_account_id: Some(id.to_string()),
            workspace_id: None,
            group_name: None,
            sort: 0,
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("insert account");
}

fn insert_test_aggregate_api(storage: &Storage, id: &str, status: &str) {
    let now = now_ts();
    storage
        .insert_aggregate_api(&AggregateApi {
            id: id.to_string(),
            provider_type: "codex".to_string(),
            supplier_name: Some(id.to_string()),
            sort: 0,
            url: format!("https://{id}.example/v1"),
            auth_type: "apikey".to_string(),
            auth_params_json: None,
            action: None,
            model_override: None,
            status: status.to_string(),
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

fn seed_platform_catalog(storage: &Storage, slugs: &[&str]) {
    let payload = ManagedModelCatalogResult {
        items: slugs
            .iter()
            .enumerate()
            .map(|(index, slug)| ManagedModelCatalogEntry {
                model: ModelInfo {
                    slug: (*slug).to_string(),
                    display_name: (*slug).to_string(),
                    supported_in_api: true,
                    visibility: Some("list".to_string()),
                    ..Default::default()
                },
                source_kind: MODEL_SOURCE_KIND_REMOTE.to_string(),
                user_edited: false,
                sort_index: index as i64,
                updated_at: 0,
            })
            .collect(),
        extra: BTreeMap::new(),
    };
    save_managed_model_catalog_with_storage(storage, &payload).expect("seed platform catalog");
}

fn seed_model_price_rule(storage: &Storage, id: &str, model_pattern: &str, source: &str) {
    let now = now_ts();
    storage
        .upsert_model_price_rule(&ModelPriceRule {
            id: id.to_string(),
            provider: "openai".to_string(),
            model_pattern: model_pattern.to_string(),
            match_type: "exact".to_string(),
            billing_mode: "standard".to_string(),
            currency: "USD".to_string(),
            unit: "per_1m_tokens".to_string(),
            input_price_per_1m: Some(1.0),
            cached_input_price_per_1m: Some(0.1),
            output_price_per_1m: Some(2.0),
            reasoning_output_price_per_1m: None,
            cache_write_5m_price_per_1m: None,
            cache_write_1h_price_per_1m: None,
            cache_hit_price_per_1m: None,
            long_context_threshold_tokens: None,
            long_context_input_price_per_1m: None,
            long_context_cached_input_price_per_1m: None,
            long_context_output_price_per_1m: None,
            source: source.to_string(),
            source_url: None,
            seed_version: None,
            enabled: true,
            priority: 20_000,
            created_at: now,
            updated_at: now,
        })
        .expect("seed model price rule");
}

fn model_catalog_record(slug: &str) -> ModelCatalogModelRecord {
    ModelCatalogModelRecord {
        scope: "default".to_string(),
        slug: slug.to_string(),
        display_name: slug.to_string(),
        source_kind: MODEL_SOURCE_KIND_REMOTE.to_string(),
        user_edited: false,
        description: None,
        default_reasoning_level: None,
        shell_type: None,
        visibility: Some("list".to_string()),
        supported_in_api: Some(true),
        priority: Some(0),
        availability_nux_json: None,
        upgrade_json: None,
        base_instructions: None,
        model_messages_json: None,
        supports_reasoning_summaries: None,
        default_reasoning_summary: None,
        support_verbosity: None,
        default_verbosity_json: None,
        apply_patch_tool_type: None,
        web_search_tool_type: None,
        truncation_mode: None,
        truncation_limit: None,
        truncation_extra_json: None,
        supports_parallel_tool_calls: None,
        supports_image_detail_original: None,
        context_window: None,
        auto_compact_token_limit: None,
        effective_context_window_percent: None,
        minimal_client_version_json: None,
        supports_search_tool: None,
        extra_json: "{}".to_string(),
        sort_index: 0,
        updated_at: now_ts(),
    }
}

#[test]
fn normalize_models_response_keeps_full_model_metadata() {
    let response = ModelsResponse {
        models: vec![
            serde_json::from_value(json!({
                "slug": "gpt-5",
                "display_name": "GPT-5",
                "supported_in_api": true,
                "visibility": "list",
                "supported_reasoning_levels": [
                    { "effort": "medium", "description": "balanced" }
                ]
            }))
            .expect("parse model"),
            ModelInfo {
                slug: " ".to_string(),
                display_name: String::new(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let normalized = normalize_models_response(response);
    assert_eq!(normalized.models.len(), 1);
    assert_eq!(normalized.models[0].slug, "gpt-5");
    assert_eq!(normalized.models[0].display_name, "GPT-5");
    assert!(normalized.models[0].supported_in_api);
    assert_eq!(normalized.models[0].supported_reasoning_levels.len(), 1);
}

#[test]
fn normalize_models_response_keeps_sparse_service_tiers() {
    let response = ModelsResponse {
        models: vec![serde_json::from_value(json!({
            "slug": "gpt-5.5",
            "display_name": "GPT-5.5",
            "supported_in_api": true,
            "visibility": "list",
            "service_tiers": [
                { "id": "flex" },
                { "id": "priority", "name": "Priority", "description": "Fast lane" }
            ],
            "default_service_tier": "flex",
            "upgrade_info": { "model": "gpt-5.5" }
        }))
        .expect("parse model")],
        ..Default::default()
    };

    let normalized = normalize_models_response(response);
    assert_eq!(normalized.models.len(), 1);
    assert_eq!(normalized.models[0].service_tiers.len(), 2);
    assert_eq!(normalized.models[0].service_tiers[0].id, "flex");
    assert_eq!(normalized.models[0].service_tiers[0].name, "flex");
    assert_eq!(normalized.models[0].service_tiers[0].description, "");
    assert_eq!(normalized.models[0].service_tiers[1].id, "priority");
    assert_eq!(normalized.models[0].service_tiers[1].name, "Priority");
    assert_eq!(
        normalized.models[0].default_service_tier.as_deref(),
        Some("flex")
    );
    assert_eq!(
        normalized.models[0]
            .upgrade_info
            .as_ref()
            .and_then(|value| value.get("model"))
            .and_then(Value::as_str),
        Some("gpt-5.5")
    );
}

#[test]
fn normalize_models_response_maps_hidden_visibility_to_hide() {
    let response = ModelsResponse {
        models: vec![serde_json::from_value(json!({
            "slug": "gpt-5.4-mini",
            "display_name": "GPT-5.4-Mini",
            "supported_in_api": true,
            "visibility": "hidden"
        }))
        .expect("parse model")],
        ..Default::default()
    };

    let normalized = normalize_models_response(response);
    assert_eq!(normalized.models.len(), 1);
    assert_eq!(normalized.models[0].visibility.as_deref(), Some("hide"));
}

#[test]
fn merge_models_response_updates_existing_without_removing_cached_fields() {
    let cached = ModelsResponse {
        models: vec![
            ModelInfo {
                slug: "gpt-5".to_string(),
                display_name: "GPT-5".to_string(),
                description: Some("cached description".to_string()),
                supported_in_api: true,
                priority: 200,
                input_modalities: vec!["text".to_string(), "image".to_string()],
                additional_speed_tiers: vec!["fast".to_string()],
                supported_reasoning_levels: vec![serde_json::from_value(json!({
                    "effort": "medium",
                    "description": "balanced"
                }))
                .expect("reasoning preset")],
                ..Default::default()
            },
            ModelInfo {
                slug: "gpt-legacy".to_string(),
                display_name: "GPT Legacy".to_string(),
                supported_in_api: true,
                ..Default::default()
            },
        ],
        extra: BTreeMap::from([("etag".to_string(), json!("cached"))]),
    };
    let incoming = ModelsResponse {
        models: vec![
            ModelInfo {
                slug: "gpt-5".to_string(),
                display_name: "GPT-5 New".to_string(),
                supported_in_api: false,
                supported_reasoning_levels: vec![serde_json::from_value(json!({
                    "effort": "high",
                    "description": "deeper"
                }))
                .expect("reasoning preset")],
                visibility: Some("list".to_string()),
                additional_speed_tiers: vec!["turbo".to_string()],
                ..Default::default()
            },
            ModelInfo {
                slug: "gpt-new".to_string(),
                display_name: "GPT New".to_string(),
                supported_in_api: true,
                ..Default::default()
            },
        ],
        extra: BTreeMap::from([("etag".to_string(), json!("fresh"))]),
    };

    let merged = merge_models_response(cached, incoming);
    assert_eq!(
        merged
            .models
            .iter()
            .map(|model| model.slug.as_str())
            .collect::<Vec<_>>(),
        vec!["gpt-5", "gpt-new", "gpt-legacy"]
    );
    assert_eq!(merged.models[0].display_name, "GPT-5 New");
    assert_eq!(
        merged.models[0].description.as_deref(),
        Some("cached description")
    );
    assert!(merged.models[0].supported_in_api);
    assert_eq!(merged.models[0].priority, 200);
    assert_eq!(
        merged.models[0].input_modalities,
        vec!["text".to_string(), "image".to_string()]
    );
    assert_eq!(
        merged.models[0].additional_speed_tiers,
        vec!["turbo".to_string(), "fast".to_string()]
    );
    assert_eq!(merged.models[0].supported_reasoning_levels.len(), 2);
    assert_eq!(
        merged.extra.get("etag").and_then(Value::as_str),
        Some("fresh")
    );
}

#[test]
fn normalize_models_response_keeps_codex_image_model_from_remote() {
    let response = ModelsResponse {
        models: vec![
            ModelInfo {
                slug: "gpt-5.4-mini".to_string(),
                display_name: "GPT-5.4 Mini".to_string(),
                supported_in_api: true,
                ..Default::default()
            },
            ModelInfo {
                slug: "gpt-image-2".to_string(),
                display_name: "GPT Image 2".to_string(),
                supported_in_api: true,
                ..Default::default()
            },
        ],
        extra: BTreeMap::from([("etag".to_string(), json!("cached"))]),
    };

    let normalized = normalize_models_response(response);

    assert_eq!(
        normalized
            .models
            .iter()
            .map(|model| model.slug.as_str())
            .collect::<Vec<_>>(),
        vec!["gpt-5.4-mini", "gpt-image-2"]
    );
    assert_eq!(
        normalized.extra.get("etag").and_then(Value::as_str),
        Some("cached")
    );
}

#[test]
fn normalize_managed_catalog_keeps_codex_image_model() {
    let catalog = ManagedModelCatalogResult {
        items: vec![
            ManagedModelCatalogEntry {
                model: ModelInfo {
                    slug: "gpt-5.4-mini".to_string(),
                    display_name: "GPT-5.4 Mini".to_string(),
                    supported_in_api: true,
                    ..Default::default()
                },
                source_kind: MODEL_SOURCE_KIND_REMOTE.to_string(),
                user_edited: false,
                sort_index: 0,
                updated_at: 123,
            },
            ManagedModelCatalogEntry {
                model: ModelInfo {
                    slug: "gpt-image-2".to_string(),
                    display_name: "GPT Image 2".to_string(),
                    supported_in_api: true,
                    ..Default::default()
                },
                source_kind: MODEL_SOURCE_KIND_CUSTOM.to_string(),
                user_edited: true,
                sort_index: 1,
                updated_at: 123,
            },
        ],
        extra: BTreeMap::from([("etag".to_string(), json!("cached"))]),
    };

    let normalized = normalize_managed_model_catalog(catalog);
    let response = managed_catalog_to_models_response(&normalized);

    assert_eq!(normalized.items.len(), 2);
    assert_eq!(normalized.items[0].model.slug, "gpt-5.4-mini");
    assert_eq!(normalized.items[1].model.slug, "gpt-image-2");
    assert_eq!(response.models.len(), 2);
    assert_eq!(response.models[0].slug, "gpt-5.4-mini");
    assert_eq!(response.models[1].slug, "gpt-image-2");
}

#[test]
fn auto_platform_model_keeps_codex_image_model_from_source() {
    let now = now_ts();
    let source_model = ModelSourceModel {
        source_kind: ROUTING_SOURCE_KIND_OPENAI_ACCOUNT.to_string(),
        source_id: "acc-image".to_string(),
        display_name: Some("GPT Image 2".to_string()),
        upstream_model: "gpt-image-2".to_string(),
        status: "available".to_string(),
        discovery_kind: "synced".to_string(),
        last_synced_at: Some(now),
        extra_json: "{}".to_string(),
        created_at: now,
        updated_at: now,
    };

    let model = auto_platform_model_from_source_model(&source_model).expect("auto platform model");
    assert_eq!(model.slug, "gpt-image-2");
}

#[test]
fn read_model_options_from_storage_reads_structured_catalog() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let payload = ModelsResponse {
        models: vec![serde_json::from_value(json!({
            "slug": "gpt-5.4",
            "display_name": "GPT-5.4",
            "description": "Latest frontier model",
            "supported_in_api": true,
            "supported_reasoning_levels": [
                { "effort": "medium", "description": "balanced" }
            ],
            "input_modalities": ["text", "image"],
            "available_in_plans": ["pro", "team"]
        }))
        .expect("parse model")],
        extra: BTreeMap::from([("etag".to_string(), json!("legacy"))]),
    };
    save_model_options_with_storage(&storage, &payload).expect("seed structured catalog");

    let response = read_model_options_from_storage(&storage).expect("read models");
    assert_eq!(response.models.len(), 1);
    assert_eq!(response.models[0].slug, "gpt-5.4");
    assert_eq!(
        response.extra.get("etag").and_then(Value::as_str),
        Some("legacy")
    );

    let scope = storage
        .get_model_catalog_scope("default")
        .expect("read scope")
        .expect("scope exists");
    assert_eq!(
        serde_json::from_str::<BTreeMap<String, Value>>(&scope.extra_json)
            .expect("parse scope extra")
            .get("etag")
            .and_then(Value::as_str),
        Some("legacy")
    );
    let models = storage
        .list_model_catalog_models("default")
        .expect("list model rows");
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].display_name, "GPT-5.4");
    assert_eq!(
        models[0].description.as_deref(),
        Some("Latest frontier model")
    );
    let reasoning_levels = storage
        .list_model_catalog_reasoning_levels("default")
        .expect("list reasoning levels");
    assert_eq!(reasoning_levels.len(), 1);
    assert_eq!(reasoning_levels[0].effort, "medium");
    let plans = storage
        .list_model_catalog_available_in_plans("default")
        .expect("list plans");
    assert_eq!(
        plans
            .iter()
            .map(|item| item.value.as_str())
            .collect::<Vec<_>>(),
        vec!["pro", "team"]
    );
}

#[test]
fn read_model_options_from_storage_keeps_sparse_service_tiers() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let mut row = model_catalog_record("gpt-5.5");
    row.extra_json = json!({
        "service_tiers": [
            { "id": "flex" },
            { "id": "priority", "name": "Priority", "description": "Fast lane" }
        ],
        "default_service_tier": "flex",
        "upgrade_info": {
            "model": "gpt-5.5"
        }
    })
    .to_string();
    storage
        .upsert_model_catalog_models(&[row])
        .expect("seed sparse service tiers");

    let response = read_model_options_from_storage(&storage).expect("read models");
    assert_eq!(response.models.len(), 1);
    assert_eq!(response.models[0].service_tiers.len(), 2);
    assert_eq!(response.models[0].service_tiers[0].id, "flex");
    assert_eq!(response.models[0].service_tiers[0].name, "flex");
    assert_eq!(
        response.models[0].default_service_tier.as_deref(),
        Some("flex")
    );
    assert_eq!(
        response.models[0]
            .upgrade_info
            .as_ref()
            .and_then(|value| value.get("model"))
            .and_then(Value::as_str),
        Some("gpt-5.5")
    );
}

#[test]
fn managed_catalog_save_prunes_stale_default_model_group_models() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let now = now_ts();
    storage
        .replace_model_group_models(
            "mg_default",
            &[ModelGroupModel {
                group_id: "mg_default".to_string(),
                platform_model_slug: "gpt-stale".to_string(),
                enabled: true,
                rate_multiplier_millis: None,
                billing_model_slug: None,
                note: Some("stale".to_string()),
                created_at: now,
                updated_at: now,
            }],
        )
        .expect("seed stale default model group model");

    seed_platform_catalog(&storage, &["gpt-current"]);

    let slugs = storage
        .list_model_group_models_for_group("mg_default")
        .expect("list default model group models")
        .into_iter()
        .map(|model| model.platform_model_slug)
        .collect::<Vec<_>>();
    assert_eq!(slugs, vec!["gpt-current"]);
}

#[test]
fn managed_catalog_save_preserves_catalog_rows_absent_from_payload() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    seed_platform_catalog(&storage, &["gpt-old"]);

    seed_platform_catalog(&storage, &["gpt-current"]);

    let mut catalog_slugs = storage
        .list_model_catalog_models("default")
        .expect("list catalog")
        .into_iter()
        .map(|model| model.slug)
        .collect::<Vec<_>>();
    catalog_slugs.sort();
    let mut group_slugs = storage
        .list_model_group_models_for_group("mg_default")
        .expect("list default model group models")
        .into_iter()
        .map(|model| model.platform_model_slug)
        .collect::<Vec<_>>();
    group_slugs.sort();
    assert_eq!(catalog_slugs, vec!["gpt-current", "gpt-old"]);
    assert_eq!(group_slugs, vec!["gpt-current", "gpt-old"]);
}

#[test]
fn delete_model_catalog_entry_removes_model_group_and_platform_source_routes() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    seed_platform_catalog(&storage, &["gpt-delete"]);
    storage
        .upsert_discovered_model_source_models(
            ROUTING_SOURCE_KIND_OPENAI_ACCOUNT,
            "acc-delete",
            &["gpt-delete".to_string()],
            "synced",
        )
        .expect("seed source model");
    let now = now_ts();
    storage
        .upsert_model_source_mapping(&ModelSourceMapping {
            id: "mapping-delete".to_string(),
            platform_model_slug: "gpt-delete".to_string(),
            source_kind: ROUTING_SOURCE_KIND_OPENAI_ACCOUNT.to_string(),
            source_id: "acc-delete".to_string(),
            upstream_model: "gpt-delete".to_string(),
            enabled: true,
            priority: 0,
            weight: 1,
            billing_model_slug: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed mapping");
    storage
        .upsert_model_source_mapping(&ModelSourceMapping {
            id: "mapping-wrapper".to_string(),
            platform_model_slug: "custom-wrapper".to_string(),
            source_kind: ROUTING_SOURCE_KIND_OPENAI_ACCOUNT.to_string(),
            source_id: "acc-delete".to_string(),
            upstream_model: "gpt-delete".to_string(),
            enabled: true,
            priority: 0,
            weight: 1,
            billing_model_slug: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed wrapper mapping");

    delete_model_catalog_entry(&storage, "gpt-delete", true).expect("delete catalog entry");

    assert!(storage
        .list_model_catalog_models("default")
        .expect("list catalog")
        .is_empty());
    assert!(storage
        .list_model_group_models_for_group("mg_default")
        .expect("list default model group models")
        .is_empty());
    let remaining_source_models = storage
        .list_model_source_models(Some(ROUTING_SOURCE_KIND_OPENAI_ACCOUNT), Some("acc-delete"))
        .expect("list source models")
        .into_iter()
        .map(|model| model.upstream_model)
        .collect::<Vec<_>>();
    assert_eq!(remaining_source_models, vec!["gpt-delete"]);
    assert!(storage
        .list_model_source_mappings(Some("gpt-delete"))
        .expect("list mappings")
        .is_empty());
    let remaining_mapping_slugs = storage
        .list_model_source_mappings(None)
        .expect("list all mappings")
        .into_iter()
        .map(|mapping| mapping.platform_model_slug)
        .collect::<Vec<_>>();
    assert_eq!(remaining_mapping_slugs, vec!["custom-wrapper"]);
}

#[test]
fn read_managed_catalog_preserves_existing_codex_image_model_rows() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    storage
        .upsert_model_catalog_models(&[model_catalog_record("gpt-image-2")])
        .expect("seed image catalog row");

    assert!(storage
        .list_model_catalog_models("default")
        .expect("list catalog before read")
        .iter()
        .any(|model| model.slug == "gpt-image-2"));
    assert!(storage
        .list_model_group_models_for_group("mg_default")
        .expect("list default model group models before read")
        .iter()
        .any(|model| model.platform_model_slug == "gpt-image-2"));

    let response = read_managed_model_catalog_from_storage(&storage).expect("read managed catalog");

    assert_eq!(response.items.len(), 1);
    assert_eq!(response.items[0].model.slug, "gpt-image-2");
    assert!(storage
        .list_model_catalog_models("default")
        .expect("list catalog after read")
        .iter()
        .any(|model| model.slug == "gpt-image-2"));
    assert!(storage
        .list_model_group_models_for_group("mg_default")
        .expect("list default model group models after read")
        .iter()
        .any(|model| model.platform_model_slug == "gpt-image-2"));
}

#[test]
fn managed_catalog_round_trip_preserves_source_kind_and_user_overrides() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let payload = ManagedModelCatalogResult {
        items: vec![ManagedModelCatalogEntry {
            model: serde_json::from_value(json!({
                "slug": "gpt-5.4",
                "display_name": "GPT-5.4 Custom",
                "description": "customized locally",
                "supported_in_api": true,
                "input_modalities": ["text", "image"],
                "service_tiers": [{
                    "id": "flex",
                    "name": "Flex",
                    "description": "Lower priority capacity."
                }],
                "default_service_tier": "flex",
                "upgrade_info": {
                    "model": "gpt-5.4",
                    "upgrade_copy": "Use this model for coding"
                }
            }))
            .expect("parse managed model"),
            source_kind: MODEL_SOURCE_KIND_CUSTOM.to_string(),
            user_edited: true,
            sort_index: 9,
            updated_at: 1_770_000_123,
        }],
        extra: BTreeMap::from([("etag".to_string(), json!("managed"))]),
    };

    save_managed_model_catalog_with_storage(&storage, &payload)
        .expect("save managed model catalog");

    let response =
        read_managed_model_catalog_from_storage(&storage).expect("read managed model catalog");
    assert_eq!(response.items.len(), 1);
    assert_eq!(response.items[0].model.slug, "gpt-5.4");
    assert_eq!(response.items[0].source_kind, MODEL_SOURCE_KIND_CUSTOM);
    assert!(response.items[0].user_edited);
    assert_eq!(response.items[0].sort_index, 9);
    assert_eq!(response.items[0].model.service_tiers.len(), 1);
    assert_eq!(response.items[0].model.service_tiers[0].id, "flex");
    assert_eq!(
        response.items[0].model.default_service_tier.as_deref(),
        Some("flex")
    );
    assert_eq!(
        response.items[0]
            .model
            .upgrade_info
            .as_ref()
            .and_then(|value| value.get("model"))
            .and_then(Value::as_str),
        Some("gpt-5.4")
    );
    assert_eq!(
        response.extra.get("etag").and_then(Value::as_str),
        Some("managed")
    );
}

#[test]
fn merge_managed_catalog_preserves_user_edited_entries_when_remote_refreshes() {
    let cached = ManagedModelCatalogResult {
        items: vec![ManagedModelCatalogEntry {
            model: serde_json::from_value(json!({
                "slug": "gpt-5.4",
                "display_name": "GPT-5.4 Local",
                "description": "keep local override",
                "supported_in_api": true
            }))
            .expect("parse cached model"),
            source_kind: MODEL_SOURCE_KIND_REMOTE.to_string(),
            user_edited: true,
            sort_index: 3,
            updated_at: 55,
        }],
        extra: BTreeMap::new(),
    };
    let incoming = ModelsResponse {
        models: vec![serde_json::from_value(json!({
            "slug": "gpt-5.4",
            "display_name": "GPT-5.4 Remote",
            "description": "remote version",
            "supported_in_api": true
        }))
        .expect("parse incoming model")],
        extra: BTreeMap::new(),
    };

    let merged = merge_managed_model_catalog(cached, incoming);
    assert_eq!(merged.items.len(), 1);
    assert_eq!(merged.items[0].model.display_name, "GPT-5.4 Local");
    assert_eq!(
        merged.items[0].model.description.as_deref(),
        Some("keep local override")
    );
    assert_eq!(merged.items[0].source_kind, MODEL_SOURCE_KIND_REMOTE);
    assert!(merged.items[0].user_edited);
}

#[test]
fn merge_managed_catalog_preserves_unedited_remote_entries_missing_from_remote() {
    let cached = ManagedModelCatalogResult {
        items: vec![
            ManagedModelCatalogEntry {
                model: serde_json::from_value(json!({
                    "slug": "gpt-stale",
                    "display_name": "GPT Stale",
                    "supported_in_api": true
                }))
                .expect("parse stale model"),
                source_kind: MODEL_SOURCE_KIND_REMOTE.to_string(),
                user_edited: false,
                sort_index: 0,
                updated_at: 10,
            },
            ManagedModelCatalogEntry {
                model: serde_json::from_value(json!({
                    "slug": "gpt-custom",
                    "display_name": "GPT Custom",
                    "supported_in_api": true
                }))
                .expect("parse custom model"),
                source_kind: MODEL_SOURCE_KIND_CUSTOM.to_string(),
                user_edited: false,
                sort_index: 1,
                updated_at: 11,
            },
        ],
        extra: BTreeMap::new(),
    };
    let incoming = ModelsResponse {
        models: vec![serde_json::from_value(json!({
            "slug": "gpt-current",
            "display_name": "GPT Current",
            "supported_in_api": true
        }))
        .expect("parse incoming model")],
        extra: BTreeMap::new(),
    };

    let merged = merge_managed_model_catalog(cached, incoming);

    assert_eq!(
        merged
            .items
            .iter()
            .map(|item| item.model.slug.as_str())
            .collect::<Vec<_>>(),
        vec!["gpt-current", "gpt-stale", "gpt-custom"]
    );
}

#[test]
fn prune_stale_remote_models_only_deletes_unedited_remote_entries() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let payload = ManagedModelCatalogResult {
        items: vec![
            ManagedModelCatalogEntry {
                model: serde_json::from_value(json!({
                    "slug": "gpt-current",
                    "display_name": "GPT Current",
                    "supported_in_api": true
                }))
                .expect("parse current model"),
                source_kind: MODEL_SOURCE_KIND_REMOTE.to_string(),
                user_edited: false,
                sort_index: 0,
                updated_at: 10,
            },
            ManagedModelCatalogEntry {
                model: serde_json::from_value(json!({
                    "slug": "gpt-stale-remote",
                    "display_name": "GPT Stale Remote",
                    "supported_in_api": true
                }))
                .expect("parse stale remote model"),
                source_kind: MODEL_SOURCE_KIND_REMOTE.to_string(),
                user_edited: false,
                sort_index: 1,
                updated_at: 11,
            },
            ManagedModelCatalogEntry {
                model: serde_json::from_value(json!({
                    "slug": "gpt-stale-edited",
                    "display_name": "GPT Stale Edited",
                    "supported_in_api": true
                }))
                .expect("parse stale edited model"),
                source_kind: MODEL_SOURCE_KIND_REMOTE.to_string(),
                user_edited: true,
                sort_index: 2,
                updated_at: 12,
            },
            ManagedModelCatalogEntry {
                model: serde_json::from_value(json!({
                    "slug": "gpt-custom",
                    "display_name": "GPT Custom",
                    "supported_in_api": true
                }))
                .expect("parse custom model"),
                source_kind: MODEL_SOURCE_KIND_CUSTOM.to_string(),
                user_edited: false,
                sort_index: 3,
                updated_at: 13,
            },
        ],
        extra: BTreeMap::new(),
    };
    save_managed_model_catalog_with_storage(&storage, &payload)
        .expect("seed managed model catalog");

    let remote = ModelsResponse {
        models: vec![serde_json::from_value(json!({
            "slug": "gpt-current",
            "display_name": "GPT Current",
            "supported_in_api": true
        }))
        .expect("parse remote model")],
        extra: BTreeMap::new(),
    };
    prune_unedited_remote_model_catalog_entries_missing_from_remote(&storage, &remote)
        .expect("prune stale remote models");

    let catalog = read_managed_model_catalog_from_storage(&storage).expect("read managed catalog");
    assert_eq!(
        catalog
            .items
            .iter()
            .map(|item| item.model.slug.as_str())
            .collect::<Vec<_>>(),
        vec!["gpt-current", "gpt-stale-edited", "gpt-custom"]
    );
}

#[test]
fn account_pool_bootstrap_links_catalog_models_on_first_load() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_test_account(&storage, "acc-auto");
    seed_platform_catalog(&storage, &["gpt-auto"]);

    bootstrap_account_pool_model_routes(&storage, false).expect("bootstrap account routes");

    let source_models = storage
        .list_model_source_models(Some(ROUTING_SOURCE_KIND_OPENAI_ACCOUNT), Some("acc-auto"))
        .expect("list source models");
    assert!(source_models
        .iter()
        .any(|model| model.upstream_model == "gpt-auto"));

    let mappings = storage
        .list_enabled_model_source_mappings_for_platform("gpt-auto")
        .expect("list mappings");
    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[0].source_kind, ROUTING_SOURCE_KIND_OPENAI_ACCOUNT);
    assert_eq!(mappings[0].source_id, "acc-auto");
    assert_eq!(mappings[0].upstream_model, "gpt-auto");
}

#[test]
fn account_pool_bootstrap_links_new_account_after_initial_sync() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_test_account(&storage, "acc-first");
    seed_platform_catalog(&storage, &["gpt-auto"]);

    bootstrap_account_pool_model_routes(&storage, false).expect("bootstrap first account");
    insert_test_account(&storage, "acc-second");
    bootstrap_account_pool_model_routes(&storage, false).expect("bootstrap second account");

    let mut source_ids = storage
        .list_enabled_model_source_mappings_for_platform("gpt-auto")
        .expect("list mappings")
        .into_iter()
        .map(|mapping| mapping.source_id)
        .collect::<Vec<_>>();
    source_ids.sort();
    assert_eq!(
        source_ids,
        vec!["acc-first".to_string(), "acc-second".to_string()]
    );
}

#[test]
fn active_openai_account_sources_reads_only_requested_account() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_test_account(&storage, "acc-target");
    insert_test_account(&storage, "acc-other");

    let accounts = active_openai_account_sources(&storage, Some("acc-target"))
        .expect("read requested account source");

    assert_eq!(accounts, vec!["acc-target".to_string()]);
}

#[test]
fn account_pool_bootstrap_skips_disabled_accounts() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_test_account(&storage, "acc-active");
    insert_test_account(&storage, "acc-disabled");
    storage
        .update_account_status("acc-disabled", "disabled")
        .expect("disable account");
    seed_platform_catalog(&storage, &["gpt-active-only"]);

    bootstrap_account_pool_model_routes(&storage, false).expect("bootstrap account routes");

    let mappings = storage
        .list_enabled_model_source_mappings_for_platform("gpt-active-only")
        .expect("list mappings");
    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[0].source_id, "acc-active");
}

#[test]
fn account_pool_bootstrap_fills_missing_mappings_for_existing_source() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_test_account(&storage, "acc-expand");
    seed_platform_catalog(&storage, &["gpt-old"]);

    bootstrap_account_pool_model_routes(&storage, false).expect("bootstrap initial catalog");
    seed_platform_catalog(&storage, &["gpt-old", "gpt-new"]);
    bootstrap_account_pool_model_routes(&storage, false).expect("bootstrap expanded catalog");

    let mappings = storage
        .list_enabled_model_source_mappings_for_platform("gpt-new")
        .expect("list new mappings");
    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[0].source_kind, ROUTING_SOURCE_KIND_OPENAI_ACCOUNT);
    assert_eq!(mappings[0].source_id, "acc-expand");
    assert_eq!(mappings[0].upstream_model, "gpt-new");
}

#[test]
fn account_pool_bootstrap_prunes_stale_account_source_routes() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    seed_platform_catalog(&storage, &["gpt-stale"]);
    storage
        .upsert_discovered_model_source_models(
            ROUTING_SOURCE_KIND_OPENAI_ACCOUNT,
            "acc-stale",
            &["gpt-stale".to_string()],
            "synced",
        )
        .expect("seed stale source model");
    let now = now_ts();
    storage
        .upsert_model_source_mapping(&ModelSourceMapping {
            id: "mapping-stale".to_string(),
            platform_model_slug: "gpt-stale".to_string(),
            source_kind: ROUTING_SOURCE_KIND_OPENAI_ACCOUNT.to_string(),
            source_id: "acc-stale".to_string(),
            upstream_model: "gpt-stale".to_string(),
            enabled: true,
            priority: 0,
            weight: 1,
            billing_model_slug: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed stale mapping");

    bootstrap_account_pool_model_routes(&storage, false).expect("bootstrap account routes");

    assert!(storage
        .list_model_source_models(Some(ROUTING_SOURCE_KIND_OPENAI_ACCOUNT), Some("acc-stale"))
        .expect("list source models")
        .is_empty());
    assert!(storage
        .list_model_source_mappings(Some("gpt-stale"))
        .expect("list mappings")
        .is_empty());
}

#[test]
fn account_pool_auto_association_creates_missing_platform_model() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    storage
        .upsert_discovered_model_source_models(
            ROUTING_SOURCE_KIND_OPENAI_ACCOUNT,
            "acc-source",
            &["vendor-new".to_string()],
            "synced",
        )
        .expect("seed source model");

    auto_associate_source_models(
        &storage,
        ROUTING_SOURCE_KIND_OPENAI_ACCOUNT,
        "acc-source",
        true,
    )
    .expect("auto associate");

    let catalog = read_managed_model_catalog_from_storage(&storage).expect("read platform catalog");
    assert!(catalog
        .items
        .iter()
        .any(|item| item.model.slug == "vendor-new" && item.model.supported_in_api));

    let mappings = storage
        .list_enabled_model_source_mappings_for_platform("vendor-new")
        .expect("list mappings");
    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[0].source_kind, ROUTING_SOURCE_KIND_OPENAI_ACCOUNT);
    assert_eq!(mappings[0].source_id, "acc-source");
}

#[test]
fn auto_association_uses_only_available_non_empty_source_models() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let now = now_ts();
    for (upstream_model, status) in [
        ("vendor-available", "available"),
        ("vendor-disabled", "disabled"),
        ("   ", "available"),
    ] {
        storage
            .upsert_model_source_model(&ModelSourceModel {
                source_kind: ROUTING_SOURCE_KIND_OPENAI_ACCOUNT.to_string(),
                source_id: "acc-filtered".to_string(),
                upstream_model: upstream_model.to_string(),
                display_name: Some(upstream_model.to_string()),
                status: status.to_string(),
                discovery_kind: "manual".to_string(),
                last_synced_at: None,
                extra_json: "{}".to_string(),
                created_at: now,
                updated_at: now,
            })
            .expect("seed source model");
    }

    auto_associate_source_models(
        &storage,
        ROUTING_SOURCE_KIND_OPENAI_ACCOUNT,
        "acc-filtered",
        true,
    )
    .expect("auto associate");

    assert_eq!(
        storage
            .list_enabled_model_source_mappings_for_platform("vendor-available")
            .expect("list available mappings")
            .len(),
        1
    );
    assert!(storage
        .list_enabled_model_source_mappings_for_platform("vendor-disabled")
        .expect("list disabled mappings")
        .is_empty());
    let catalog = read_managed_model_catalog_from_storage(&storage).expect("read platform catalog");
    assert!(catalog
        .items
        .iter()
        .any(|item| item.model.slug == "vendor-available"));
    assert!(!catalog
        .items
        .iter()
        .any(|item| item.model.slug == "vendor-disabled"));
}

#[test]
fn aggregate_auto_association_creates_missing_platform_models() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    seed_platform_catalog(&storage, &["gpt-known"]);
    storage
        .upsert_discovered_model_source_models(
            ROUTING_SOURCE_KIND_AGGREGATE_API,
            "agg-1",
            &["gpt-known".to_string(), "vendor-only".to_string()],
            "synced",
        )
        .expect("seed aggregate source models");

    auto_associate_source_models(&storage, ROUTING_SOURCE_KIND_AGGREGATE_API, "agg-1", true)
        .expect("auto associate");

    let mappings = storage
        .list_enabled_model_source_mappings_for_platform("gpt-known")
        .expect("list known mappings");
    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[0].source_kind, ROUTING_SOURCE_KIND_AGGREGATE_API);
    assert_eq!(mappings[0].source_id, "agg-1");
    assert_eq!(mappings[0].upstream_model, "gpt-known");

    let catalog = read_managed_model_catalog_from_storage(&storage).expect("read platform catalog");
    assert!(catalog
        .items
        .iter()
        .any(|item| item.model.slug == "vendor-only" && item.model.supported_in_api));
    let vendor_mappings = storage
        .list_enabled_model_source_mappings_for_platform("vendor-only")
        .expect("list vendor mappings");
    assert_eq!(vendor_mappings.len(), 1);
    assert_eq!(
        vendor_mappings[0].source_kind,
        ROUTING_SOURCE_KIND_AGGREGATE_API
    );
    assert_eq!(vendor_mappings[0].source_id, "agg-1");
    assert_eq!(vendor_mappings[0].upstream_model, "vendor-only");
}

#[test]
fn aggregate_auto_association_skips_price_rule_when_enabled_pattern_exists() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    seed_model_price_rule(
        &storage,
        "official-vendor-priced",
        "VENDOR-PRICED",
        "official_seed",
    );
    storage
        .upsert_discovered_model_source_models(
            ROUTING_SOURCE_KIND_AGGREGATE_API,
            "agg-priced",
            &["vendor-priced".to_string()],
            "synced",
        )
        .expect("seed aggregate source models");

    auto_associate_source_models(
        &storage,
        ROUTING_SOURCE_KIND_AGGREGATE_API,
        "agg-priced",
        true,
    )
    .expect("auto associate");

    let price_rules = storage
        .list_enabled_model_price_rules()
        .expect("list price rules");
    assert!(price_rules
        .iter()
        .any(|rule| rule.id == "official-vendor-priced"));
    assert!(!price_rules.iter().any(|rule| {
        rule.source == "aggregate_api_sync"
            && rule.model_pattern.eq_ignore_ascii_case("vendor-priced")
    }));
}

#[test]
fn aggregate_route_bootstrap_links_existing_source_models() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_test_aggregate_api(&storage, "agg-bootstrap", "active");
    storage
        .upsert_discovered_model_source_models(
            ROUTING_SOURCE_KIND_AGGREGATE_API,
            "agg-bootstrap",
            &["vendor-bootstrap".to_string()],
            "synced",
        )
        .expect("seed aggregate source model");

    bootstrap_aggregate_api_model_routes(&storage).expect("bootstrap aggregate routes");

    let catalog = read_managed_model_catalog_from_storage(&storage).expect("read platform catalog");
    assert!(catalog
        .items
        .iter()
        .any(|item| item.model.slug == "vendor-bootstrap"));
    let mappings = storage
        .list_enabled_model_source_mappings_for_platform("vendor-bootstrap")
        .expect("list mappings");
    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[0].source_kind, ROUTING_SOURCE_KIND_AGGREGATE_API);
    assert_eq!(mappings[0].source_id, "agg-bootstrap");
}

#[test]
fn managed_model_routing_read_bootstraps_aggregate_sources() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_test_aggregate_api(&storage, "agg-routing", "active");
    storage
        .upsert_discovered_model_source_models(
            ROUTING_SOURCE_KIND_AGGREGATE_API,
            "agg-routing",
            &["vendor-routing".to_string()],
            "synced",
        )
        .expect("seed aggregate source model");

    let result = read_managed_model_routing_from_storage(&storage, false).expect("read routing");

    assert!(result
        .source_models
        .iter()
        .any(|item| item.source_kind == ROUTING_SOURCE_KIND_AGGREGATE_API
            && item.source_id == "agg-routing"
            && item.upstream_model == "vendor-routing"));
    assert!(result.mappings.iter().any(|item| item.source_kind
        == ROUTING_SOURCE_KIND_AGGREGATE_API
        && item.source_id == "agg-routing"
        && item.platform_model_slug == "vendor-routing"
        && item.upstream_model == "vendor-routing"
        && item.enabled));
}

#[test]
fn aggregate_source_sync_creates_platform_models_and_mappings() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_test_aggregate_api(&storage, "agg-sync", "active");

    sync_aggregate_api_source_models_with_discovery(&storage, Some("agg-sync"), |source_id| {
        assert_eq!(source_id, "agg-sync");
        Ok(vec!["vendor-sync".to_string()])
    })
    .expect("sync aggregate models");

    let catalog = read_managed_model_catalog_from_storage(&storage).expect("read platform catalog");
    assert!(catalog
        .items
        .iter()
        .any(|item| item.model.slug == "vendor-sync" && item.model.supported_in_api));
    let mappings = storage
        .list_enabled_model_source_mappings_for_platform("vendor-sync")
        .expect("list mappings");
    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[0].source_kind, ROUTING_SOURCE_KIND_AGGREGATE_API);
    assert_eq!(mappings[0].source_id, "agg-sync");
}

#[test]
fn aggregate_supplier_import_association_creates_platform_route() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_test_aggregate_api(&storage, "agg-template", "active");
    storage
        .upsert_discovered_model_source_models(
            ROUTING_SOURCE_KIND_AGGREGATE_API,
            "agg-template",
            &["vendor-template".to_string()],
            "template",
        )
        .expect("seed template source model");

    auto_associate_aggregate_api_source_models(&storage, "agg-template")
        .expect("associate template import");

    let catalog = read_managed_model_catalog_from_storage(&storage).expect("read platform catalog");
    assert!(catalog
        .items
        .iter()
        .any(|item| item.model.slug == "vendor-template"));
    let mappings = storage
        .list_enabled_model_source_mappings_for_platform("vendor-template")
        .expect("list mappings");
    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[0].source_kind, ROUTING_SOURCE_KIND_AGGREGATE_API);
    assert_eq!(mappings[0].source_id, "agg-template");
}

#[test]
fn aggregate_bootstrap_preserves_disabled_mapping_state() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_test_aggregate_api(&storage, "agg-disabled-mapping", "active");
    storage
        .upsert_discovered_model_source_models(
            ROUTING_SOURCE_KIND_AGGREGATE_API,
            "agg-disabled-mapping",
            &["vendor-disabled".to_string()],
            "synced",
        )
        .expect("seed aggregate source model");
    let now = now_ts();
    storage
        .upsert_model_source_mapping(&ModelSourceMapping {
            id: "mapping-aggregate-disabled".to_string(),
            platform_model_slug: "vendor-disabled".to_string(),
            source_kind: ROUTING_SOURCE_KIND_AGGREGATE_API.to_string(),
            source_id: "agg-disabled-mapping".to_string(),
            upstream_model: "vendor-disabled".to_string(),
            enabled: false,
            priority: 0,
            weight: 1,
            billing_model_slug: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed disabled mapping");

    bootstrap_aggregate_api_model_routes(&storage).expect("bootstrap aggregate routes");

    let mappings = storage
        .list_model_source_mappings(Some("vendor-disabled"))
        .expect("list mappings");
    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[0].id, "mapping-aggregate-disabled");
    assert!(!mappings[0].enabled);
}

#[test]
fn aggregate_bootstrap_preserves_disabled_source_routes() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_test_aggregate_api(&storage, "agg-stale", "disabled");
    seed_platform_catalog(&storage, &["vendor-stale"]);
    storage
        .upsert_discovered_model_source_models(
            ROUTING_SOURCE_KIND_AGGREGATE_API,
            "agg-stale",
            &["vendor-stale".to_string()],
            "synced",
        )
        .expect("seed aggregate source model");
    let now = now_ts();
    storage
        .upsert_model_source_mapping(&ModelSourceMapping {
            id: "mapping-aggregate-stale".to_string(),
            platform_model_slug: "vendor-stale".to_string(),
            source_kind: ROUTING_SOURCE_KIND_AGGREGATE_API.to_string(),
            source_id: "agg-stale".to_string(),
            upstream_model: "vendor-stale".to_string(),
            enabled: true,
            priority: 0,
            weight: 1,
            billing_model_slug: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed stale mapping");
    storage
        .upsert_model_source_mapping_preference(
            ROUTING_SOURCE_KIND_AGGREGATE_API,
            "agg-stale",
            "vendor-stale",
            PREF_UNLINKED,
        )
        .expect("seed preference");

    bootstrap_aggregate_api_model_routes(&storage).expect("bootstrap aggregate routes");

    assert_eq!(
        storage
            .list_model_source_models(Some(ROUTING_SOURCE_KIND_AGGREGATE_API), Some("agg-stale"))
            .expect("list source models")
            .len(),
        1
    );
    assert_eq!(
        storage
            .list_model_source_mappings(Some("vendor-stale"))
            .expect("list mappings")
            .len(),
        1
    );
    assert_eq!(
        storage
            .list_model_source_mapping_preferences(ROUTING_SOURCE_KIND_AGGREGATE_API, "agg-stale",)
            .expect("list preferences")
            .len(),
        1
    );
}

#[test]
fn aggregate_bootstrap_prunes_deleted_source_routes() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    seed_platform_catalog(&storage, &["vendor-stale"]);
    storage
        .upsert_discovered_model_source_models(
            ROUTING_SOURCE_KIND_AGGREGATE_API,
            "agg-stale",
            &["vendor-stale".to_string()],
            "synced",
        )
        .expect("seed aggregate source model");
    let now = now_ts();
    storage
        .upsert_model_source_mapping(&ModelSourceMapping {
            id: "mapping-aggregate-stale".to_string(),
            platform_model_slug: "vendor-stale".to_string(),
            source_kind: ROUTING_SOURCE_KIND_AGGREGATE_API.to_string(),
            source_id: "agg-stale".to_string(),
            upstream_model: "vendor-stale".to_string(),
            enabled: true,
            priority: 0,
            weight: 1,
            billing_model_slug: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed stale mapping");

    bootstrap_aggregate_api_model_routes(&storage).expect("bootstrap aggregate routes");

    assert!(storage
        .list_model_source_models(Some(ROUTING_SOURCE_KIND_AGGREGATE_API), Some("agg-stale"))
        .expect("list source models")
        .is_empty());
    assert!(storage
        .list_model_source_mappings(Some("vendor-stale"))
        .expect("list mappings")
        .is_empty());
}

#[test]
fn bootstrap_aggregate_routes_cleans_orphan_auto_catalog_model() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let now = now_ts();
    storage
        .upsert_model_source_model(&ModelSourceModel {
            source_kind: ROUTING_SOURCE_KIND_AGGREGATE_API.to_string(),
            source_id: "agg-orphan".to_string(),
            upstream_model: "orphan-agg-model".to_string(),
            display_name: Some("Orphan Aggregate Model".to_string()),
            status: "available".to_string(),
            discovery_kind: "synced".to_string(),
            last_synced_at: Some(now),
            extra_json: "{}".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("seed source model");
    save_managed_model_catalog_with_storage(
        &storage,
        &ManagedModelCatalogResult {
            items: vec![ManagedModelCatalogEntry {
                model: ModelInfo {
                    slug: "orphan-agg-model".to_string(),
                    display_name: "Orphan Aggregate Model".to_string(),
                    supported_in_api: true,
                    visibility: Some("list".to_string()),
                    input_modalities: vec!["text".to_string()],
                    ..Default::default()
                },
                source_kind: MODEL_SOURCE_KIND_REMOTE.to_string(),
                user_edited: false,
                sort_index: 0,
                updated_at: now,
            }],
            extra: Default::default(),
        },
    )
    .expect("seed catalog model");

    bootstrap_aggregate_api_model_routes(&storage).expect("bootstrap aggregate routes");

    let source_models = storage
        .list_model_source_models(Some(ROUTING_SOURCE_KIND_AGGREGATE_API), Some("agg-orphan"))
        .expect("list source models");
    assert!(source_models.is_empty());

    let catalog_after =
        read_managed_model_catalog_from_storage(&storage).expect("read catalog after bootstrap");
    assert!(!catalog_after
        .items
        .iter()
        .any(|item| item.model.slug == "orphan-agg-model"));
}

#[test]
fn bootstrap_aggregate_routes_keeps_unrelated_remote_catalog_model() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let now = now_ts();
    storage
        .upsert_model_source_model(&ModelSourceModel {
            source_kind: ROUTING_SOURCE_KIND_AGGREGATE_API.to_string(),
            source_id: "agg-orphan".to_string(),
            upstream_model: "orphan-agg-model".to_string(),
            display_name: Some("Orphan Aggregate Model".to_string()),
            status: "available".to_string(),
            discovery_kind: "synced".to_string(),
            last_synced_at: Some(now),
            extra_json: "{}".to_string(),
            created_at: now,
            updated_at: now,
        })
        .expect("seed source model");
    save_managed_model_catalog_with_storage(
        &storage,
        &ManagedModelCatalogResult {
            items: vec![
                ManagedModelCatalogEntry {
                    model: ModelInfo {
                        slug: "orphan-agg-model".to_string(),
                        display_name: "Orphan Aggregate Model".to_string(),
                        supported_in_api: true,
                        visibility: Some("list".to_string()),
                        input_modalities: vec!["text".to_string()],
                        ..Default::default()
                    },
                    source_kind: MODEL_SOURCE_KIND_REMOTE.to_string(),
                    user_edited: false,
                    sort_index: 0,
                    updated_at: now,
                },
                ManagedModelCatalogEntry {
                    model: ModelInfo {
                        slug: "unrelated-remote-model".to_string(),
                        display_name: "Unrelated Remote Model".to_string(),
                        supported_in_api: true,
                        visibility: Some("list".to_string()),
                        input_modalities: vec!["text".to_string()],
                        ..Default::default()
                    },
                    source_kind: MODEL_SOURCE_KIND_REMOTE.to_string(),
                    user_edited: false,
                    sort_index: 1,
                    updated_at: now,
                },
            ],
            extra: Default::default(),
        },
    )
    .expect("seed catalog models");

    bootstrap_aggregate_api_model_routes(&storage).expect("bootstrap aggregate routes");

    let catalog_after =
        read_managed_model_catalog_from_storage(&storage).expect("read catalog after bootstrap");
    assert!(catalog_after
        .items
        .iter()
        .any(|item| item.model.slug == "unrelated-remote-model"));
    assert!(!catalog_after
        .items
        .iter()
        .any(|item| item.model.slug == "orphan-agg-model"));
}

#[test]
fn aggregate_source_sync_cleans_orphan_auto_catalog_model_when_model_disappears() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    insert_test_aggregate_api(&storage, "agg-changing", "active");

    sync_aggregate_api_source_models_with_discovery(&storage, Some("agg-changing"), |source_id| {
        assert_eq!(source_id, "agg-changing");
        Ok(vec!["vendor-old".to_string()])
    })
    .expect("sync initial aggregate models");

    let initial_catalog =
        read_managed_model_catalog_from_storage(&storage).expect("read initial catalog");
    assert!(initial_catalog
        .items
        .iter()
        .any(|item| item.model.slug == "vendor-old"));
    assert_eq!(
        storage
            .list_enabled_model_source_mappings_for_platform("vendor-old")
            .expect("list initial mapping")
            .len(),
        1
    );

    sync_aggregate_api_source_models_with_discovery(&storage, Some("agg-changing"), |source_id| {
        assert_eq!(source_id, "agg-changing");
        Ok(vec!["vendor-new".to_string()])
    })
    .expect("sync changed aggregate models");

    let source_models = storage
        .list_model_source_models(
            Some(ROUTING_SOURCE_KIND_AGGREGATE_API),
            Some("agg-changing"),
        )
        .expect("list source models");
    assert!(!source_models
        .iter()
        .any(|item| item.upstream_model == "vendor-old"));
    assert!(source_models
        .iter()
        .any(|item| item.upstream_model == "vendor-new"));
    assert!(storage
        .list_model_source_mappings(Some("vendor-old"))
        .expect("list old mappings")
        .is_empty());

    let catalog_after =
        read_managed_model_catalog_from_storage(&storage).expect("read catalog after sync");
    assert!(!catalog_after
        .items
        .iter()
        .any(|item| item.model.slug == "vendor-old"));
    assert!(catalog_after
        .items
        .iter()
        .any(|item| item.model.slug == "vendor-new"));
    let new_mappings = storage
        .list_enabled_model_source_mappings_for_platform("vendor-new")
        .expect("list new mappings");
    assert_eq!(new_mappings.len(), 1);
    assert_eq!(new_mappings[0].source_id, "agg-changing");
}

#[test]
fn aggregate_source_sync_rejects_disabled_api() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    storage
        .insert_aggregate_api(&codexmanager_core::storage::AggregateApi {
            id: "agg-disabled".to_string(),
            provider_type: "codex".to_string(),
            supplier_name: Some("disabled".to_string()),
            sort: 0,
            url: "https://disabled.example/v1".to_string(),
            auth_type: "apikey".to_string(),
            auth_params_json: None,
            action: None,
            model_override: None,
            status: "disabled".to_string(),
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
        })
        .expect("insert disabled aggregate api");

    let err = sync_aggregate_api_source_models(&storage, Some("agg-disabled"))
        .expect_err("disabled aggregate api should not sync");
    assert!(err.contains("agg-disabled"));
    assert!(err.contains("disabled"));
}

#[test]
fn auto_association_preserves_existing_source_mapping_state() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    seed_platform_catalog(&storage, &["gpt-disabled"]);
    storage
        .upsert_discovered_model_source_models(
            ROUTING_SOURCE_KIND_OPENAI_ACCOUNT,
            "acc-disabled",
            &["gpt-disabled".to_string()],
            "synced",
        )
        .expect("seed source model");
    let now = now_ts();
    storage
        .upsert_model_source_mapping(&ModelSourceMapping {
            id: "mapping-disabled".to_string(),
            platform_model_slug: "gpt-disabled".to_string(),
            source_kind: ROUTING_SOURCE_KIND_OPENAI_ACCOUNT.to_string(),
            source_id: "acc-disabled".to_string(),
            upstream_model: "gpt-disabled".to_string(),
            enabled: false,
            priority: 0,
            weight: 1,
            billing_model_slug: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed disabled mapping");

    auto_associate_source_models(
        &storage,
        ROUTING_SOURCE_KIND_OPENAI_ACCOUNT,
        "acc-disabled",
        true,
    )
    .expect("auto associate");

    let mappings = storage
        .list_model_source_mappings(Some("gpt-disabled"))
        .expect("list mappings");
    assert_eq!(mappings.len(), 1);
    assert!(!mappings[0].enabled);
    assert_eq!(mappings[0].id, "mapping-disabled");
}

#[test]
fn auto_association_preserves_existing_platform_mapping_override() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    seed_platform_catalog(&storage, &["gpt-platform"]);
    storage
        .upsert_discovered_model_source_models(
            ROUTING_SOURCE_KIND_OPENAI_ACCOUNT,
            "acc-override",
            &["gpt-upstream".to_string(), "gpt-platform".to_string()],
            "synced",
        )
        .expect("seed source models");
    let now = now_ts();
    storage
        .upsert_model_source_mapping(&ModelSourceMapping {
            id: "mapping-override".to_string(),
            platform_model_slug: "gpt-platform".to_string(),
            source_kind: ROUTING_SOURCE_KIND_OPENAI_ACCOUNT.to_string(),
            source_id: "acc-override".to_string(),
            upstream_model: "gpt-upstream".to_string(),
            enabled: true,
            priority: 0,
            weight: 1,
            billing_model_slug: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed mapping override");

    auto_associate_source_models(
        &storage,
        ROUTING_SOURCE_KIND_OPENAI_ACCOUNT,
        "acc-override",
        true,
    )
    .expect("auto associate");

    let mappings = storage
        .list_model_source_mappings(Some("gpt-platform"))
        .expect("list mappings");
    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[0].id, "mapping-override");
    assert_eq!(mappings[0].upstream_model, "gpt-upstream");
}

#[test]
fn delete_catalog_entry_sets_unlinked_preference_and_prevents_auto_recreation() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    seed_platform_catalog(&storage, &["gpt-unlink"]);
    insert_test_aggregate_api(&storage, "agg-unlink", "active");
    storage
        .upsert_discovered_model_source_models(
            ROUTING_SOURCE_KIND_AGGREGATE_API,
            "agg-unlink",
            &["gpt-unlink".to_string()],
            "synced",
        )
        .expect("seed aggregate source model");
    let now = now_ts();
    storage
        .upsert_model_source_mapping(&ModelSourceMapping {
            id: "mapping-unlink".to_string(),
            platform_model_slug: "gpt-unlink".to_string(),
            source_kind: ROUTING_SOURCE_KIND_AGGREGATE_API.to_string(),
            source_id: "agg-unlink".to_string(),
            upstream_model: "gpt-unlink".to_string(),
            enabled: true,
            priority: 0,
            weight: 1,
            billing_model_slug: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed mapping");

    delete_model_catalog_entry(&storage, "gpt-unlink", true).expect("delete catalog entry");

    assert!(storage
        .list_model_catalog_models("default")
        .expect("list catalog")
        .is_empty());
    assert!(storage
        .list_model_source_mappings(Some("gpt-unlink"))
        .expect("list mappings")
        .is_empty());

    let prefs: Vec<_> = storage
        .list_model_source_mapping_preferences(ROUTING_SOURCE_KIND_AGGREGATE_API, "agg-unlink")
        .expect("list preferences")
        .into_iter()
        .filter(|p| p.upstream_model == "gpt-unlink")
        .collect();
    assert_eq!(prefs.len(), 1);
    assert_eq!(prefs[0].preference, PREF_UNLINKED);

    auto_associate_source_models(
        &storage,
        ROUTING_SOURCE_KIND_AGGREGATE_API,
        "agg-unlink",
        true,
    )
    .expect("auto associate after delete");

    let catalog_after =
        read_managed_model_catalog_from_storage(&storage).expect("read catalog after associate");
    assert!(
        !catalog_after
            .items
            .iter()
            .any(|item| item.model.slug == "gpt-unlink"),
        "deleted model should not be recreated by auto_associate after unlink"
    );
    assert!(storage
        .list_model_source_mappings(Some("gpt-unlink"))
        .expect("list mappings after associate")
        .is_empty());
}

#[test]
fn delete_catalog_entry_sets_unlinked_preference_for_multiple_sources() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    seed_platform_catalog(&storage, &["gpt-multi"]);
    insert_test_account(&storage, "acc-multi");
    insert_test_aggregate_api(&storage, "agg-multi", "active");
    storage
        .upsert_discovered_model_source_models(
            ROUTING_SOURCE_KIND_OPENAI_ACCOUNT,
            "acc-multi",
            &["gpt-multi".to_string()],
            "synced",
        )
        .expect("seed account source model");
    storage
        .upsert_discovered_model_source_models(
            ROUTING_SOURCE_KIND_AGGREGATE_API,
            "agg-multi",
            &["gpt-multi".to_string()],
            "synced",
        )
        .expect("seed aggregate source model");
    let now = now_ts();
    storage
        .upsert_model_source_mapping(&ModelSourceMapping {
            id: "mapping-multi-account".to_string(),
            platform_model_slug: "gpt-multi".to_string(),
            source_kind: ROUTING_SOURCE_KIND_OPENAI_ACCOUNT.to_string(),
            source_id: "acc-multi".to_string(),
            upstream_model: "gpt-multi".to_string(),
            enabled: true,
            priority: 0,
            weight: 1,
            billing_model_slug: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed account mapping");
    storage
        .upsert_model_source_mapping(&ModelSourceMapping {
            id: "mapping-multi-aggregate".to_string(),
            platform_model_slug: "gpt-multi".to_string(),
            source_kind: ROUTING_SOURCE_KIND_AGGREGATE_API.to_string(),
            source_id: "agg-multi".to_string(),
            upstream_model: "gpt-multi".to_string(),
            enabled: true,
            priority: 0,
            weight: 1,
            billing_model_slug: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed aggregate mapping");

    delete_model_catalog_entry(&storage, "gpt-multi", true).expect("delete catalog entry");

    assert!(storage
        .list_model_catalog_models("default")
        .expect("list catalog")
        .is_empty());
    assert!(storage
        .list_model_source_mappings(Some("gpt-multi"))
        .expect("list mappings")
        .is_empty());

    let account_prefs: Vec<_> = storage
        .list_model_source_mapping_preferences(ROUTING_SOURCE_KIND_OPENAI_ACCOUNT, "acc-multi")
        .expect("list account preferences")
        .into_iter()
        .filter(|p| p.upstream_model == "gpt-multi")
        .collect();
    assert_eq!(account_prefs.len(), 1);
    assert_eq!(account_prefs[0].preference, PREF_UNLINKED);

    let aggregate_prefs: Vec<_> = storage
        .list_model_source_mapping_preferences(ROUTING_SOURCE_KIND_AGGREGATE_API, "agg-multi")
        .expect("list aggregate preferences")
        .into_iter()
        .filter(|p| p.upstream_model == "gpt-multi")
        .collect();
    assert_eq!(aggregate_prefs.len(), 1);
    assert_eq!(aggregate_prefs[0].preference, PREF_UNLINKED);

    auto_associate_source_models(
        &storage,
        ROUTING_SOURCE_KIND_OPENAI_ACCOUNT,
        "acc-multi",
        true,
    )
    .expect("auto associate account after delete");
    auto_associate_source_models(
        &storage,
        ROUTING_SOURCE_KIND_AGGREGATE_API,
        "agg-multi",
        true,
    )
    .expect("auto associate aggregate after delete");

    assert!(storage
        .list_model_source_mappings(Some("gpt-multi"))
        .expect("list mappings after associate")
        .is_empty());
}

#[test]
fn unlink_preference_blocks_mapping_when_platform_model_exists() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    seed_platform_catalog(&storage, &["gpt-exists"]);
    insert_test_aggregate_api(&storage, "agg-test", "active");
    storage
        .upsert_discovered_model_source_models(
            ROUTING_SOURCE_KIND_AGGREGATE_API,
            "agg-test",
            &["gpt-exists".to_string()],
            "synced",
        )
        .expect("seed aggregate source model");
    let now = now_ts();
    storage
        .upsert_model_source_mapping(&ModelSourceMapping {
            id: "mapping-test".to_string(),
            platform_model_slug: "gpt-exists".to_string(),
            source_kind: ROUTING_SOURCE_KIND_AGGREGATE_API.to_string(),
            source_id: "agg-test".to_string(),
            upstream_model: "gpt-exists".to_string(),
            enabled: true,
            priority: 0,
            weight: 1,
            billing_model_slug: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed mapping");

    storage
        .delete_model_source_mapping_with_unlink_preference(
            "mapping-test",
            ROUTING_SOURCE_KIND_AGGREGATE_API,
            "agg-test",
            "gpt-exists",
        )
        .expect("delete mapping with unlink");

    let prefs_after_delete: Vec<_> = storage
        .list_model_source_mapping_preferences(ROUTING_SOURCE_KIND_AGGREGATE_API, "agg-test")
        .expect("list preferences")
        .into_iter()
        .filter(|p| p.upstream_model == "gpt-exists")
        .collect();
    assert_eq!(prefs_after_delete.len(), 1);
    assert_eq!(prefs_after_delete[0].preference, PREF_UNLINKED);

    auto_associate_source_models(
        &storage,
        ROUTING_SOURCE_KIND_AGGREGATE_API,
        "agg-test",
        false,
    )
    .expect("auto associate with auto_create=false");

    let catalog = read_managed_model_catalog_from_storage(&storage).expect("read catalog");
    assert!(
        catalog
            .items
            .iter()
            .any(|item| item.model.slug == "gpt-exists"),
        "platform model should remain in catalog"
    );

    assert!(
        storage
            .list_model_source_mappings(Some("gpt-exists"))
            .expect("list mappings")
            .is_empty(),
        "unlink preference should prevent mapping creation"
    );
}
