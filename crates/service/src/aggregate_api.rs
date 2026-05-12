use codexmanager_core::rpc::types::{
    AggregateApiBalanceResult, AggregateApiCreateResult, AggregateApiSecretResult,
    AggregateApiSummary, AggregateApiTestResult,
};
use codexmanager_core::storage::{now_ts, AggregateApi};
use reqwest::header::{HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::Read;
use std::time::Instant;

use crate::apikey_profile::normalize_upstream_base_url;
use crate::gateway;
use crate::storage_helpers::{generate_aggregate_api_id, open_storage};

pub(crate) const AGGREGATE_API_PROVIDER_CODEX: &str = "codex";
pub(crate) const AGGREGATE_API_PROVIDER_CLAUDE: &str = "claude";
pub(crate) const AGGREGATE_API_PROVIDER_GEMINI: &str = "gemini";
pub(crate) const AGGREGATE_API_AUTH_APIKEY: &str = "apikey";
pub(crate) const AGGREGATE_API_AUTH_USERPASS: &str = "userpass";
const CLAUDE_DEFAULT_PROBE_MODEL: &str = "claude-haiku-4-5-20251001";
const ALIBABA_CODING_PLAN_PROBE_MODEL: &str = "qwen3.5-plus";
const MAX_DISCOVERED_CLAUDE_PROBE_MODELS: usize = 8;
const DEFAULT_BALANCE_UNIT: &str = "USD";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OfficialBalanceProvider {
    DeepSeek,
    StepFun,
    SiliconFlowCn,
    SiliconFlowGlobal,
    OpenRouter,
    Novita,
}

#[derive(Debug, Clone, PartialEq)]
struct BalanceSnapshot {
    remaining: Option<f64>,
    used: Option<f64>,
    total: Option<f64>,
    unit: Option<String>,
    plan_name: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UserPassSecret {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiKeyAuthParams {
    location: String,
    name: String,
    #[serde(default)]
    header_value_format: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserPassAuthParams {
    mode: String,
    #[serde(default)]
    username_name: Option<String>,
    #[serde(default)]
    password_name: Option<String>,
}

/// 函数 `normalize_secret`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - value: 参数 value
///
/// # 返回
/// 返回函数执行结果
fn normalize_secret(value: Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// 函数 `normalize_supplier_name`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - value: 参数 value
///
/// # 返回
/// 返回函数执行结果
fn normalize_supplier_name(value: Option<String>) -> Result<String, String> {
    let normalized = value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "supplier name is required".to_string())?;
    Ok(normalized)
}

/// 函数 `normalize_sort`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - value: 参数 value
///
/// # 返回
/// 返回函数执行结果
fn normalize_sort(value: Option<i64>) -> i64 {
    value.unwrap_or(0)
}

fn normalize_cost_multiplier(value: Option<f64>) -> Result<f64, String> {
    let Some(value) = value else {
        return Ok(1.0);
    };
    if !value.is_finite() || value <= 0.0 {
        return Err("aggregate api costMultiplier must be greater than 0".to_string());
    }
    if value > 100.0 {
        return Err("aggregate api costMultiplier must be less than or equal to 100".to_string());
    }
    Ok(value)
}

fn normalize_daily_spend_limit_usd(value: Option<f64>) -> Result<Option<f64>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if !value.is_finite() {
        return Err("aggregate api dailySpendLimitUsd must be finite".to_string());
    }
    if value <= 0.0 {
        return Ok(None);
    }
    if value > 1_000_000.0 {
        return Err(
            "aggregate api dailySpendLimitUsd must be less than or equal to 1000000".to_string(),
        );
    }
    Ok(Some(value))
}

fn normalize_status(value: Option<String>) -> Result<String, String> {
    match value {
        Some(raw) => {
            let normalized = raw.trim().to_ascii_lowercase().replace('-', "_");
            match normalized.as_str() {
                "active" | "enabled" | "enable" => Ok("active".to_string()),
                "disabled" | "disable" | "inactive" => Ok("disabled".to_string()),
                other => Err(format!("unsupported aggregate api status: {other}")),
            }
        }
        None => Ok("active".to_string()),
    }
}

fn normalize_auth_type(value: Option<String>) -> Result<String, String> {
    match value {
        Some(raw) => {
            let normalized = raw.trim().to_ascii_lowercase().replace('-', "_");
            match normalized.as_str() {
                "apikey" | "api_key" | "key" => Ok(AGGREGATE_API_AUTH_APIKEY.to_string()),
                "userpass" | "username_password" | "account_password" | "basic" | "http_basic" => {
                    Ok(AGGREGATE_API_AUTH_USERPASS.to_string())
                }
                other => Err(format!("unsupported aggregate api auth type: {other}")),
            }
        }
        None => Ok(AGGREGATE_API_AUTH_APIKEY.to_string()),
    }
}

fn normalize_action(value: Option<String>) -> Result<Option<String>, String> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let normalized = trimmed.to_string();
    let lower = normalized.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Err("aggregate api action must be a path, not a full url".to_string());
    }
    if normalized.contains("://") {
        return Err("aggregate api action is invalid".to_string());
    }
    let with_slash = if normalized.starts_with('/') {
        normalized
    } else {
        format!("/{normalized}")
    };
    Ok(Some(with_slash))
}

fn normalize_model_override(value: Option<String>) -> Result<Option<String>, String> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
        return Ok(None);
    }
    if trimmed
        .chars()
        .any(|ch| !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':')))
    {
        return Err("aggregate api modelOverride contains unsupported characters".to_string());
    }
    Ok(Some(trimmed.to_string()))
}

fn normalize_auth_params_json(
    auth_type: &str,
    enabled: Option<bool>,
    auth_params: Option<serde_json::Value>,
) -> Result<Option<String>, String> {
    match enabled {
        None => Ok(None),
        Some(false) => Ok(Some(String::new())),
        Some(true) => {
            let value = auth_params.ok_or_else(|| "authParams is required".to_string())?;
            let obj = value
                .as_object()
                .ok_or_else(|| "authParams must be a JSON object".to_string())?;
            if obj.is_empty() {
                return Err("authParams must not be empty".to_string());
            }
            if auth_type == AGGREGATE_API_AUTH_APIKEY {
                let parsed: ApiKeyAuthParams = serde_json::from_value(value.clone())
                    .map_err(|_| "authParams is invalid".to_string())?;
                let location = parsed.location.trim().to_ascii_lowercase();
                if location != "header" && location != "query" {
                    return Err("authParams.location must be header or query".to_string());
                }
                if parsed.name.trim().is_empty() {
                    return Err("authParams.name is required".to_string());
                }
                if location == "header" {
                    let format = parsed
                        .header_value_format
                        .as_deref()
                        .unwrap_or("bearer")
                        .trim()
                        .to_ascii_lowercase();
                    if format != "bearer" && format != "raw" {
                        return Err(
                            "authParams.headerValueFormat must be bearer or raw".to_string()
                        );
                    }
                }
            } else if auth_type == AGGREGATE_API_AUTH_USERPASS {
                let parsed: UserPassAuthParams = serde_json::from_value(value.clone())
                    .map_err(|_| "authParams is invalid".to_string())?;
                let mode = parsed.mode.trim().to_ascii_lowercase();
                match mode.as_str() {
                    "basic" => {}
                    "headerpair" | "querypair" => {
                        if parsed
                            .username_name
                            .as_deref()
                            .map(str::trim)
                            .unwrap_or("")
                            .is_empty()
                        {
                            return Err("authParams.usernameName is required".to_string());
                        }
                        if parsed
                            .password_name
                            .as_deref()
                            .map(str::trim)
                            .unwrap_or("")
                            .is_empty()
                        {
                            return Err("authParams.passwordName is required".to_string());
                        }
                    }
                    _ => {
                        return Err(
                            "authParams.mode must be basic, headerPair, or queryPair".to_string()
                        );
                    }
                }
            }
            serde_json::to_string(&value)
                .map(Some)
                .map_err(|_| "authParams must be a valid JSON object".to_string())
        }
    }
}

fn normalize_action_override(
    enabled: Option<bool>,
    action: Option<String>,
) -> Result<Option<Option<String>>, String> {
    match enabled {
        None => Ok(None),
        Some(false) => Ok(Some(None)),
        Some(true) => {
            normalize_action(action).map(|value| Some(Some(value.unwrap_or_else(String::new))))
        }
    }
}

#[cfg(test)]
mod tests {
    use codexmanager_core::storage::AggregateApi;
    use serde_json::Value;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;
    use tiny_http::{Response, Server, StatusCode};

    use super::{
        action_path_or_default, balance_query_paths, build_codex_models_probe_url,
        claude_probe_fallback_models_for_api, extract_model_ids_from_models_response,
        normalize_action_override, normalize_cost_multiplier, normalize_daily_spend_limit_usd,
        normalize_provider_type, normalize_provider_type_value, parse_balance_snapshot_with_provider,
        probe_claude_endpoint, probe_codex_endpoint, provider_default_url, query_balance_path,
        query_balance_request_url, OfficialBalanceProvider, AGGREGATE_API_PROVIDER_CLAUDE,
        AGGREGATE_API_PROVIDER_GEMINI, ALIBABA_CODING_PLAN_PROBE_MODEL, CLAUDE_DEFAULT_PROBE_MODEL,
    };

    fn aggregate_api_with_action(action: Option<&str>) -> AggregateApi {
        AggregateApi {
            id: "agg-test".to_string(),
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
        }
    }

