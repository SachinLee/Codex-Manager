use crate::commands::shared::rpc_call_in_background;

/// 函数 `service_aggregate_api_list`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
///
/// # 返回
/// 返回函数执行结果
#[tauri::command]
pub async fn service_aggregate_api_list(addr: Option<String>) -> Result<serde_json::Value, String> {
    rpc_call_in_background("aggregateApi/list", addr, None).await
}

#[tauri::command]
pub async fn service_aggregate_api_runtime_status_list(
    addr: Option<String>,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background("aggregateApi/runtimeStatus/list", addr, None).await
}

#[tauri::command]
pub async fn service_aggregate_api_runtime_status_reset(
    addr: Option<String>,
    id: String,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background(
        "aggregateApi/runtimeStatus/reset",
        addr,
        Some(serde_json::json!({ "id": id })),
    )
    .await
}

/// 函数 `service_aggregate_api_create`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
/// - provider_type: 参数 provider_type
/// - supplier_name: 参数 supplier_name
/// - sort: 参数 sort
/// - url: 参数 url
/// - key: 参数 key
///
/// # 返回
/// 返回函数执行结果
#[tauri::command]
pub async fn service_aggregate_api_create(
    addr: Option<String>,
    provider_type: Option<String>,
    supplier_name: Option<String>,
    sort: Option<i64>,
    url: Option<String>,
    key: Option<String>,
    auth_type: Option<String>,
    auth_custom_enabled: Option<bool>,
    auth_params: Option<serde_json::Value>,
    action_custom_enabled: Option<bool>,
    action: Option<String>,
    model_override: Option<String>,
    username: Option<String>,
    password: Option<String>,
    cost_multiplier: Option<f64>,
    daily_spend_limit_usd: Option<f64>,
    balance_query_enabled: Option<bool>,
    balance_query_template: Option<String>,
    balance_query_base_url: Option<String>,
    balance_query_access_token: Option<String>,
    balance_query_user_id: Option<String>,
    balance_query_config_json: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "providerType": provider_type,
        "supplierName": supplier_name,
        "sort": sort,
        "url": url,
        "key": key,
        "authType": auth_type,
        "authCustomEnabled": auth_custom_enabled,
        "authParams": auth_params,
        "actionCustomEnabled": action_custom_enabled,
        "action": action,
        "modelOverride": model_override,
        "username": username,
        "password": password,
        "costMultiplier": cost_multiplier,
        "dailySpendLimitUsd": daily_spend_limit_usd,
        "balanceQueryEnabled": balance_query_enabled,
        "balanceQueryTemplate": balance_query_template,
        "balanceQueryBaseUrl": balance_query_base_url,
        "balanceQueryAccessToken": balance_query_access_token,
        "balanceQueryUserId": balance_query_user_id,
        "balanceQueryConfigJson": balance_query_config_json,
    });
    rpc_call_in_background("aggregateApi/create", addr, Some(params)).await
}

/// 函数 `service_aggregate_api_update`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
/// - id: 参数 id
/// - provider_type: 参数 provider_type
/// - supplier_name: 参数 supplier_name
/// - sort: 参数 sort
/// - url: 参数 url
/// - key: 参数 key
///
/// # 返回
/// 返回函数执行结果
#[tauri::command]
pub async fn service_aggregate_api_update(
    addr: Option<String>,
    id: String,
    provider_type: Option<String>,
    supplier_name: Option<String>,
    sort: Option<i64>,
    status: Option<String>,
    url: Option<String>,
    key: Option<String>,
    auth_type: Option<String>,
    auth_custom_enabled: Option<bool>,
    auth_params: Option<serde_json::Value>,
    action_custom_enabled: Option<bool>,
    action: Option<String>,
    model_override: Option<String>,
    username: Option<String>,
    password: Option<String>,
    cost_multiplier: Option<f64>,
    daily_spend_limit_usd: Option<f64>,
    clear_daily_spend_limit_usd: Option<bool>,
    balance_query_enabled: Option<bool>,
    balance_query_template: Option<String>,
    balance_query_base_url: Option<String>,
    balance_query_access_token: Option<String>,
    balance_query_user_id: Option<String>,
    balance_query_config_json: Option<String>,
) -> Result<serde_json::Value, String> {
    let mut params = serde_json::json!({
        "id": id,
        "providerType": provider_type,
        "supplierName": supplier_name,
        "sort": sort,
        "status": status,
        "url": url,
        "key": key,
        "authType": auth_type,
        "authCustomEnabled": auth_custom_enabled,
        "authParams": auth_params,
        "actionCustomEnabled": action_custom_enabled,
        "action": action,
        "modelOverride": model_override,
        "username": username,
        "password": password,
        "costMultiplier": cost_multiplier,
        "balanceQueryEnabled": balance_query_enabled,
        "balanceQueryTemplate": balance_query_template,
        "balanceQueryBaseUrl": balance_query_base_url,
        "balanceQueryAccessToken": balance_query_access_token,
        "balanceQueryUserId": balance_query_user_id,
        "balanceQueryConfigJson": balance_query_config_json,
    });
    if let Some(value) = daily_spend_limit_usd {
        params["dailySpendLimitUsd"] = serde_json::Value::from(value);
    } else if clear_daily_spend_limit_usd.unwrap_or(false) {
        params["dailySpendLimitUsd"] = serde_json::Value::Null;
    }
    rpc_call_in_background("aggregateApi/update", addr, Some(params)).await
}

