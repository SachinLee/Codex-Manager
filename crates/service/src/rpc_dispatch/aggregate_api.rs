use codexmanager_core::rpc::types::{
    AggregateApiListResult, AggregateApiRuntimeStatusListResult,
    AggregateApiSupplierModelDeleteParams, AggregateApiSupplierModelImportParams,
    AggregateApiSupplierModelListResult, AggregateApiSupplierModelUpsertParams, JsonRpcRequest,
    JsonRpcResponse,
};

use crate::{
    clear_aggregate_api_capability_observation,
    create_aggregate_api, delete_aggregate_api, delete_aggregate_api_supplier_model,
    diagnose_aggregate_api_capabilities, get_aggregate_api_capabilities,
    import_aggregate_api_supplier_models,
    list_recent_aggregate_api_capability_attempts,
    list_aggregate_api_runtime_statuses, list_aggregate_api_supplier_models, list_aggregate_apis,
    read_aggregate_api_secret, refresh_aggregate_api_balance, reset_aggregate_api_runtime_status,
    reset_aggregate_api_capability_override, save_aggregate_api_supplier_model,
    set_aggregate_api_capability_override, set_aggregate_api_capability_routing_mode,
    test_aggregate_api_connection, update_aggregate_api,
};

/// 函数 `api_id_param`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - req: 参数 req
///
/// # 返回
/// 返回函数执行结果
fn api_id_param(req: &JsonRpcRequest) -> Option<&str> {
    super::str_param(req, "id").or_else(|| super::str_param(req, "apiId"))
}

fn optional_f64_param(req: &JsonRpcRequest, key: &str) -> Option<Option<f64>> {
    let value = req.params.as_ref()?.get(key)?;
    if value.is_null() {
        return Some(None);
    }
    value.as_f64().map(Some)
}