    #[test]
    fn action_override_disabled_stays_none() {
        let value =
            normalize_action_override(Some(false), Some("/v1/messages".to_string())).unwrap();
        assert_eq!(value, Some(None));
    }

    #[test]
    fn action_override_enabled_and_empty_preserves_empty_string() {
        let value = normalize_action_override(Some(true), Some("   ".to_string())).unwrap();
        assert_eq!(value, Some(Some(String::new())));
    }

    #[test]
    fn cost_multiplier_defaults_to_one_and_rejects_invalid_values() {
        assert_eq!(normalize_cost_multiplier(None).unwrap(), 1.0);
        assert_eq!(normalize_cost_multiplier(Some(2.5)).unwrap(), 2.5);
        assert!(normalize_cost_multiplier(Some(0.0)).is_err());
        assert!(normalize_cost_multiplier(Some(101.0)).is_err());
        assert!(normalize_cost_multiplier(Some(f64::INFINITY)).is_err());
    }

    #[test]
    fn daily_spend_limit_normalizes_empty_and_rejects_invalid_values() {
        assert_eq!(normalize_daily_spend_limit_usd(None).unwrap(), None);
        assert_eq!(normalize_daily_spend_limit_usd(Some(0.0)).unwrap(), None);
        assert_eq!(
            normalize_daily_spend_limit_usd(Some(3.5)).unwrap(),
            Some(3.5)
        );
        assert!(normalize_daily_spend_limit_usd(Some(f64::INFINITY)).is_err());
        assert!(normalize_daily_spend_limit_usd(Some(1_000_000.01)).is_err());
    }

    #[test]
    fn empty_action_uses_base_url_without_default_path() {
        let api = aggregate_api_with_action(Some(""));
        let path = action_path_or_default(&api, "/v1/messages?beta=true");
        assert_eq!(path, "");
    }

    #[test]
    fn codex_models_probe_url_appends_client_version() {
        let _guard = crate::test_env_guard();
        crate::gateway::set_codex_user_agent_version("0.101.0")
            .expect("set default codex user agent version");
        let mut api = aggregate_api_with_action(None);
        api.url = "https://api.openai.com/v1".to_string();

        let url = build_codex_models_probe_url(&api);

        assert_eq!(
            url,
            "https://api.openai.com/v1/models?client_version=0.101.0"
        );
    }

    #[test]
    fn codex_models_probe_url_preserves_custom_action_with_client_version() {
        let _guard = crate::test_env_guard();
        crate::gateway::set_codex_user_agent_version("0.101.0")
            .expect("set default codex user agent version");
        let mut api = aggregate_api_with_action(Some("/models?limit=20"));
        api.url = "https://api.openai.com/v1".to_string();

        let url = build_codex_models_probe_url(&api);

        assert_eq!(
            url,
            "https://api.openai.com/v1/models?limit=20&client_version=0.101.0"
        );
    }

    #[test]
    fn gemini_provider_type_is_normalized_independently() {
        assert_eq!(
            normalize_provider_type(Some("gemini_native".to_string())).unwrap(),
            AGGREGATE_API_PROVIDER_GEMINI
        );
        assert_eq!(
            normalize_provider_type_value("google_gemini"),
            AGGREGATE_API_PROVIDER_GEMINI
        );
        assert_eq!(
            provider_default_url(AGGREGATE_API_PROVIDER_GEMINI),
            "https://generativelanguage.googleapis.com"
        );
        assert_eq!(
            normalize_provider_type(Some("claude".to_string())).unwrap(),
            AGGREGATE_API_PROVIDER_CLAUDE
        );
    }

    #[test]
    fn claude_probe_models_prefer_alibaba_coding_plan_model_for_dashscope() {
        let mut api = aggregate_api_with_action(None);
        api.url = "https://coding-intl.dashscope.aliyuncs.com/apps/anthropic".to_string();

        assert_eq!(
            claude_probe_fallback_models_for_api(&api),
            vec![ALIBABA_CODING_PLAN_PROBE_MODEL, CLAUDE_DEFAULT_PROBE_MODEL]
        );
    }

    #[test]
    fn claude_probe_models_keep_anthropic_default_first_for_generic_urls() {
        let mut api = aggregate_api_with_action(None);
        api.url = "https://api.anthropic.com/v1".to_string();

        assert_eq!(
            claude_probe_fallback_models_for_api(&api),
            vec![CLAUDE_DEFAULT_PROBE_MODEL, ALIBABA_CODING_PLAN_PROBE_MODEL]
        );
    }

    #[test]
    fn extract_model_ids_from_models_response_accepts_common_shapes() {
        let body = r#"{
            "data": [
                {"id":"provider-model-a"},
                {"model":"provider-model-b"},
                "provider-model-c"
            ],
            "models": [{"slug":"ignored-because-data-wins"}]
        }"#;