/// 函数 `service_aggregate_api_read_secret`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
/// - id: 参数 id
///
/// # 返回
/// 返回函数执行结果
#[tauri::command]
pub async fn service_aggregate_api_read_secret(
    addr: Option<String>,
    id: String,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "id": id });
    rpc_call_in_background("aggregateApi/readSecret", addr, Some(params)).await
}

/// 函数 `service_aggregate_api_delete`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
/// - id: 参数 id
///
/// # 返回
/// 返回函数执行结果
#[tauri::command]
pub async fn service_aggregate_api_delete(
    addr: Option<String>,
    id: String,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "id": id });
    rpc_call_in_background("aggregateApi/delete", addr, Some(params)).await
}

/// 函数 `service_aggregate_api_test_connection`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
/// - id: 参数 id
///
/// # 返回
/// 返回函数执行结果
#[tauri::command]
pub async fn service_aggregate_api_test_connection(
    addr: Option<String>,
    id: String,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "id": id });
    rpc_call_in_background("aggregateApi/testConnection", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_aggregate_api_diagnose_capabilities(
    addr: Option<String>,
    id: String,
    live_smoke: Option<bool>,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background(
        "aggregateApi/diagnoseCapabilities",
        addr,
        Some(serde_json::json!({ "id": id, "liveSmoke": live_smoke.unwrap_or(false) })),
    )
    .await
}

#[tauri::command]
pub async fn service_aggregate_api_capabilities_get(
    addr: Option<String>,
    id: String,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background("aggregateApi/capabilities/get", addr, Some(serde_json::json!({ "id": id }))).await
}

#[tauri::command]
pub async fn service_aggregate_api_capabilities_set_override(
    addr: Option<String>,
    id: String,
    upstream_model_pattern: String,
    protocol: String,
    capability_key: String,
    state: String,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background(
        "aggregateApi/capabilities/setOverride",
        addr,
        Some(serde_json::json!({ "id": id, "upstreamModelPattern": upstream_model_pattern, "protocol": protocol, "capabilityKey": capability_key, "state": state })),
    )
    .await
}

#[tauri::command]
pub async fn service_aggregate_api_capabilities_reset_override(
    addr: Option<String>,
    id: String,
    upstream_model_pattern: String,
    protocol: String,
    capability_key: String,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background(
        "aggregateApi/capabilities/resetOverride",
        addr,
        Some(serde_json::json!({ "id": id, "upstreamModelPattern": upstream_model_pattern, "protocol": protocol, "capabilityKey": capability_key })),
    )
    .await
}

#[tauri::command]
pub async fn service_aggregate_api_capabilities_clear_observation(
    addr: Option<String>,
    id: String,
    upstream_model_pattern: String,
    protocol: String,
    capability_key: String,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background(
        "aggregateApi/capabilities/clearObservation",
        addr,
        Some(serde_json::json!({ "id": id, "upstreamModelPattern": upstream_model_pattern, "protocol": protocol, "capabilityKey": capability_key })),
    )
    .await
}

#[tauri::command]
pub async fn service_aggregate_api_capabilities_list_recent_attempts(
    addr: Option<String>,
    id: String,
    limit: Option<i64>,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background(
        "aggregateApi/capabilities/listRecentAttempts",
        addr,
        Some(serde_json::json!({ "id": id, "limit": limit.unwrap_or(50) })),
    )
    .await
}

#[tauri::command]
pub async fn service_aggregate_api_capabilities_set_mode(
    addr: Option<String>,
    routing_mode: String,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background(
        "aggregateApi/capabilities/setMode",
        addr,
        Some(serde_json::json!({ "routingMode": routing_mode })),
    )
    .await
}

#[tauri::command]
pub async fn service_aggregate_api_refresh_balance(
    addr: Option<String>,
    id: String,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "id": id });
    rpc_call_in_background("aggregateApi/refreshBalance", addr, Some(params)).await
}