/// 函数 `try_handle`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn try_handle(req: &JsonRpcRequest) -> Option<JsonRpcResponse> {
    let result = match req.method.as_str() {
        "aggregateApi/list" => super::value_or_error(
            list_aggregate_apis().map(|items| AggregateApiListResult { items }),
        ),
        "aggregateApi/runtimeStatus/list" => super::value_or_error(
            list_aggregate_api_runtime_statuses()
                .map(|items| AggregateApiRuntimeStatusListResult { items }),
        ),
        "aggregateApi/runtimeStatus/reset" => {
            let api_id = api_id_param(req).unwrap_or("");
            super::value_or_error(reset_aggregate_api_runtime_status(api_id))
        }
        "aggregateApi/create" => {
            let provider_type = super::string_param(req, "providerType");
            let supplier_name = super::string_param(req, "supplierName");
            let sort = super::i64_param(req, "sort");
            let url = super::string_param(req, "url");
            let key = super::string_param(req, "key");
            let auth_type = super::string_param(req, "authType");
            let auth_custom_enabled = super::bool_param(req, "authCustomEnabled");
            let auth_params = req
                .params
                .as_ref()
                .and_then(|v| v.get("authParams"))
                .cloned();
            let action_custom_enabled = super::bool_param(req, "actionCustomEnabled");
            let action = super::string_param(req, "action");
            let model_override = super::string_param(req, "modelOverride");
            let username = super::string_param(req, "username");
            let password = super::string_param(req, "password");
            let cost_multiplier = super::f64_param(req, "costMultiplier");
            let daily_spend_limit_usd = super::f64_param(req, "dailySpendLimitUsd");
            let balance_query_enabled = super::bool_param(req, "balanceQueryEnabled");
            let balance_query_template = super::string_param(req, "balanceQueryTemplate");
            let balance_query_base_url = super::string_param(req, "balanceQueryBaseUrl");
            let balance_query_access_token = super::string_param(req, "balanceQueryAccessToken");
            let balance_query_user_id = super::string_param(req, "balanceQueryUserId");
            let balance_query_config_json = super::string_param(req, "balanceQueryConfigJson");
            let model_slugs = string_array_param(req, "modelSlugs");
            super::value_or_error(create_aggregate_api(
                url,
                key,
                provider_type,
                supplier_name,
                sort,
                auth_type,
                auth_custom_enabled,
                auth_params,
                action_custom_enabled,
                action,
                model_override,
                username,
                password,
                cost_multiplier,
                daily_spend_limit_usd,
                balance_query_enabled,
                balance_query_template,
                balance_query_base_url,
                balance_query_access_token,
                balance_query_user_id,
                balance_query_config_json,
                model_slugs,
            ))
        }
        "aggregateApi/update" => {
            let api_id = api_id_param(req).unwrap_or("");
            let provider_type = super::string_param(req, "providerType");
            let supplier_name = super::string_param(req, "supplierName");
            let sort = super::i64_param(req, "sort");
            let status = super::string_param(req, "status");
            let url = super::string_param(req, "url");
            let key = super::string_param(req, "key");
            let auth_type = super::string_param(req, "authType");
            let auth_custom_enabled = super::bool_param(req, "authCustomEnabled");
            let auth_params = req
                .params
                .as_ref()
                .and_then(|v| v.get("authParams"))
                .cloned();
            let action_custom_enabled = super::bool_param(req, "actionCustomEnabled");
            let action = super::string_param(req, "action");
            let model_override = super::string_param(req, "modelOverride");
            let username = super::string_param(req, "username");
            let password = super::string_param(req, "password");
            let cost_multiplier = super::f64_param(req, "costMultiplier");
            let daily_spend_limit_usd = optional_f64_param(req, "dailySpendLimitUsd");
            let balance_query_enabled = super::bool_param(req, "balanceQueryEnabled");
            let balance_query_template = super::string_param(req, "balanceQueryTemplate");
            let balance_query_base_url = super::string_param(req, "balanceQueryBaseUrl");
            let balance_query_access_token = super::string_param(req, "balanceQueryAccessToken");
            let balance_query_user_id = super::string_param(req, "balanceQueryUserId");
            let balance_query_config_json = super::string_param(req, "balanceQueryConfigJson");
            let model_slugs = string_array_param(req, "modelSlugs");
            super::ok_or_error(update_aggregate_api(
                api_id,
                url,
                key,
                provider_type,
                supplier_name,
                sort,
                status,
                auth_type,
                auth_custom_enabled,
                auth_params,
                action_custom_enabled,
                action,
                model_override,
                username,
                password,
                cost_multiplier,
                daily_spend_limit_usd,
                balance_query_enabled,
                balance_query_template,
                balance_query_base_url,
                balance_query_access_token,
                balance_query_user_id,
                balance_query_config_json,
                model_slugs,
            ))
        }
        "aggregateApi/readSecret" => {
            let api_id = api_id_param(req).unwrap_or("");
            super::value_or_error(read_aggregate_api_secret(api_id))
        }
        "aggregateApi/delete" => {
            let api_id = api_id_param(req).unwrap_or("");
            super::ok_or_error(delete_aggregate_api(api_id))
        }
        "aggregateApi/testConnection" => {
            let api_id = api_id_param(req).unwrap_or("");
            super::value_or_error(test_aggregate_api_connection(api_id))
        }
        "aggregateApi/diagnoseCapabilities" => {
            let api_id = api_id_param(req).unwrap_or("");
            let live_smoke = super::bool_param(req, "liveSmoke").unwrap_or(false);
            super::value_or_error(diagnose_aggregate_api_capabilities(api_id, live_smoke))
        }
        "aggregateApi/capabilities/get" => {
            super::value_or_error(get_aggregate_api_capabilities(api_id_param(req).unwrap_or("")))
        }
        "aggregateApi/capabilities/setOverride" => super::value_or_error(
            set_aggregate_api_capability_override(
                api_id_param(req).unwrap_or(""),
                super::str_param(req, "upstreamModelPattern"),
                super::str_param(req, "protocol"),
                super::str_param(req, "capabilityKey"),
                super::str_param(req, "state").unwrap_or(""),
            ),
        ),
        "aggregateApi/capabilities/resetOverride" => super::value_or_error(
            reset_aggregate_api_capability_override(
                api_id_param(req).unwrap_or(""),
                super::str_param(req, "upstreamModelPattern"),
                super::str_param(req, "protocol"),
                super::str_param(req, "capabilityKey"),
            ),
        ),
        "aggregateApi/capabilities/clearObservation" => super::value_or_error(
            clear_aggregate_api_capability_observation(
                api_id_param(req).unwrap_or(""),
                super::str_param(req, "upstreamModelPattern"),
                super::str_param(req, "protocol"),
                super::str_param(req, "capabilityKey"),
            ),
        ),
        "aggregateApi/capabilities/listRecentAttempts" => super::value_or_error(
            list_recent_aggregate_api_capability_attempts(
                api_id_param(req).unwrap_or(""),
                super::i64_param(req, "limit").unwrap_or(50),
            ),
        ),
        "aggregateApi/capabilities/setMode" => super::value_or_error(
            set_aggregate_api_capability_routing_mode(
                super::str_param(req, "routingMode").unwrap_or(""),
            ),
        ),
        "aggregateApi/refreshBalance" => {
            let api_id = api_id_param(req).unwrap_or("");
            super::value_or_error(refresh_aggregate_api_balance(api_id))
        }
        "aggregateApi/supplierModels/list" => {
            let supplier_key = super::string_param(req, "supplierKey");
            let provider_type = super::string_param(req, "providerType");
            super::value_or_error(
                list_aggregate_api_supplier_models(supplier_key, provider_type)
                    .map(|items| AggregateApiSupplierModelListResult { items }),
            )
        }
        "aggregateApi/supplierModels/save" => {
            let params = req
                .params
                .clone()
                .ok_or_else(|| "缺少供应商模型参数".to_string())
                .and_then(|value| {
                    serde_json::from_value::<AggregateApiSupplierModelUpsertParams>(value)
                        .map_err(|err| format!("解析供应商模型参数失败: {err}"))
                });
            super::value_or_error(params.and_then(save_aggregate_api_supplier_model))
        }
        "aggregateApi/supplierModels/delete" => {
            let params = req
                .params
                .clone()
                .ok_or_else(|| "缺少供应商模型参数".to_string())
                .and_then(|value| {
                    serde_json::from_value::<AggregateApiSupplierModelDeleteParams>(value)
                        .map_err(|err| format!("解析供应商模型参数失败: {err}"))
                });
            super::ok_or_error(params.and_then(delete_aggregate_api_supplier_model))
        }
        "aggregateApi/sourceModels/importSupplier" => {
            let params = req
                .params
                .clone()
                .ok_or_else(|| "缺少供应商模型导入参数".to_string())
                .and_then(|value| {
                    serde_json::from_value::<AggregateApiSupplierModelImportParams>(value)
                        .map_err(|err| format!("解析供应商模型导入参数失败: {err}"))
                });
            super::value_or_error(params.and_then(import_aggregate_api_supplier_models))
        }
        _ => return None,
    };

    Some(super::response(req, result))
}