        assert_eq!(
            extract_model_ids_from_models_response(body),
            vec![
                "provider-model-a".to_string(),
                "provider-model-b".to_string(),
                "provider-model-c".to_string()
            ]
        );
    }

    #[test]
    fn balance_parser_accepts_deepseek_balance_shape() {
        let payload: Value = serde_json::from_str(
            r#"{
                "is_available": true,
                "balance_infos": [
                    { "total_balance": "12.50", "currency": "USD" }
                ]
            }"#,
        )
        .expect("parse payload");

        let balance = parse_balance_snapshot_with_provider(&payload, None).expect("balance");

        assert_eq!(balance.remaining, Some(12.5));
        assert_eq!(balance.unit.as_deref(), Some("USD"));
        assert_eq!(balance.message, None);
    }

    #[test]
    fn balance_parser_accepts_newapi_quota_shape() {
        let payload: Value = serde_json::from_str(
            r#"{
                "success": true,
                "data": {
                    "group": "pro",
                    "quota": 1000000,
                    "used_quota": 500000
                }
            }"#,
        )
        .expect("parse payload");

        let balance = parse_balance_snapshot_with_provider(&payload, None).expect("balance");

        assert_eq!(balance.remaining, Some(2.0));
        assert_eq!(balance.used, Some(1.0));
        assert_eq!(balance.total, Some(3.0));
        assert_eq!(balance.unit.as_deref(), Some("USD"));
        assert_eq!(balance.plan_name.as_deref(), Some("pro"));
    }

    #[test]
    fn balance_parser_ignores_success_json_without_balance_signal() {
        let payload: Value = serde_json::from_str(
            r#"{
                "success": true,
                "data": {
                    "id": "user-1",
                    "name": "Alice"
                }
            }"#,
        )
        .expect("parse payload");

        assert_eq!(parse_balance_snapshot_with_provider(&payload, None), None);
    }

    #[test]
    fn balance_query_paths_try_newapi_fallbacks_for_unlabeled_apis() {
        let mut api = aggregate_api_with_action(None);
        api.supplier_name = Some("custom upstream".to_string());
        api.url = "https://api.example.com/v1".to_string();

        assert_eq!(
            balance_query_paths(&api),
            vec!["/user/balance", "/api/usage/token", "/api/user/self"]
        );
    }

    #[test]
    fn balance_query_uses_official_deepseek_endpoint() {
        let mut api = aggregate_api_with_action(None);
        api.url = "https://api.deepseek.com/v1".to_string();

        assert_eq!(balance_query_paths(&api), vec!["/user/balance"]);
        assert_eq!(
            query_balance_request_url(&api, "/user/balance"),
            "https://api.deepseek.com/user/balance"
        );
    }

    #[test]
    fn balance_query_uses_official_openrouter_endpoint() {
        let mut api = aggregate_api_with_action(None);
        api.url = "https://openrouter.ai/api/v1".to_string();

        assert_eq!(balance_query_paths(&api), vec!["/api/v1/credits"]);
        assert_eq!(
            query_balance_request_url(&api, "/api/v1/credits"),
            "https://openrouter.ai/api/v1/credits"
        );
    }

    #[test]
    fn balance_parser_accepts_openrouter_credits_shape() {
        let payload: Value = serde_json::from_str(
            r#"{
                "data": {
                    "total_credits": 10,
                    "total_usage": 2.5
                }
            }"#,
        )
        .expect("parse payload");

        let balance = parse_balance_snapshot_with_provider(&payload, None).expect("balance");

        assert_eq!(balance.remaining, Some(7.5));
        assert_eq!(balance.used, Some(2.5));
        assert_eq!(balance.total, Some(10.0));
        assert_eq!(balance.unit.as_deref(), Some("USD"));
    }

    #[test]
    fn balance_parser_accepts_siliconflow_nested_shape() {
        let payload: Value = serde_json::from_str(
            r#"{
                "code": 20000,
                "data": {
                    "totalBalance": "3.25",
                    "status": "normal"
                }
            }"#,
        )
        .expect("parse payload");

        let balance = parse_balance_snapshot_with_provider(&payload, None).expect("balance");

        assert_eq!(balance.remaining, Some(3.25));
        assert_eq!(balance.unit.as_deref(), Some("CNY"));
        assert_eq!(balance.plan_name.as_deref(), Some("SiliconFlow"));
    }

    #[test]
    fn balance_parser_uses_usd_for_global_siliconflow() {
        let payload: Value = serde_json::from_str(
            r#"{
                "code": 20000,
                "data": {
                    "totalBalance": "4.50"
                }
            }"#,
        )
        .expect("parse payload");

        let balance = parse_balance_snapshot_with_provider(
            &payload,
            Some(OfficialBalanceProvider::SiliconFlowGlobal),
        )
        .expect("balance");

        assert_eq!(balance.remaining, Some(4.5));
        assert_eq!(balance.unit.as_deref(), Some("USD"));
    }

    #[test]
    fn balance_parser_accepts_novita_minor_unit_shape() {
        let payload: Value = serde_json::from_str(
            r#"{
                "availableBalance": 125000,
                "cashBalance": 100000
            }"#,
        )
        .expect("parse payload");

        let balance = parse_balance_snapshot_with_provider(&payload, None).expect("balance");

        assert_eq!(balance.remaining, Some(12.5));
        assert_eq!(balance.unit.as_deref(), Some("USD"));
        assert_eq!(balance.plan_name.as_deref(), Some("Novita AI"));
    }

    #[test]
    fn balance_query_error_does_not_leak_query_secret_or_body() {
        let server = Server::http("127.0.0.1:0").expect("start mock server");
        let base_url = format!("http://{}", server.server_addr());
        let (tx, rx) = mpsc::channel();
        let join = thread::spawn(move || {
            let request = server
                .recv_timeout(Duration::from_secs(2))
                .expect("receive balance request")
                .expect("balance request present");
            tx.send(request.url().to_string())
                .expect("send captured balance request");
            request
                .respond(
                    Response::from_string(r#"{"error":"secret sk-balance-secret leaked"}"#)
                        .with_status_code(StatusCode(401)),
                )
                .expect("respond balance request");
        });

        let mut api = aggregate_api_with_action(None);
        api.url = base_url.clone();
        api.auth_params_json =
            Some(r#"{"location":"query","name":"api_key","headerValueFormat":"raw"}"#.to_string());
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("build client");

        let err = query_balance_path(&client, &api, "sk-balance-secret", "/user/balance")
            .expect_err("balance request should fail");

        let captured = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("captured request");
        join.join().expect("join mock server");
        assert!(captured.contains("api_key=sk-balance-secret"));
        assert!(err.contains("/user/balance"));
        assert!(!err.contains("sk-balance-secret"));
        assert!(!err.contains("api_key="));
        assert!(!err.contains("leaked"));
    }

    #[test]
    fn claude_probe_uses_configured_model_without_model_discovery() {
        let server = Server::http("127.0.0.1:0").expect("start mock server");
        let base_url = format!("http://{}", server.server_addr());
        let (tx, rx) = mpsc::channel();
        let join = thread::spawn(move || {
            let mut request = server
                .recv_timeout(Duration::from_secs(2))
                .expect("receive messages request")
                .expect("messages request present");
            let mut body = String::new();
            request
                .as_reader()
                .read_to_string(&mut body)
                .expect("read request body");
            tx.send((
                request.method().as_str().to_string(),
                request.url().to_string(),
                body,
            ))
            .expect("send messages request");
            request
                .respond(Response::from_string(
                    r#"{"id":"msg_probe","type":"message"}"#,
                ))
                .expect("respond messages");
        });

        let mut api = aggregate_api_with_action(None);
        api.url = base_url;
        api.model_override = Some("qwen3.5-plus".to_string());
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("build client");

        let status = probe_claude_endpoint(&client, &api, "secret").expect("probe succeeds");

        assert_eq!(status, 200);
        let captured = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("captured request");
        join.join().expect("join mock server");
        assert_eq!(captured.0, "POST");
        assert_eq!(captured.1, "/v1/messages?beta=true");
        let body: Value = serde_json::from_str(captured.2.as_str()).expect("parse body");
        assert_eq!(body["model"], "qwen3.5-plus");
    }

    #[test]
    fn codex_probe_uses_configured_model_without_model_discovery() {
        let server = Server::http("127.0.0.1:0").expect("start mock server");
        let base_url = format!("http://{}", server.server_addr());
        let (tx, rx) = mpsc::channel();
        let join = thread::spawn(move || {
            let mut request = server
                .recv_timeout(Duration::from_secs(2))
                .expect("receive chat completions request")
                .expect("chat completions request present");
            let mut body = String::new();
            request
                .as_reader()
                .read_to_string(&mut body)
                .expect("read request body");
            tx.send((
                request.method().as_str().to_string(),
                request.url().to_string(),
                body,
            ))
            .expect("send chat completions request");
            request
                .respond(Response::from_string(r#"{"id":"chatcmpl_probe"}"#))
                .expect("respond chat completions");
        });

        let mut api = aggregate_api_with_action(Some("/chat/completions"));
        api.url = base_url;
        api.model_override = Some("qwen3.5-plus".to_string());
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("build client");

        let status = probe_codex_endpoint(&client, &api, "secret").expect("probe succeeds");

        assert_eq!(status, 200);
        let captured = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("captured request");
        join.join().expect("join mock server");
        assert_eq!(captured.0, "POST");
        assert_eq!(captured.1, "/v1/chat/completions");
        let body: Value = serde_json::from_str(captured.2.as_str()).expect("parse body");
        assert_eq!(body["model"], "qwen3.5-plus");
    }

    #[test]
    fn claude_probe_uses_discovered_model_before_fallbacks() {
        let server = Server::http("127.0.0.1:0").expect("start mock server");
        let base_url = format!("http://{}", server.server_addr());
        let (tx, rx) = mpsc::channel();
        let join = thread::spawn(move || {
            let request = server
                .recv_timeout(Duration::from_secs(2))
                .expect("receive model list request")
                .expect("model list request present");
            tx.send((
                request.method().as_str().to_string(),
                request.url().to_string(),
                None,
            ))
            .expect("send model list request");
            request
                .respond(Response::from_string(
                    r#"{"data":[{"id":"provider-model"}]}"#,
                ))
                .expect("respond model list");

            let mut request = server
                .recv_timeout(Duration::from_secs(2))
                .expect("receive messages request")
                .expect("messages request present");
            let mut body = String::new();
            request
                .as_reader()
                .read_to_string(&mut body)
                .expect("read request body");
            tx.send((
                request.method().as_str().to_string(),
                request.url().to_string(),
                Some(body),
            ))
            .expect("send messages request");
            request
                .respond(Response::from_string(
                    r#"{"id":"msg_probe","type":"message"}"#,
                ))
                .expect("respond messages");
        });

        let mut api = aggregate_api_with_action(None);
        api.url = base_url;
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("build client");

        let status = probe_claude_endpoint(&client, &api, "secret").expect("probe succeeds");

        assert_eq!(status, 200);
        let first = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("first captured request");
        let second = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("second captured request");
        join.join().expect("join mock server");
        assert_eq!(first.0, "GET");
        assert_eq!(first.1, "/v1/models");
        assert_eq!(second.0, "POST");
        assert_eq!(second.1, "/v1/messages?beta=true");
        let second_body: Value =
            serde_json::from_str(second.2.as_deref().expect("second body")).expect("parse body");
        assert_eq!(second_body["model"], "provider-model");
    }

    #[test]
    fn claude_probe_retries_with_alibaba_model_after_default_model_bad_request() {
        let server = Server::http("127.0.0.1:0").expect("start mock server");
        let base_url = format!("http://{}/apps/anthropic", server.server_addr());
        let (tx, rx) = mpsc::channel();
        let join = thread::spawn(move || {
            let request = server
                .recv_timeout(Duration::from_secs(2))
                .expect("receive model list request")
                .expect("model list request present");
            tx.send((request.url().to_string(), String::new()))
                .expect("send captured model list request");
            request
                .respond(Response::from_string("").with_status_code(StatusCode(404)))
                .expect("respond model list request");

            for (index, status) in [400u16, 200u16].into_iter().enumerate() {
                let mut request = server
                    .recv_timeout(Duration::from_secs(2))
                    .expect("receive mock request")
                    .expect("mock request present");
                let mut body = String::new();
                request
                    .as_reader()
                    .read_to_string(&mut body)
                    .expect("read request body");
                tx.send((request.url().to_string(), body))
                    .expect("send captured request");
                let response_body = if index == 0 {
                    r#"{"error":{"message":"model not found"}}"#
                } else {
                    r#"{"id":"msg_probe","type":"message","content":[]}"#
                };
                request
                    .respond(
                        Response::from_string(response_body).with_status_code(StatusCode(status)),
                    )
                    .expect("respond mock request");
            }
        });

        let mut api = aggregate_api_with_action(None);
        api.url = base_url;
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("build client");

        let status = probe_claude_endpoint(&client, &api, "secret").expect("probe succeeds");

        assert_eq!(status, 200);
        let models_request = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("captured model list request");
        let first = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("first captured request");
        let second = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("second captured request");
        join.join().expect("join mock server");
        assert_eq!(models_request.0, "/apps/anthropic/v1/models");
        assert_eq!(first.0, "/apps/anthropic/v1/messages?beta=true");
        assert_eq!(second.0, "/apps/anthropic/v1/messages?beta=true");
        let first_body: Value = serde_json::from_str(first.1.as_str()).expect("parse first body");
        let second_body: Value =
            serde_json::from_str(second.1.as_str()).expect("parse second body");
        assert_eq!(first_body["model"], CLAUDE_DEFAULT_PROBE_MODEL);
        assert_eq!(second_body["model"], ALIBABA_CODING_PLAN_PROBE_MODEL);
    }
}

fn serialize_userpass_secret(username: &str, password: &str) -> Result<String, String> {
    let secret = UserPassSecret {
        username: username.trim().to_string(),
        password: password.trim().to_string(),
    };
    serde_json::to_string(&secret).map_err(|_| "invalid username/password".to_string())
}

fn action_path_or_default(api: &AggregateApi, default: &str) -> String {
    match api.action.as_deref().map(str::trim) {
        Some("") => String::new(),
        Some(value) => {
            if value.starts_with('/') {
                value.to_string()
            } else {
                format!("/{value}")
            }
        }
        None => default.to_string(),
    }
}

fn with_query_param(url: &str, name: &str, value: &str) -> String {
    let mut parsed = match reqwest::Url::parse(url) {
        Ok(value) => value,
        Err(_) => return url.to_string(),
    };
    let existing = parsed.query_pairs().into_owned().collect::<Vec<_>>();
    parsed.set_query(None);
    {
        let mut query = parsed.query_pairs_mut();
        for (key, val) in existing {
            if key == name {
                continue;
            }
            query.append_pair(key.as_str(), val.as_str());
        }
        query.append_pair(name, value);
    }
    parsed.to_string()
}

fn apply_probe_auth(
    mut builder: reqwest::blocking::RequestBuilder,
    mut url: String,
    api: &AggregateApi,
    secret: &str,
) -> Result<(reqwest::blocking::RequestBuilder, String), String> {
    let auth_type = normalize_auth_type(Some(api.auth_type.clone()))?;
    let auth_params = api
        .auth_params_json
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if auth_type == AGGREGATE_API_AUTH_USERPASS {
        let parsed: UserPassSecret = serde_json::from_str(secret.trim())
            .map_err(|_| "invalid aggregate api secret".to_string())?;
        if let Some(raw) = auth_params {
            let params: UserPassAuthParams =
                serde_json::from_str(raw).map_err(|_| "invalid authParams".to_string())?;
            let mode = params.mode.trim().to_ascii_lowercase();
            if mode == "headerpair" {
                let username_name = params.username_name.as_deref().unwrap_or("username").trim();
                let password_name = params.password_name.as_deref().unwrap_or("password").trim();
                builder = builder
                    .header(username_name, parsed.username.as_str())
                    .header(password_name, parsed.password.as_str());
                return Ok((builder, url));
            }
            if mode == "querypair" {
                let username_name = params.username_name.as_deref().unwrap_or("username").trim();
                let password_name = params.password_name.as_deref().unwrap_or("password").trim();
                url = with_query_param(url.as_str(), username_name, parsed.username.as_str());
                url = with_query_param(url.as_str(), password_name, parsed.password.as_str());
                return Ok((builder, url));
            }
        }
        builder = builder.basic_auth(parsed.username, Some(parsed.password));
        return Ok((builder, url));
    }

    if let Some(raw) = auth_params {
        let params: ApiKeyAuthParams =
            serde_json::from_str(raw).map_err(|_| "invalid authParams".to_string())?;
        let location = params.location.trim().to_ascii_lowercase();
        if location == "query" {
            url = with_query_param(url.as_str(), params.name.trim(), secret.trim());
            return Ok((builder, url));
        }
        let value_format = params
            .header_value_format
            .as_deref()
            .unwrap_or("bearer")
            .trim()
            .to_ascii_lowercase();
        let header_value = if value_format == "raw" {
            secret.trim().to_string()
        } else {
            format!("Bearer {}", secret.trim())
        };
        builder = builder.header(params.name.trim(), header_value);
        return Ok((builder, url));
    }

    let auth_value = format!("Bearer {}", secret.trim());
    builder = builder
        .header(
            HeaderName::from_static("authorization"),
            HeaderValue::from_str(auth_value.as_str())
                .map_err(|_| "invalid aggregate api key".to_string())?,
        )
        .header("x-api-key", secret.trim())
        .header("api-key", secret.trim());
    Ok((builder, url))
}

fn number_from_value(value: Option<&serde_json::Value>) -> Option<f64> {
    match value {
        Some(serde_json::Value::Number(number)) => number.as_f64(),
        Some(serde_json::Value::String(raw)) => raw.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn string_from_value(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn official_balance_provider(base_url: &str) -> Option<OfficialBalanceProvider> {
    let url = base_url.to_ascii_lowercase();
    if url.contains("api.deepseek.com") {
        Some(OfficialBalanceProvider::DeepSeek)
    } else if url.contains("api.stepfun.ai") || url.contains("api.stepfun.com") {
        Some(OfficialBalanceProvider::StepFun)
    } else if url.contains("api.siliconflow.cn") {
        Some(OfficialBalanceProvider::SiliconFlowCn)
    } else if url.contains("api.siliconflow.com") {
        Some(OfficialBalanceProvider::SiliconFlowGlobal)
    } else if url.contains("openrouter.ai") {
        Some(OfficialBalanceProvider::OpenRouter)
    } else if url.contains("api.novita.ai") {
        Some(OfficialBalanceProvider::Novita)
    } else {
        None
    }
}

fn official_balance_endpoint(provider: OfficialBalanceProvider) -> &'static str {
    match provider {
        OfficialBalanceProvider::DeepSeek => "https://api.deepseek.com/user/balance",
        OfficialBalanceProvider::StepFun => "https://api.stepfun.com/v1/accounts",
        OfficialBalanceProvider::SiliconFlowCn => "https://api.siliconflow.cn/v1/user/info",
        OfficialBalanceProvider::SiliconFlowGlobal => "https://api.siliconflow.com/v1/user/info",
        OfficialBalanceProvider::OpenRouter => "https://openrouter.ai/api/v1/credits",
        OfficialBalanceProvider::Novita => "https://api.novita.ai/v3/user/balance",
    }
}

fn parse_deepseek_balance(value: &serde_json::Value) -> Option<BalanceSnapshot> {
    let is_valid = value
        .get("is_available")
        .or_else(|| value.get("is_active"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let first_balance = value
        .get("balance_infos")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first());
    let remaining = first_balance
        .and_then(|item| number_from_value(item.get("total_balance")))
        .or_else(|| number_from_value(value.get("balance")))
        .or_else(|| number_from_value(value.pointer("/credits/balance")));
    remaining?;
    let unit = first_balance
        .and_then(|item| string_from_value(item.get("currency")))
        .or_else(|| string_from_value(value.get("currency")))
        .or_else(|| Some(DEFAULT_BALANCE_UNIT.to_string()));
    let message = if is_valid {
        None
    } else {
        Some("balance api reported unavailable".to_string())
    };

    Some(BalanceSnapshot {
        remaining,
        used: None,
        total: None,
        unit,
        plan_name: None,
        message,
    })
}

fn parse_newapi_balance(value: &serde_json::Value) -> Option<BalanceSnapshot> {
    let data = value.get("data").unwrap_or(value);
    let success = value
        .get("success")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    if !success {
        return Some(BalanceSnapshot {
            remaining: None,
            used: None,
            total: None,
            unit: Some(DEFAULT_BALANCE_UNIT.to_string()),
            plan_name: None,
            message: string_from_value(value.get("message")).or_else(|| {
                string_from_value(value.get("error"))
                    .or_else(|| Some("balance query failed".to_string()))
            }),
        });
    }

    let has_balance_signal = [
        "quota",
        "remaining_quota",
        "used_quota",
        "total_quota",
        "remaining",
        "used",
        "total",
    ]
    .iter()
    .any(|key| data.get(*key).is_some());
    if !has_balance_signal {
        return None;
    }

    let remaining = number_from_value(data.get("quota"))
        .map(|value| value / 500000.0)
        .or_else(|| number_from_value(data.get("remaining_quota")).map(|value| value / 500000.0))
        .or_else(|| number_from_value(data.get("remaining")));
    let used = number_from_value(data.get("used_quota"))
        .map(|value| value / 500000.0)
        .or_else(|| number_from_value(data.get("used")));
    let total = match (remaining, used) {
        (Some(remaining), Some(used)) => Some(remaining + used),
        _ => number_from_value(data.get("total_quota"))
            .map(|value| value / 500000.0)
            .or_else(|| number_from_value(data.get("total"))),
    };

    Some(BalanceSnapshot {
        remaining,
        used,
        total,
        unit: Some(DEFAULT_BALANCE_UNIT.to_string()),
        plan_name: string_from_value(data.get("group"))
            .or_else(|| string_from_value(data.get("plan_name"))),
        message: None,
    })
}

fn parse_generic_balance(value: &serde_json::Value) -> Option<BalanceSnapshot> {
    let remaining = number_from_value(value.get("balance"))
        .or_else(|| number_from_value(value.get("remaining")))
        .or_else(|| number_from_value(value.get("credit")))
        .or_else(|| number_from_value(value.pointer("/credits/balance")));
    remaining.map(|remaining| BalanceSnapshot {
        remaining: Some(remaining),
        used: number_from_value(value.get("used")),
        total: number_from_value(value.get("total")),
        unit: string_from_value(value.get("unit"))
            .or_else(|| string_from_value(value.get("currency")))
            .or_else(|| Some(DEFAULT_BALANCE_UNIT.to_string())),
        plan_name: string_from_value(value.get("planName"))
            .or_else(|| string_from_value(value.get("plan_name"))),
        message: None,
    })
}

fn parse_stepfun_balance(value: &serde_json::Value) -> Option<BalanceSnapshot> {
    let has_stepfun_signal = value.get("object").is_some()
        || value.get("total_cash_balance").is_some()
        || value.get("total_voucher_balance").is_some();
    if !has_stepfun_signal {
        return None;
    }
    let remaining = number_from_value(value.get("balance"))?;
    Some(BalanceSnapshot {
        remaining: Some(remaining),
        used: None,
        total: None,
        unit: Some("CNY".to_string()),
        plan_name: Some("StepFun".to_string()),
        message: None,
    })
}

fn parse_siliconflow_balance(
    value: &serde_json::Value,
    provider: Option<OfficialBalanceProvider>,
) -> Option<BalanceSnapshot> {
    let data = value.get("data")?;
    let remaining = number_from_value(data.get("totalBalance"))
        .or_else(|| number_from_value(data.get("balance")))
        .or_else(|| number_from_value(data.get("chargeBalance")))?;
    let unit = if provider == Some(OfficialBalanceProvider::SiliconFlowGlobal) {
        "USD"
    } else {
        "CNY"
    };
    Some(BalanceSnapshot {
        remaining: Some(remaining),
        used: None,
        total: None,
        unit: Some(unit.to_string()),
        plan_name: Some("SiliconFlow".to_string()),
        message: None,
    })
}

fn parse_openrouter_balance(value: &serde_json::Value) -> Option<BalanceSnapshot> {
    let data = value.get("data").unwrap_or(value);
    let total = number_from_value(data.get("total_credits"))?;
    let used = number_from_value(data.get("total_usage")).unwrap_or(0.0);
    let remaining = total - used;
    Some(BalanceSnapshot {
        remaining: Some(remaining),
        used: Some(used),
        total: Some(total),
        unit: Some(DEFAULT_BALANCE_UNIT.to_string()),
        plan_name: Some("OpenRouter".to_string()),
        message: if remaining > 0.0 {
            None
        } else {
            Some("No credits remaining".to_string())
        },
    })
}

fn parse_novita_balance(value: &serde_json::Value) -> Option<BalanceSnapshot> {
    let remaining = number_from_value(value.get("availableBalance"))? / 10000.0;
    Some(BalanceSnapshot {
        remaining: Some(remaining),
        used: None,
        total: None,
        unit: Some(DEFAULT_BALANCE_UNIT.to_string()),
        plan_name: Some("Novita AI".to_string()),
        message: if remaining > 0.0 {
            None
        } else {
            Some("No balance remaining".to_string())
        },
    })
}

fn parse_balance_snapshot_with_provider(
    value: &serde_json::Value,
    provider: Option<OfficialBalanceProvider>,
) -> Option<BalanceSnapshot> {
    parse_deepseek_balance(value)
        .or_else(|| parse_stepfun_balance(value))
        .or_else(|| parse_siliconflow_balance(value, provider))
        .or_else(|| parse_openrouter_balance(value))
        .or_else(|| parse_novita_balance(value))
        .or_else(|| parse_newapi_balance(value))
        .or_else(|| parse_generic_balance(value))
}

fn balance_query_paths(api: &AggregateApi) -> Vec<&'static str> {
    if let Some(provider) = official_balance_provider(api.url.as_str()) {
        return match provider {
            OfficialBalanceProvider::DeepSeek => vec!["/user/balance"],
            OfficialBalanceProvider::StepFun => vec!["/v1/accounts"],
            OfficialBalanceProvider::SiliconFlowCn | OfficialBalanceProvider::SiliconFlowGlobal => {
                vec!["/v1/user/info"]
            }
            OfficialBalanceProvider::OpenRouter => vec!["/api/v1/credits"],
            OfficialBalanceProvider::Novita => vec!["/v3/user/balance"],
        };
    }
    let text =
        format!("{} {}", api.supplier_name.as_deref().unwrap_or(""), api.url).to_ascii_lowercase();
    if text.contains("newapi") || text.contains("new-api") {
        return vec!["/api/usage/token", "/api/user/self", "/user/balance"];
    }
    vec!["/user/balance", "/api/usage/token", "/api/user/self"]
}

fn query_balance_request_url(api: &AggregateApi, path: &str) -> String {
    if let Some(provider) = official_balance_provider(api.url.as_str()) {
        return official_balance_endpoint(provider).to_string();
    }
    normalize_probe_url(api.url.as_str(), path)
}

fn query_balance_path(
    client: &reqwest::blocking::Client,
    api: &AggregateApi,
    secret: &str,
    path: &str,
) -> Result<(i64, BalanceSnapshot), String> {
    let provider = official_balance_provider(api.url.as_str());
    let url = query_balance_request_url(api, path);
    let builder = client
        .get(url.as_str())
        .header("accept", "application/json")
        .header("user-agent", "codex-manager/1.0");
    let (request, updated_url) = apply_probe_auth(builder, url.clone(), api, secret)?;
    let request = if updated_url != url {
        let rebuilt = client
            .get(updated_url.as_str())
            .header("accept", "application/json")
            .header("user-agent", "codex-manager/1.0");
        let (rebuilt, _) = apply_probe_auth(rebuilt, updated_url, api, secret)?;
        rebuilt
    } else {
        request
    };
    let response = request
        .send()
        .map_err(|err| format!("balance request failed: {err}"))?;
    let status = response.status().as_u16() as i64;
    let body = response
        .text()
        .map_err(|err| format!("read balance response failed: {err}"))?;
    if !(200..300).contains(&status) {
        let safe_url = query_balance_request_url(api, path);
        return Err(format!(
            "balance endpoint {safe_url} returned HTTP {status}"
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body.as_str())
        .map_err(|err| format!("invalid balance JSON: {err}"))?;
    let snapshot = parse_balance_snapshot_with_provider(&value, provider)
        .ok_or_else(|| "balance response does not contain a supported balance shape".to_string())?;
    Ok((status, snapshot))
}

/// 函数 `normalize_provider_type`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - value: 参数 value
///
/// # 返回
/// 返回函数执行结果
fn normalize_provider_type(value: Option<String>) -> Result<String, String> {
    match value {
        Some(raw) => {
            let normalized = raw.trim().to_ascii_lowercase().replace('-', "_");
            match normalized.as_str() {
                "codex" | "openai" | "openai_compat" | "gpt" => {
                    Ok(AGGREGATE_API_PROVIDER_CODEX.to_string())
                }
                "gemini" | "gemini_native" | "google" | "google_ai" | "google_gemini" => {
                    Ok(AGGREGATE_API_PROVIDER_GEMINI.to_string())
                }
                "claude" | "anthropic" | "anthropic_native" | "claude_code" => {
                    Ok(AGGREGATE_API_PROVIDER_CLAUDE.to_string())
                }
                other => Err(format!("unsupported aggregate api provider type: {other}")),
            }
        }
        None => Ok(AGGREGATE_API_PROVIDER_CODEX.to_string()),
    }
}

/// 函数 `normalize_provider_type_value`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - value: 参数 value
///
/// # 返回
/// 返回函数执行结果
fn normalize_provider_type_value(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase().replace('-', "_");
    match normalized.as_str() {
        "claude" | "anthropic" | "anthropic_native" | "claude_code" => {
            AGGREGATE_API_PROVIDER_CLAUDE.to_string()
        }
        "gemini" | "gemini_native" | "google" | "google_ai" | "google_gemini" => {
            AGGREGATE_API_PROVIDER_GEMINI.to_string()
        }
        _ => AGGREGATE_API_PROVIDER_CODEX.to_string(),
    }
}

/// 函数 `provider_default_url`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - provider_type: 参数 provider_type
///
/// # 返回
/// 返回函数执行结果
fn provider_default_url(provider_type: &str) -> &'static str {
    match provider_type {
        AGGREGATE_API_PROVIDER_CLAUDE => "https://api.anthropic.com/v1",
        AGGREGATE_API_PROVIDER_GEMINI => "https://generativelanguage.googleapis.com",
        _ => "https://api.openai.com/v1",
    }
}

/// 函数 `normalize_probe_url`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - base_url: 参数 base_url
/// - suffix: 参数 suffix
///
/// # 返回
/// 返回函数执行结果
fn normalize_probe_url(base_url: &str, suffix: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if suffix.trim().is_empty() {
        return base.to_string();
    }
    if base.ends_with("/v1") {
        format!("{base}{suffix}")
    } else {
        format!("{base}/v1{suffix}")
    }
}

/// 函数 `read_first_chunk`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - response: 参数 response
///
/// # 返回
/// 返回函数执行结果
fn read_first_chunk(mut response: reqwest::blocking::Response) -> Result<(), String> {
    let mut buf = [0u8; 16];
    let read = response.read(&mut buf).map_err(|err| err.to_string())?;
    if read > 0 {
        Ok(())
    } else {
        Err("No response data received".to_string())
    }
}

/// 函数 `build_claude_probe_body`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
fn build_claude_probe_body(model: &str) -> serde_json::Value {
    json!({
        "model": model,
        "max_tokens": 1,
        "messages": [{
            "role": "user",
            "content": "Who are you?"
        }],
        "stream": true
    })
}

/// 函数 `build_codex_probe_body`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
fn build_codex_probe_body() -> serde_json::Value {
    json!({
        "model": "gpt-5.1-codex",
        "input": [{
            "role": "user",
            "content": [{
                "type": "text",
                "text": "Who are you?"
            }]
        }],
        "stream": true
    })
}

fn build_gemini_probe_body() -> serde_json::Value {
    json!({
        "contents": [{
            "role": "user",
            "parts": [{
                "text": "Who are you?"
            }]
        }],
        "generationConfig": {
            "maxOutputTokens": 1
        }
    })
}

fn append_client_version_query(url: &str) -> String {
    if url.contains("client_version=") {
        return url.to_string();
    }
    let separator = if url.contains('?') { '&' } else { '?' };
    format!(
        "{url}{separator}client_version={}",
        gateway::current_codex_user_agent_version()
    )
}

/// 函数 `probe_codex_only_for_provider`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - provider_type: 参数 provider_type
///
/// # 返回
/// 返回函数执行结果
fn probe_codex_only_for_provider(provider_type: &str) -> bool {
    !matches!(
        provider_type,
        AGGREGATE_API_PROVIDER_CLAUDE | AGGREGATE_API_PROVIDER_GEMINI
    )
}

fn is_alibaba_claude_compat_url(url: &str) -> bool {
    let normalized = url.trim().to_ascii_lowercase();
    normalized.contains("dashscope.aliyuncs.com/apps/anthropic")
        || normalized.contains("dashscope-intl.aliyuncs.com/apps/anthropic")
        || normalized.contains("coding-intl.dashscope.aliyuncs.com/apps/anthropic")
}

fn claude_probe_fallback_models_for_api(api: &AggregateApi) -> Vec<&'static str> {
    if is_alibaba_claude_compat_url(api.url.as_str()) {
        return vec![ALIBABA_CODING_PLAN_PROBE_MODEL, CLAUDE_DEFAULT_PROBE_MODEL];
    }
    vec![CLAUDE_DEFAULT_PROBE_MODEL, ALIBABA_CODING_PLAN_PROBE_MODEL]
}

fn push_unique_model(models: &mut Vec<String>, model: &str) {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return;
    }
    if !models.iter().any(|item| item == trimmed) {
        models.push(trimmed.to_string());
    }
}

fn model_id_from_value(value: &serde_json::Value) -> Option<String> {
    if let Some(model) = value.as_str() {
        let trimmed = model.trim();
        return if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
    }
    let object = value.as_object()?;
    ["id", "model", "slug", "name"]
        .iter()
        .find_map(|key| object.get(*key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_string)
}

fn extract_model_ids_from_models_response(body: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    let items = value
        .get("data")
        .and_then(serde_json::Value::as_array)
        .or_else(|| value.get("models").and_then(serde_json::Value::as_array))
        .or_else(|| value.as_array());
    let Some(items) = items else {
        return Vec::new();
    };
    let mut models = Vec::new();
    for item in items {
        if models.len() >= MAX_DISCOVERED_CLAUDE_PROBE_MODELS {
            break;
        }
        if let Some(model) = model_id_from_value(item) {
            push_unique_model(&mut models, model.as_str());
        }
    }
    models
}

fn add_claude_probe_headers(
    builder: reqwest::blocking::RequestBuilder,
) -> reqwest::blocking::RequestBuilder {
    builder
        .header("anthropic-version", "2023-06-01")
        .header(
            "anthropic-beta",
            "claude-code-20250219,interleaved-thinking-2025-05-14",
        )
        .header("accept", "application/json")
        .header("accept-encoding", "identity")
        .header("user-agent", "claude-cli/2.1.2 (external, cli)")
        .header("x-app", "cli")
}

fn build_claude_models_probe_url(api: &AggregateApi) -> String {
    normalize_probe_url(api.url.as_str(), "/models")
}

fn probe_claude_models_endpoint(
    client: &reqwest::blocking::Client,
    api: &AggregateApi,
    secret: &str,
) -> Result<Vec<String>, String> {
    let url = build_claude_models_probe_url(api);
    let builder = client.get(url.as_str());
    let (builder, updated_url) = apply_probe_auth(builder, url.clone(), api, secret)?;
    let builder = if updated_url != url {
        let rebuilt = client.get(updated_url.as_str());
        let (rebuilt, _) = apply_probe_auth(rebuilt, updated_url, api, secret)?;
        rebuilt
    } else {
        builder
    };
    let response = add_claude_probe_headers(builder)
        .send()
        .map_err(|err| err.to_string())?;

    let status_code = response.status().as_u16() as i64;
    if !response.status().is_success() {
        return Err(format!("claude models probe http_status={status_code}"));
    }
    let body = response.text().map_err(|err| err.to_string())?;
    let models = extract_model_ids_from_models_response(body.as_str());
    if models.is_empty() {
        return Err("claude models probe returned empty model list".to_string());
    }
    Ok(models)
}

fn claude_probe_models_for_api(
    client: &reqwest::blocking::Client,
    api: &AggregateApi,
    secret: &str,
) -> (Vec<String>, Option<String>) {
    let mut models = Vec::new();
    let discovery_error = match probe_claude_models_endpoint(client, api, secret) {
        Ok(discovered) => {
            for model in discovered {
                push_unique_model(&mut models, model.as_str());
            }
            None
        }
        Err(err) => Some(err),
    };
    for fallback in claude_probe_fallback_models_for_api(api) {
        push_unique_model(&mut models, fallback);
    }
    (models, discovery_error)
}

fn should_retry_claude_probe_with_next_model(status_code: u16) -> bool {
    status_code == 400
}

/// 函数 `add_codex_probe_headers`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - builder: 参数 builder
/// - secret: 参数 secret
///
/// # 返回
/// 返回函数执行结果
fn add_codex_probe_headers(
    builder: reqwest::blocking::RequestBuilder,
) -> Result<reqwest::blocking::RequestBuilder, String> {
    Ok(builder
        .header("accept", "application/json")
        .header("user-agent", gateway::current_codex_user_agent())
        .header("originator", gateway::current_wire_originator())
        .header("accept-encoding", "identity"))
}

fn build_codex_models_probe_url(api: &AggregateApi) -> String {
    let probe_path = action_path_or_default(api, "/models");
    let url = normalize_probe_url(api.url.as_str(), probe_path.as_str());
    append_client_version_query(url.as_str())
}

/// 函数 `probe_codex_models_endpoint`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - client: 参数 client
/// - base_url: 参数 base_url
/// - secret: 参数 secret
///
/// # 返回
/// 返回函数执行结果
fn probe_codex_models_endpoint(
    client: &reqwest::blocking::Client,
    api: &AggregateApi,
    secret: &str,
) -> Result<i64, String> {
    let url = build_codex_models_probe_url(api);
    let builder = client.get(url.as_str());
    let (builder, updated_url) = apply_probe_auth(builder, url.clone(), api, secret)?;
    let builder = if updated_url != url {
        let rebuilt = client.get(updated_url.as_str());
        let (rebuilt, _) = apply_probe_auth(rebuilt, updated_url, api, secret)?;
        rebuilt
    } else {
        builder
    };
    let response = add_codex_probe_headers(builder)?
        .send()
        .map_err(|err| err.to_string())?;

    let status_code = response.status().as_u16() as i64;
    if !response.status().is_success() {
        return Err(format!("codex models probe http_status={status_code}"));
    }
    read_first_chunk(response)?;
    Ok(status_code)
}

/// 函数 `probe_codex_responses_endpoint`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - client: 参数 client
/// - base_url: 参数 base_url
/// - secret: 参数 secret
///
/// # 返回
/// 返回函数执行结果
fn probe_codex_responses_endpoint(
    client: &reqwest::blocking::Client,
    api: &AggregateApi,
    secret: &str,
) -> Result<i64, String> {
    let action_hint = api
        .action
        .as_deref()
        .map(str::trim)
        .unwrap_or("/responses")
        .to_ascii_lowercase();
    let default_path = if action_hint.contains("chat/completions") {
        "/chat/completions"
    } else if action_hint.contains("responses") {
        "/responses"
    } else {
        "/responses"
    };
    let probe_path = action_path_or_default(api, default_path);
    let url = normalize_probe_url(api.url.as_str(), probe_path.as_str());
    let builder = client.post(url.as_str());
    let (builder, updated_url) = apply_probe_auth(builder, url.clone(), api, secret)?;
    let builder = if updated_url != url {
        let rebuilt = client.post(updated_url.as_str());
        let (rebuilt, _) = apply_probe_auth(rebuilt, updated_url, api, secret)?;
        rebuilt
    } else {
        builder
    };
    let request_body = if probe_path.to_ascii_lowercase().contains("chat/completions") {
        json!({
            "model": api.model_override.as_deref().unwrap_or("gpt-4o-mini"),
            "messages": [{"role":"user","content":"hi"}],
            "stream": false
        })
    } else if let Some(model_override) = api.model_override.as_deref() {
        let mut body = build_codex_probe_body();
        if let Some(obj) = body.as_object_mut() {
            obj.insert("model".to_string(), json!(model_override));
        }
        body
    } else {
        build_codex_probe_body()
    };
    let response = add_codex_probe_headers(builder)?
        .header("content-type", "application/json")
        .header("accept", "text/event-stream")
        .json(&request_body)
        .send()
        .map_err(|err| err.to_string())?;

    let status_code = response.status().as_u16() as i64;
    if !response.status().is_success() {
        return Err(format!("codex probe http_status={status_code}"));
    }
    read_first_chunk(response)?;
    Ok(status_code)
}

/// 函数 `probe_codex_endpoint`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - client: 参数 client
/// - base_url: 参数 base_url
/// - secret: 参数 secret
///
/// # 返回
/// 返回函数执行结果
fn probe_codex_endpoint(
    client: &reqwest::blocking::Client,
    api: &AggregateApi,
    secret: &str,
) -> Result<i64, String> {
    let has_model_override = api
        .model_override
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    if has_model_override {
        return probe_codex_responses_endpoint(client, api, secret);
    }

    let models_result = probe_codex_models_endpoint(client, api, secret);
    if let Ok(code) = models_result {
        return Ok(code);
    }
    let models_err = models_result
        .err()
        .unwrap_or_else(|| "codex models probe failed".to_string());
    let responses_result = probe_codex_responses_endpoint(client, api, secret);
    if let Ok(code) = responses_result {
        return Ok(code);
    }

    let responses_err = responses_result
        .err()
        .unwrap_or_else(|| "codex responses probe failed".to_string());
    Err(format!("{models_err}; {responses_err}"))
}

/// 函数 `probe_claude_endpoint`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - client: 参数 client
/// - base_url: 参数 base_url
/// - secret: 参数 secret
///
/// # 返回
/// 返回函数执行结果
fn probe_claude_endpoint(
    client: &reqwest::blocking::Client,
    api: &AggregateApi,
    secret: &str,
) -> Result<i64, String> {
    let probe_path = action_path_or_default(api, "/messages?beta=true");
    let url = normalize_probe_url(api.url.as_str(), probe_path.as_str());
    let (mut models, discovery_error) = if let Some(model_override) = api
        .model_override
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        (vec![model_override.to_string()], None)
    } else {
        claude_probe_models_for_api(client, api, secret)
    };
    if models.is_empty() {
        models = claude_probe_fallback_models_for_api(api)
            .into_iter()
            .map(str::to_string)
            .collect();
    }
    let mut last_error = None;
    for (index, model) in models.iter().enumerate() {
        let builder = client.post(url.as_str());
        let (builder, updated_url) = apply_probe_auth(builder, url.clone(), api, secret)?;
        let builder = if updated_url != url {
            let rebuilt = client.post(updated_url.as_str());
            let (rebuilt, _) = apply_probe_auth(rebuilt, updated_url, api, secret)?;
            rebuilt
        } else {
            builder
        };
        let response = builder
            .header("anthropic-version", "2023-06-01")
            .header(
                "anthropic-beta",
                "claude-code-20250219,interleaved-thinking-2025-05-14",
            )
            .header("content-type", "application/json")
            .header("accept", "application/json")
            .header("accept-encoding", "identity")
            .header("user-agent", "claude-cli/2.1.2 (external, cli)")
            .header("x-app", "cli")
            .json(&build_claude_probe_body(model))
            .send()
            .map_err(|err| err.to_string())?;

        let status_code = response.status().as_u16() as i64;
        if response.status().is_success() {
            read_first_chunk(response)?;
            return Ok(status_code);
        }

        let status_code_u16 = response.status().as_u16();
        last_error = Some(format!(
            "claude probe http_status={status_code} model={model}"
        ));
        if index + 1 >= models.len() || !should_retry_claude_probe_with_next_model(status_code_u16)
        {
            break;
        }
    }
    Err(match (discovery_error, last_error) {
        (Some(discovery), Some(probe)) => format!("{discovery}; {probe}"),
        (None, Some(probe)) => probe,
        (Some(discovery), None) => discovery,
        (None, None) => "claude probe failed".to_string(),
    })
}

fn probe_gemini_endpoint(
    client: &reqwest::blocking::Client,
    api: &AggregateApi,
    secret: &str,
) -> Result<i64, String> {
    let probe_path = action_path_or_default(api, "/v1beta/models/gemini-2.5-flash:generateContent");
    let url = normalize_probe_url(api.url.as_str(), probe_path.as_str());
    let builder = client.post(url.as_str());
    let (builder, updated_url) = apply_probe_auth(builder, url.clone(), api, secret)?;
    let builder = if updated_url != url {
        let rebuilt = client.post(updated_url.as_str());
        let (rebuilt, _) = apply_probe_auth(rebuilt, updated_url, api, secret)?;
        rebuilt
    } else {
        builder
    };
    let response = builder
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .header("accept-encoding", "identity")
        .json(&build_gemini_probe_body())
        .send()
        .map_err(|err| err.to_string())?;

    let status_code = response.status().as_u16() as i64;
    if !response.status().is_success() {
        return Err(format!("gemini probe http_status={status_code}"));
    }
    read_first_chunk(response)?;
    Ok(status_code)
}

/// 函数 `list_aggregate_apis`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn list_aggregate_apis() -> Result<Vec<AggregateApiSummary>, String> {
    let storage = open_storage().ok_or_else(|| "open storage failed".to_string())?;
    let items = storage
        .list_aggregate_apis()
        .map_err(|err| format!("list aggregate apis failed: {err}"))?;
    Ok(items
        .into_iter()
        .map(|item| AggregateApiSummary {
            id: item.id,
            provider_type: item.provider_type,
            supplier_name: item.supplier_name,
            sort: item.sort,
            url: item.url,
            auth_type: item.auth_type,
            auth_params: item
                .auth_params_json
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok()),
            action: item.action,
            model_override: item.model_override,
            cost_multiplier: item.cost_multiplier,
            daily_spend_limit_usd: item.daily_spend_limit_usd,
            status: item.status,
            created_at: item.created_at,
            updated_at: item.updated_at,
            last_test_at: item.last_test_at,
            last_test_status: item.last_test_status,
            last_test_error: item.last_test_error,
        })
        .collect())
}

/// 函数 `create_aggregate_api`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn create_aggregate_api(
    url: Option<String>,
    key: Option<String>,
    provider_type: Option<String>,
    supplier_name: Option<String>,
    sort: Option<i64>,
    auth_type: Option<String>,
    auth_custom_enabled: Option<bool>,
    auth_params: Option<serde_json::Value>,
    action_custom_enabled: Option<bool>,
    action: Option<String>,
    model_override: Option<String>,
    cost_multiplier: Option<f64>,
    daily_spend_limit_usd: Option<f64>,
    username: Option<String>,
    password: Option<String>,
) -> Result<AggregateApiCreateResult, String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let normalized_provider_type = normalize_provider_type(provider_type)?;
    let normalized_supplier_name = normalize_supplier_name(supplier_name)?;
    let normalized_sort = normalize_sort(sort);
    let normalized_url = normalize_upstream_base_url(url)?
        .unwrap_or_else(|| provider_default_url(normalized_provider_type.as_str()).to_string());
    let normalized_auth_type = normalize_auth_type(auth_type)?;
    let normalized_auth_params_json = normalize_auth_params_json(
        normalized_auth_type.as_str(),
        auth_custom_enabled,
        auth_params,
    )?;
    let normalized_action =
        normalize_action_override(action_custom_enabled, action)?.unwrap_or(None);
    let normalized_model_override = normalize_model_override(model_override)?;
    let normalized_cost_multiplier = normalize_cost_multiplier(cost_multiplier)?;
    let normalized_daily_spend_limit_usd = normalize_daily_spend_limit_usd(daily_spend_limit_usd)?;
    let normalized_secret = if normalized_auth_type == AGGREGATE_API_AUTH_APIKEY {
        normalize_secret(key).ok_or_else(|| "key is required".to_string())?
    } else {
        let username = username
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| "username is required".to_string())?;
        let password = password
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| "password is required".to_string())?;
        serialize_userpass_secret(username, password)?
    };
    let id = generate_aggregate_api_id();
    let created_at = now_ts();
    let record = AggregateApi {
        id: id.clone(),
        provider_type: normalized_provider_type,
        supplier_name: Some(normalized_supplier_name),
        sort: normalized_sort,
        url: normalized_url,
        auth_type: normalized_auth_type,
        auth_params_json: normalized_auth_params_json
            .map(|value| if value.is_empty() { None } else { Some(value) })
            .unwrap_or(None),
        action: normalized_action,
        model_override: normalized_model_override,
        cost_multiplier: normalized_cost_multiplier,
        daily_spend_limit_usd: normalized_daily_spend_limit_usd,
        status: "active".to_string(),
        created_at,
        updated_at: created_at,
        last_test_at: None,
        last_test_status: None,
        last_test_error: None,
    };
    storage
        .insert_aggregate_api(&record)
        .map_err(|err| err.to_string())?;
    if let Err(err) = storage.upsert_aggregate_api_secret(&id, &normalized_secret) {
        let _ = storage.delete_aggregate_api(&id);
        return Err(format!("persist aggregate api secret failed: {err}"));
    }
    Ok(AggregateApiCreateResult {
        id,
        key: if record.auth_type == AGGREGATE_API_AUTH_APIKEY {
            normalized_secret
        } else {
            String::new()
        },
    })
}

