use codexmanager_core::rpc::types::{
    AggregateApiListResult, AggregateApiRuntimeStatusListResult,
    AggregateApiZeroBalanceStatusListResult, JsonRpcRequest, JsonRpcResponse,
};

use crate::{
    clear_aggregate_api_capability_observation, create_aggregate_api, delete_aggregate_api,
    diagnose_aggregate_api_capabilities, get_aggregate_api_capabilities, get_aggregate_api_health,
    list_aggregate_api_health, list_aggregate_api_probe_costs, list_aggregate_api_runtime_statuses,
    list_aggregate_api_zero_balance_statuses, list_aggregate_apis,
    list_recent_aggregate_api_capability_attempts, probe_aggregate_api_health,
    read_aggregate_api_secret, refresh_aggregate_api_balance,
    reset_aggregate_api_capability_override, reset_aggregate_api_health,
    reset_aggregate_api_runtime_status, reset_aggregate_api_zero_balance_status,
    set_aggregate_api_capability_override, set_aggregate_api_capability_routing_mode,
    test_aggregate_api_connection, update_aggregate_api,
    update_aggregate_api_health_config,
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
    req.params
        .as_ref()
        .and_then(|params| params.get(key))
        .map(serde_json::Value::as_f64)
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
        "aggregateApi/runtimeStatus/reset" => super::value_or_error(
            reset_aggregate_api_runtime_status(api_id_param(req).unwrap_or("")),
        ),
        "aggregateApi/zeroBalanceStatus/list" => super::value_or_error(
            list_aggregate_api_zero_balance_statuses()
                .map(|items| AggregateApiZeroBalanceStatusListResult { items }),
        ),
        "aggregateApi/zeroBalanceStatus/reset" => super::value_or_error(
            reset_aggregate_api_zero_balance_status(api_id_param(req).unwrap_or("")),
        ),
        "aggregateApi/health/list" => super::value_or_error(list_aggregate_api_health()),
        "aggregateApi/health/costs" => super::value_or_error(list_aggregate_api_probe_costs(
            super::i64_param(req, "startTs").unwrap_or(0),
            super::i64_param(req, "endTs").unwrap_or(0),
        )),
        "aggregateApi/health/get" => super::value_or_error(get_aggregate_api_health(
            api_id_param(req).unwrap_or(""),
            super::i64_param(req, "limit").unwrap_or(50),
        )),
        "aggregateApi/health/config/update" => {
            super::value_or_error(update_aggregate_api_health_config(
                api_id_param(req).unwrap_or(""),
                super::bool_param(req, "enabled").unwrap_or(false),
                super::i64_param(req, "intervalSecs"),
                super::i64_param(req, "timeoutMs"),
                super::str_param(req, "probeModel"),
            ))
        }
        "aggregateApi/health/probe" => super::value_or_error(probe_aggregate_api_health(
            api_id_param(req).unwrap_or(""),
            super::str_param(req, "model"),
        )),
        "aggregateApi/health/reset" => super::value_or_error(reset_aggregate_api_health(
            api_id_param(req).unwrap_or(""),
            super::str_param(req, "scopeModel"),
            super::str_param(req, "scopeProtocol"),
        )),
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
            super::value_or_error(diagnose_aggregate_api_capabilities(
                api_id_param(req).unwrap_or(""),
                super::bool_param(req, "liveSmoke").unwrap_or(false),
            ))
        }
        "aggregateApi/capabilities/get" => super::value_or_error(get_aggregate_api_capabilities(
            api_id_param(req).unwrap_or(""),
        )),
        "aggregateApi/capabilities/setOverride" => {
            super::value_or_error(set_aggregate_api_capability_override(
                api_id_param(req).unwrap_or(""),
                super::str_param(req, "upstreamModelPattern"),
                super::str_param(req, "protocol"),
                super::str_param(req, "capabilityKey"),
                super::str_param(req, "state").unwrap_or(""),
            ))
        }
        "aggregateApi/capabilities/resetOverride" => {
            super::value_or_error(reset_aggregate_api_capability_override(
                api_id_param(req).unwrap_or(""),
                super::str_param(req, "upstreamModelPattern"),
                super::str_param(req, "protocol"),
                super::str_param(req, "capabilityKey"),
            ))
        }
        "aggregateApi/capabilities/clearObservation" => {
            super::value_or_error(clear_aggregate_api_capability_observation(
                api_id_param(req).unwrap_or(""),
                super::str_param(req, "upstreamModelPattern"),
                super::str_param(req, "protocol"),
                super::str_param(req, "capabilityKey"),
            ))
        }
        "aggregateApi/capabilities/listRecentAttempts" => {
            super::value_or_error(list_recent_aggregate_api_capability_attempts(
                api_id_param(req).unwrap_or(""),
                super::i64_param(req, "limit").unwrap_or(50),
            ))
        }
        "aggregateApi/capabilities/setMode" => {
            super::value_or_error(set_aggregate_api_capability_routing_mode(
                super::str_param(req, "routingMode").unwrap_or(""),
            ))
        }
        "aggregateApi/refreshBalance" => {
            let api_id = api_id_param(req).unwrap_or("");
            super::value_or_error(refresh_aggregate_api_balance(api_id))
        }
        _ => return None,
    };

    Some(super::response(req, result))
}

#[cfg(test)]
#[path = "aggregate_api_tests.rs"]
mod tests;