fn string_array_param(req: &JsonRpcRequest, key: &str) -> Option<Vec<String>> {
    req.params
        .as_ref()
        .and_then(|params| params.get(key))
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
}

#[cfg(test)]
mod tests {
    use super::try_handle;
    use crate::storage_helpers;
    use codexmanager_core::rpc::types::JsonRpcRequest;
    use codexmanager_core::storage::Storage;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// 函数 `rpc_request`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - method: 参数 method
    /// - params: 参数 params
    ///
    /// # 返回
    /// 返回函数执行结果
    fn rpc_request(method: &str, params: serde_json::Value) -> JsonRpcRequest {
        JsonRpcRequest {
            id: 1.into(),
            method: method.to_string(),
            params: Some(params),
            trace: None,
        }
    }

    /// 函数 `error_message`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - resp: 参数 resp
    ///
    /// # 返回
    /// 返回函数执行结果
    fn error_message(resp: &codexmanager_core::rpc::types::JsonRpcResponse) -> String {
        resp.result
            .get("error")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string()
    }

    fn isolated_db_path(label: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after unix epoch")
            .as_nanos();
        std::env::temp_dir()
            .join(format!(
                "codexmanager-aggregate-rpc-{label}-{}-{nanos}.db",
                std::process::id()
            ))
            .to_string_lossy()
            .into_owned()
    }

    fn setup_storage(label: &str) -> String {
        let db_path = isolated_db_path(label);
        let _ = std::fs::remove_file(&db_path);
        std::env::set_var("CODEXMANAGER_DB_PATH", db_path.as_str());
        storage_helpers::initialize_storage().expect("init storage");
        db_path
    }