/// 函数 `update_aggregate_api`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn update_aggregate_api(
    api_id: &str,
    url: Option<String>,
    key: Option<String>,
    provider_type: Option<String>,
    supplier_name: Option<String>,
    sort: Option<i64>,
    status: Option<String>,
    auth_type: Option<String>,
    auth_custom_enabled: Option<bool>,
    auth_params: Option<serde_json::Value>,
    action_custom_enabled: Option<bool>,
    action: Option<String>,
    model_override: Option<String>,
    cost_multiplier: Option<f64>,
    daily_spend_limit_usd: Option<f64>,
    username: Option<String>,
    password: Option<String>,
) -> Result<(), String> {
    if api_id.is_empty() {
        return Err("aggregate api id required".to_string());
    }
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let existing = storage
        .find_aggregate_api_by_id(api_id)
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "aggregate api not found".to_string())?;
    let existing_auth_type = normalize_auth_type(Some(existing.auth_type.clone()))
        .unwrap_or_else(|_| AGGREGATE_API_AUTH_APIKEY.to_string());
    let normalized_auth_type = match auth_type {
        Some(raw) => Some(normalize_auth_type(Some(raw))?),
        None => None,
    };
    let next_auth_type = normalized_auth_type
        .as_deref()
        .unwrap_or(existing_auth_type.as_str())
        .to_string();
    let auth_type_changed = next_auth_type != existing_auth_type;

    if let Some(next) = normalized_auth_type.as_deref() {
        storage
            .update_aggregate_api_auth_type(api_id, next)
            .map_err(|err| err.to_string())?;
    }
    if let Some(provider_type) = provider_type {
        let normalized_provider_type = normalize_provider_type(Some(provider_type))?;
        storage
            .update_aggregate_api_type(api_id, normalized_provider_type.as_str())
            .map_err(|err| err.to_string())?;
    }
    let normalized_supplier_name = normalize_supplier_name(supplier_name)?;
    storage
        .update_aggregate_api_supplier_name(api_id, Some(normalized_supplier_name.as_str()))
        .map_err(|err| err.to_string())?;
    if sort.is_some() {
        storage
            .update_aggregate_api_sort(api_id, normalize_sort(sort))
            .map_err(|err| err.to_string())?;
    }
    if let Some(status) = status {
        let normalized_status = normalize_status(Some(status))?;
        storage
            .update_aggregate_api_status(api_id, normalized_status.as_str())
            .map_err(|err| err.to_string())?;
    }
    if let Some(url) = url {
        let normalized_url =
            normalize_upstream_base_url(Some(url))?.ok_or_else(|| "url is required".to_string())?;
        storage
            .update_aggregate_api(api_id, normalized_url.as_str())
            .map_err(|err| err.to_string())?;
    }

    if let Some(auth_params_json) =
        normalize_auth_params_json(next_auth_type.as_str(), auth_custom_enabled, auth_params)?
    {
        let normalized = auth_params_json.trim().to_string();
        if normalized.is_empty() {
            storage
                .update_aggregate_api_auth_params_json(api_id, None)
                .map_err(|err| err.to_string())?;
        } else {
            storage
                .update_aggregate_api_auth_params_json(api_id, Some(normalized.as_str()))
                .map_err(|err| err.to_string())?;
        }
    }

    if let Some(action_override) = normalize_action_override(action_custom_enabled, action)? {
        if let Some(action) = action_override {
            let normalized = action.trim().to_string();
            storage
                .update_aggregate_api_action(api_id, Some(normalized.as_str()))
                .map_err(|err| err.to_string())?;
        } else {
            storage
                .update_aggregate_api_action(api_id, None)
                .map_err(|err| err.to_string())?;
        }
    }
    if model_override.is_some() {
        let normalized = normalize_model_override(model_override)?;
        storage
            .update_aggregate_api_model_override(api_id, normalized.as_deref())
            .map_err(|err| err.to_string())?;
    }
    if cost_multiplier.is_some() {
        let normalized = normalize_cost_multiplier(cost_multiplier)?;
        storage
            .update_aggregate_api_cost_multiplier(api_id, normalized)
            .map_err(|err| err.to_string())?;
    }
    if daily_spend_limit_usd.is_some() {
        let normalized = normalize_daily_spend_limit_usd(daily_spend_limit_usd)?;
        storage
            .update_aggregate_api_daily_spend_limit_usd(api_id, normalized)
            .map_err(|err| err.to_string())?;
    }

    if next_auth_type == AGGREGATE_API_AUTH_APIKEY {
        let normalized_secret = normalize_secret(key);
        if auth_type_changed && normalized_secret.is_none() {
            return Err("key is required when switching authType to apikey".to_string());
        }
        if let Some(secret) = normalized_secret {
            storage
                .upsert_aggregate_api_secret(api_id, &secret)
                .map_err(|err| err.to_string())?;
        }
    } else {
        let username = username.as_deref().map(str::trim).unwrap_or("");
        let password = password.as_deref().map(str::trim).unwrap_or("");
        let has_user = !username.is_empty();
        let has_pass = !password.is_empty();
        if (has_user && !has_pass) || (!has_user && has_pass) {
            return Err("username and password must be provided together".to_string());
        }
        if auth_type_changed && (!has_user || !has_pass) {
            return Err(
                "username and password are required when switching authType to userpass"
                    .to_string(),
            );
        }
        if has_user && has_pass {
            let secret = serialize_userpass_secret(username, password)?;
            storage
                .upsert_aggregate_api_secret(api_id, &secret)
                .map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

/// 函数 `delete_aggregate_api`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn delete_aggregate_api(api_id: &str) -> Result<(), String> {
    if api_id.is_empty() {
        return Err("aggregate api id required".to_string());
    }
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    storage
        .delete_aggregate_api(api_id)
        .map_err(|err| err.to_string())
}

/// 函数 `read_aggregate_api_secret`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn read_aggregate_api_secret(api_id: &str) -> Result<AggregateApiSecretResult, String> {
    if api_id.is_empty() {
        return Err("aggregate api id required".to_string());
    }
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let api = storage
        .find_aggregate_api_by_id(api_id)
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "aggregate api not found".to_string())?;
    let key = storage
        .find_aggregate_api_secret_by_id(api_id)
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "aggregate api secret not found".to_string())?;
    let auth_type = normalize_auth_type(Some(api.auth_type))?;
    if auth_type == AGGREGATE_API_AUTH_USERPASS {
        let parsed: UserPassSecret = serde_json::from_str(key.as_str())
            .map_err(|_| "invalid aggregate api secret".to_string())?;
        return Ok(AggregateApiSecretResult {
            id: api_id.to_string(),
            key: String::new(),
            auth_type,
            username: Some(parsed.username),
            password: Some(parsed.password),
        });
    }
    Ok(AggregateApiSecretResult {
        id: api_id.to_string(),
        key,
        auth_type,
        username: None,
        password: None,
    })
}

