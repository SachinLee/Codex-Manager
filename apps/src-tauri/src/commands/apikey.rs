use crate::commands::shared::rpc_call_in_background;

/// 函数 `service_apikey_list`
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
pub async fn service_apikey_list(addr: Option<String>) -> Result<serde_json::Value, String> {
    rpc_call_in_background("apikey/list", addr, None).await
}

/// 函数 `service_apikey_read_secret`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
/// - key_id: 参数 key_id
///
/// # 返回
/// 返回函数执行结果
#[tauri::command]
pub async fn service_apikey_read_secret(
    addr: Option<String>,
    key_id: String,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "id": key_id });
    rpc_call_in_background("apikey/readSecret", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_managed_model_list_v2(
    addr: Option<String>,
    include_hidden: Option<bool>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "includeHidden": include_hidden.unwrap_or(false) });
    rpc_call_in_background("apikey/managedModelListV2", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_managed_model_get_v2(
    addr: Option<String>,
    slug: String,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "slug": slug });
    rpc_call_in_background("apikey/managedModelGetV2", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_managed_model_upsert_v2(
    addr: Option<String>,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background("apikey/managedModelUpsertV2", addr, Some(payload)).await
}

#[tauri::command]
pub async fn service_managed_model_update_state_v2(
    addr: Option<String>,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background("apikey/managedModelUpdateStateV2", addr, Some(payload)).await
}

#[tauri::command]
pub async fn service_managed_model_batch_update_state_v2(
    addr: Option<String>,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background(
        "apikey/managedModelBatchUpdateStateV2",
        addr,
        Some(payload),
    )
    .await
}

#[tauri::command]
pub async fn service_managed_model_delete_v2(
    addr: Option<String>,
    slug: String,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "slug": slug });
    rpc_call_in_background("apikey/managedModelDeleteV2", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_managed_model_import_preview_v2(
    addr: Option<String>,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background(
        "apikey/managedModelImportPreviewV2",
        addr,
        Some(payload),
    )
    .await
}

#[tauri::command]
pub async fn service_managed_model_import_commit_v2(
    addr: Option<String>,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background("apikey/managedModelImportCommitV2", addr, Some(payload)).await
}

/// 函数 `service_apikey_create`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
/// - name: 参数 name
/// - model_slug: 参数 model_slug
/// - reasoning_effort: 参数 reasoning_effort
/// - service_tier: 参数 service_tier
/// - protocol_type: 参数 protocol_type
/// - upstream_base_url: 参数 upstream_base_url
/// - static_headers_json: 参数 static_headers_json
/// - rotation_strategy: 参数 rotation_strategy
/// - aggregate_api_id: 参数 aggregate_api_id
/// - account_plan_filter: 参数 account_plan_filter
/// - quota_limit_tokens: 参数 quota_limit_tokens
/// - custom_key: 参数 custom_key
///
/// # 返回
/// 返回函数执行结果
#[tauri::command]
pub async fn service_apikey_create(
    addr: Option<String>,
    name: Option<String>,
    model_slug: Option<String>,
    reasoning_effort: Option<String>,
    service_tier: Option<String>,
    protocol_type: Option<String>,
    upstream_base_url: Option<String>,
    static_headers_json: Option<String>,
    rotation_strategy: Option<String>,
    aggregate_api_id: Option<String>,
    account_plan_filter: Option<String>,
    quota_limit_tokens: Option<i64>,
    custom_key: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
      "name": name,
      "modelSlug": model_slug,
      "reasoningEffort": reasoning_effort,
      "serviceTier": service_tier,
      "protocolType": protocol_type,
      "upstreamBaseUrl": upstream_base_url,
      "staticHeadersJson": static_headers_json,
      "rotationStrategy": rotation_strategy,
      "aggregateApiId": aggregate_api_id,
      "accountPlanFilter": account_plan_filter,
      "quotaLimitTokens": quota_limit_tokens,
      "customKey": custom_key,
    });
    rpc_call_in_background("apikey/create", addr, Some(params)).await
}

/// 函数 `service_apikey_usage_stats`
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
pub async fn service_apikey_usage_stats(addr: Option<String>) -> Result<serde_json::Value, String> {
    rpc_call_in_background("apikey/usageStats", addr, None).await
}

/// 函数 `service_apikey_update_model`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
/// - key_id: 参数 key_id
/// - name: 参数 name
/// - model_slug: 参数 model_slug
/// - reasoning_effort: 参数 reasoning_effort
/// - service_tier: 参数 service_tier
/// - protocol_type: 参数 protocol_type
/// - upstream_base_url: 参数 upstream_base_url
/// - static_headers_json: 参数 static_headers_json
/// - rotation_strategy: 参数 rotation_strategy
/// - aggregate_api_id: 参数 aggregate_api_id
/// - account_plan_filter: 参数 account_plan_filter
///
/// # 返回
/// 返回函数执行结果
#[tauri::command]
pub async fn service_apikey_update_model(
    addr: Option<String>,
    key_id: String,
    name: Option<String>,
    model_slug: Option<String>,
    reasoning_effort: Option<String>,
    service_tier: Option<String>,
    protocol_type: Option<String>,
    upstream_base_url: Option<String>,
    static_headers_json: Option<String>,
    rotation_strategy: Option<String>,
    aggregate_api_id: Option<String>,
    account_plan_filter: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
      "id": key_id,
      "name": name,
      "modelSlug": model_slug,
      "reasoningEffort": reasoning_effort,
      "serviceTier": service_tier,
      "protocolType": protocol_type,
      "upstreamBaseUrl": upstream_base_url,
      "staticHeadersJson": static_headers_json,
      "rotationStrategy": rotation_strategy,
      "aggregateApiId": aggregate_api_id,
      "accountPlanFilter": account_plan_filter,
    });
    rpc_call_in_background("apikey/updateModel", addr, Some(params)).await
}

/// 函数 `service_apikey_delete`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
/// - key_id: 参数 key_id
///
/// # 返回
/// 返回函数执行结果
#[tauri::command]
pub async fn service_apikey_delete(
    addr: Option<String>,
    key_id: String,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "id": key_id });
    rpc_call_in_background("apikey/delete", addr, Some(params)).await
}

/// 函数 `service_apikey_disable`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
/// - key_id: 参数 key_id
///
/// # 返回
/// 返回函数执行结果
#[tauri::command]
pub async fn service_apikey_disable(
    addr: Option<String>,
    key_id: String,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "id": key_id });
    rpc_call_in_background("apikey/disable", addr, Some(params)).await
}

/// 函数 `service_apikey_enable`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
/// - key_id: 参数 key_id
///
/// # 返回
/// 返回函数执行结果
#[tauri::command]
pub async fn service_apikey_enable(
    addr: Option<String>,
    key_id: String,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "id": key_id });
    rpc_call_in_background("apikey/enable", addr, Some(params)).await
}