    /// 函数 `aggregate_api_update_accepts_id_and_api_id`
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
    fn aggregate_api_update_accepts_id_and_api_id() {
        let missing = try_handle(&rpc_request(
            "aggregateApi/update",
            serde_json::json!({ "supplierName": "codex" }),
        ))
        .expect("response");
        assert_eq!(error_message(&missing), "aggregate api id required");

        let with_id = try_handle(&rpc_request(
            "aggregateApi/update",
            serde_json::json!({ "id": "ag_test", "supplierName": "codex" }),
        ))
        .expect("response");
        assert_ne!(error_message(&with_id), "aggregate api id required");

        let with_api_id = try_handle(&rpc_request(
            "aggregateApi/update",
            serde_json::json!({ "apiId": "ag_test", "supplierName": "codex" }),
        ))
        .expect("response");
        assert_ne!(error_message(&with_api_id), "aggregate api id required");
    }

    #[test]
    fn aggregate_api_create_persists_cost_multiplier() {
        let _guard = crate::test_env_guard();
        let db_path = setup_storage("create-cost-multiplier");

        let response = try_handle(&rpc_request(
            "aggregateApi/create",
            serde_json::json!({
                "providerType": "codex",
                "supplierName": "cost supplier",
                "url": "https://cost.example.com/v1",
                "key": "secret",
                "costMultiplier": 1.8,
                "dailySpendLimitUsd": 12.5
            }),
        ))
        .expect("response");
        assert_eq!(error_message(&response), "");
        let api_id = response.result["id"].as_str().expect("created id");

        let storage = Storage::open(db_path.as_str()).expect("open storage");
        let api = storage
            .find_aggregate_api_by_id(api_id)
            .expect("read aggregate api")
            .expect("aggregate api exists");
        assert!((api.cost_multiplier - 1.8).abs() < f64::EPSILON);
        assert_eq!(api.daily_spend_limit_usd, Some(12.5));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn aggregate_api_update_persists_cost_multiplier() {
        let _guard = crate::test_env_guard();
        let db_path = setup_storage("update-cost-multiplier");
        let create_response = try_handle(&rpc_request(
            "aggregateApi/create",
            serde_json::json!({
                "providerType": "codex",
                "supplierName": "cost supplier",
                "url": "https://cost.example.com/v1",
                "key": "secret"
            }),
        ))
        .expect("create response");
        assert_eq!(error_message(&create_response), "");
        let api_id = create_response.result["id"].as_str().expect("created id");

        let update_response = try_handle(&rpc_request(
            "aggregateApi/update",
            serde_json::json!({
                "id": api_id,
                "supplierName": "cost supplier",
                "costMultiplier": 2.25,
                "dailySpendLimitUsd": 30.0
            }),
        ))
        .expect("update response");
        assert_eq!(error_message(&update_response), "");

        let storage = Storage::open(db_path.as_str()).expect("open storage");
        let api = storage
            .find_aggregate_api_by_id(api_id)
            .expect("read aggregate api")
            .expect("aggregate api exists");
        assert!((api.cost_multiplier - 2.25).abs() < f64::EPSILON);
        assert_eq!(api.daily_spend_limit_usd, Some(30.0));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn aggregate_api_update_can_clear_daily_spend_limit() {
        let _guard = crate::test_env_guard();
        let db_path = setup_storage("clear-daily-spend-limit");
        let create_response = try_handle(&rpc_request(
            "aggregateApi/create",
            serde_json::json!({
                "providerType": "codex",
                "supplierName": "cost supplier",
                "url": "https://cost.example.com/v1",
                "key": "secret",
                "dailySpendLimitUsd": 12.5
            }),
        ))
        .expect("create response");
        assert_eq!(error_message(&create_response), "");
        let api_id = create_response.result["id"].as_str().expect("created id");

        let update_response = try_handle(&rpc_request(
            "aggregateApi/update",
            serde_json::json!({
                "id": api_id,
                "supplierName": "cost supplier",
                "dailySpendLimitUsd": null
            }),
        ))
        .expect("update response");
        assert_eq!(error_message(&update_response), "");

        let storage = Storage::open(db_path.as_str()).expect("open storage");
        let api = storage
            .find_aggregate_api_by_id(api_id)
            .expect("read aggregate api")
            .expect("aggregate api exists");
        assert_eq!(api.daily_spend_limit_usd, None);

        let _ = std::fs::remove_file(db_path);
    }

    /// 函数 `aggregate_api_test_connection_accepts_id_and_api_id`
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
    fn aggregate_api_test_connection_accepts_id_and_api_id() {
        let missing = try_handle(&rpc_request(
            "aggregateApi/testConnection",
            serde_json::json!({}),
        ))
        .expect("response");
        assert_eq!(error_message(&missing), "aggregate api id required");

        let with_id = try_handle(&rpc_request(
            "aggregateApi/testConnection",
            serde_json::json!({ "id": "ag_test" }),
        ))
        .expect("response");
        assert_ne!(error_message(&with_id), "aggregate api id required");

        let with_api_id = try_handle(&rpc_request(
            "aggregateApi/testConnection",
            serde_json::json!({ "apiId": "ag_test" }),
        ))
        .expect("response");
        assert_ne!(error_message(&with_api_id), "aggregate api id required");
    }

    #[test]
    fn aggregate_api_runtime_status_list_and_reset_use_current_runtime_state() {
        let _guard = crate::test_env_guard();
        let db_path = setup_storage("runtime-status");
        let create_response = try_handle(&rpc_request(
            "aggregateApi/create",
            serde_json::json!({
                "providerType": "codex",
                "supplierName": "runtime status supplier",
                "url": "https://runtime-status.example.com/v1",
                "key": "secret"
            }),
        ))
        .expect("create response");
        assert_eq!(error_message(&create_response), "");
        let api_id = create_response.result["id"].as_str().expect("created id");

        for _ in 0..5 {
            crate::gateway::gateway_record_aggregate_api_failure(api_id);
        }

        let list_response = try_handle(&rpc_request(
            "aggregateApi/runtimeStatus/list",
            serde_json::json!({}),
        ))
        .expect("runtime status list response");
        assert_eq!(error_message(&list_response), "");
        assert_eq!(list_response.result["items"][0]["aggregateApiId"], api_id);
        assert_eq!(list_response.result["items"][0]["isCoolingDown"], true);
        assert_eq!(list_response.result["items"][0]["consecutiveFailures"], 5);

        let reset_response = try_handle(&rpc_request(
            "aggregateApi/runtimeStatus/reset",
            serde_json::json!({ "id": api_id }),
        ))
        .expect("runtime status reset response");
        assert_eq!(error_message(&reset_response), "");
        assert_eq!(reset_response.result["aggregateApiId"], api_id);
        assert_eq!(reset_response.result["isCoolingDown"], false);

        let missing_response = try_handle(&rpc_request(
            "aggregateApi/runtimeStatus/reset",
            serde_json::json!({ "id": "missing-aggregate-api" }),
        ))
        .expect("missing runtime status reset response");
        assert_eq!(error_message(&missing_response), "aggregate api not found");

        crate::gateway::gateway_clear_aggregate_api_cooldown(api_id);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn aggregate_api_capability_override_round_trips_through_rpc() {
        let _guard = crate::test_env_guard();
        let db_path = setup_storage("capability-override");
        let create_response = try_handle(&rpc_request(
            "aggregateApi/create",
            serde_json::json!({
                "providerType": "codex",
                "supplierName": "capability supplier",
                "url": "https://capability.example.com/v1",
                "key": "secret",
                "modelOverride": "grok-4.5"
            }),
        ))
        .expect("create response");
        let api_id = create_response.result["id"].as_str().expect("created id");

        let set_response = try_handle(&rpc_request(
            "aggregateApi/capabilities/setOverride",
            serde_json::json!({
                "id": api_id,
                "upstreamModelPattern": "grok-4.5",
                "protocol": "responses",
                "capabilityKey": "responses.hosted_tool.image_generation",
                "state": "unsupported"
            }),
        ))
        .expect("set override response");
        assert_eq!(error_message(&set_response), "");
        assert_eq!(set_response.result["items"][0]["effectiveState"], "unsupported");
        assert_eq!(set_response.result["items"][0]["resolvedSource"], "operator");

        let reset_response = try_handle(&rpc_request(
            "aggregateApi/capabilities/resetOverride",
            serde_json::json!({
                "id": api_id,
                "upstreamModelPattern": "grok-4.5",
                "protocol": "responses",
                "capabilityKey": "responses.hosted_tool.image_generation"
            }),
        ))
        .expect("reset override response");
        assert_eq!(error_message(&reset_response), "");
        assert_eq!(reset_response.result["items"][0]["effectiveState"], "unknown");

        let _ = std::fs::remove_file(db_path);
    }
}