/// 函数 `test_aggregate_api_connection`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn test_aggregate_api_connection(
    api_id: &str,
) -> Result<AggregateApiTestResult, String> {
    if api_id.is_empty() {
        return Err("aggregate api id required".to_string());
    }
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let api = storage
        .find_aggregate_api_by_id(api_id)
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "aggregate api not found".to_string())?;
    let secret = storage
        .find_aggregate_api_secret_by_id(api_id)
        .map_err(|err| err.to_string())?;
    let Some(secret) = secret else {
        return Err("aggregate api secret not found".to_string());
    };
    let client = gateway::fresh_upstream_client();
    let started_at = Instant::now();
    let provider_type = normalize_provider_type_value(api.provider_type.as_str());
    let result = match provider_type.as_str() {
        AGGREGATE_API_PROVIDER_CLAUDE => probe_claude_endpoint(&client, &api, &secret),
        AGGREGATE_API_PROVIDER_GEMINI => probe_gemini_endpoint(&client, &api, &secret),
        _ if probe_codex_only_for_provider(provider_type.as_str()) => {
            probe_codex_endpoint(&client, &api, &secret)
        }
        _ => probe_codex_endpoint(&client, &api, &secret),
    };
    let (ok, status_code, last_error) = match result {
        Ok(code) => (true, Some(code), None),
        Err(err) => (false, None, Some(err)),
    };
    let message = last_error.map(|err| format!("provider={provider_type}; {err}"));

    let _ = storage.update_aggregate_api_test_result(api_id, ok, status_code, message.as_deref());
    Ok(AggregateApiTestResult {
        id: api_id.to_string(),
        ok,
        status_code,
        message,
        tested_at: now_ts(),
        latency_ms: started_at.elapsed().as_millis() as i64,
    })
}

pub(crate) fn query_aggregate_api_balance(
    api_id: &str,
) -> Result<AggregateApiBalanceResult, String> {
    if api_id.is_empty() {
        return Err("aggregate api id required".to_string());
    }
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let api = storage
        .find_aggregate_api_by_id(api_id)
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "aggregate api not found".to_string())?;
    let secret = storage
        .find_aggregate_api_secret_by_id(api_id)
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "aggregate api secret not found".to_string())?;

    let client = gateway::fresh_upstream_client();
    let started_at = Instant::now();
    let provider = normalize_provider_type_value(api.provider_type.as_str());
    let mut last_error = None;

    for path in balance_query_paths(&api) {
        match query_balance_path(&client, &api, &secret, path) {
            Ok((_status, snapshot)) => {
                return Ok(AggregateApiBalanceResult {
                    id: api_id.to_string(),
                    ok: snapshot.message.is_none(),
                    provider,
                    remaining: snapshot.remaining,
                    used: snapshot.used,
                    total: snapshot.total,
                    unit: snapshot.unit,
                    plan_name: snapshot.plan_name,
                    message: snapshot.message,
                    queried_at: now_ts(),
                    latency_ms: started_at.elapsed().as_millis() as i64,
                });
            }
            Err(err) => {
                last_error = Some(err);
            }
        }
    }

    Ok(AggregateApiBalanceResult {
        id: api_id.to_string(),
        ok: false,
        provider,
        remaining: None,
        used: None,
        total: None,
        unit: None,
        plan_name: None,
        message: last_error.or_else(|| Some("balance query failed".to_string())),
        queried_at: now_ts(),
        latency_ms: started_at.elapsed().as_millis() as i64,
    })
}
