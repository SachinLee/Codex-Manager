use super::{
    append_client_version_query, build_model_picker_client, build_models_request_headers,
    build_models_request_url, model_picker_client, model_picker_client_build_count_for_test,
    refresh_model_picker_client, reset_model_picker_client_build_count_for_test,
    summarize_models_error_response,
};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::StatusCode;

/// 函数 `append_client_version_query_adds_missing_param`
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
fn append_client_version_query_adds_missing_param() {
    let _guard = crate::test_env_guard();
    crate::gateway::set_codex_user_agent_version("0.101.0")
        .expect("set default codex user agent version");
    let actual = append_client_version_query("https://example.com/backend-api/codex/models");
    assert_eq!(
        actual,
        "https://example.com/backend-api/codex/models?client_version=0.101.0"
    );
}

/// 函数 `append_client_version_query_preserves_existing_query`
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
fn append_client_version_query_preserves_existing_query() {
    let _guard = crate::test_env_guard();
    crate::gateway::set_codex_user_agent_version("0.101.0")
        .expect("set default codex user agent version");
    let actual =
        append_client_version_query("https://example.com/backend-api/codex/models?limit=20");
    assert_eq!(
        actual,
        "https://example.com/backend-api/codex/models?limit=20&client_version=0.101.0"
    );
}

/// 函数 `append_client_version_query_does_not_duplicate_param`
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
fn append_client_version_query_does_not_duplicate_param() {
    let _guard = crate::test_env_guard();
    crate::gateway::set_codex_user_agent_version("0.101.0")
        .expect("set default codex user agent version");
    let actual = append_client_version_query(
        "https://example.com/backend-api/codex/models?client_version=0.101.0",
    );
    assert_eq!(
        actual,
        "https://example.com/backend-api/codex/models?client_version=0.101.0"
    );
}

/// 函数 `build_models_request_url_appends_client_version_for_codex_backend`
///
/// 作者: gaohongshun
///
/// 时间: 2026-05-03
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn build_models_request_url_appends_client_version_for_codex_backend() {
    let _guard = crate::test_env_guard();
    crate::gateway::set_codex_user_agent_version("0.101.0")
        .expect("set default codex user agent version");
    let actual = build_models_request_url("https://example.com/backend-api/codex", "/v1/models");
    assert_eq!(
        actual,
        "https://example.com/backend-api/codex/models?client_version=0.101.0"
    );
}

/// 函数 `build_models_request_url_preserves_existing_query_with_client_version`
///
/// 作者: gaohongshun
///
/// 时间: 2026-05-03
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn build_models_request_url_preserves_existing_query_with_client_version() {
    let _guard = crate::test_env_guard();
    crate::gateway::set_codex_user_agent_version("0.101.0")
        .expect("set default codex user agent version");
    let actual = build_models_request_url(
        "https://example.com/backend-api/codex",
        "/v1/models?limit=20",
    );
    assert_eq!(
        actual,
        "https://example.com/backend-api/codex/models?limit=20&client_version=0.101.0"
    );
}

/// 函数 `build_models_request_headers_match_codex_profile`
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
fn build_models_request_headers_match_codex_profile() {
    let headers = build_models_request_headers(
        "access-token",
        "codex_cli_rs/1.2.3 (Windows 11; x86_64) terminal",
        "codex_cli_rs",
        Some("us"),
        true,
        Some("acc_123"),
    );
    let find = |name: &str| {
        headers
            .iter()
            .find(|(header, _)| header == name)
            .map(|(_, value)| value.as_str())
    };

    assert_eq!(find("Accept"), Some("application/json"));
    assert_eq!(
        find("User-Agent"),
        Some("codex_cli_rs/1.2.3 (Windows 11; x86_64) terminal")
    );
    assert_eq!(find("originator"), Some("codex_cli_rs"));
    assert_eq!(find("Authorization"), Some("Bearer access-token"));
    assert!(find("Cookie").is_none());
    assert_eq!(find("ChatGPT-Account-ID"), Some("acc_123"));
    assert_eq!(
        find(crate::gateway::runtime_config::RESIDENCY_HEADER_NAME),
        Some("us")
    );
    assert!(find("Version").is_none());
    assert!(find("ChatGPT-Account-Id").is_none());
}

/// 函数 `build_models_request_headers_omits_optional_headers_when_not_applicable`
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
fn build_models_request_headers_omits_optional_headers_when_not_applicable() {
    let headers = build_models_request_headers(
        "access-token",
        "codex_cli_rs/1.2.3",
        "codex_cli_rs",
        None,
        false,
        Some("acc_123"),
    );
    let find = |name: &str| {
        headers
            .iter()
            .find(|(header, _)| header == name)
            .map(|(_, value)| value.as_str())
    };

    assert!(find("Cookie").is_none());
    assert!(find("ChatGPT-Account-ID").is_none());
    assert!(find(crate::gateway::runtime_config::RESIDENCY_HEADER_NAME).is_none());
}

#[test]
fn model_picker_client_reuses_cached_client_until_config_changes() {
    let _guard = crate::test_env_guard();
    crate::gateway::set_upstream_proxy_url(None).expect("clear proxy");
    crate::gateway::set_codex_user_agent_version("0.201.0").expect("set codex user agent version");
    reset_model_picker_client_build_count_for_test();

    let first = model_picker_client();
    let after_first = model_picker_client_build_count_for_test();
    let second = model_picker_client();

    assert_eq!(model_picker_client_build_count_for_test(), after_first);
    drop(first);
    drop(second);

    crate::gateway::set_codex_user_agent_version("0.201.1")
        .expect("change codex user agent version");
    let refreshed = model_picker_client();
    assert_eq!(model_picker_client_build_count_for_test(), after_first + 1);
    drop(refreshed);

    let fresh = build_model_picker_client();
    assert_eq!(model_picker_client_build_count_for_test(), after_first + 2);
    drop(fresh);

    let refreshed_retry = refresh_model_picker_client();
    let after_refresh = model_picker_client_build_count_for_test();
    assert_eq!(after_refresh, after_first + 3);
    let reused_after_refresh = model_picker_client();
    assert_eq!(model_picker_client_build_count_for_test(), after_refresh);
    drop(refreshed_retry);
    drop(reused_after_refresh);
}

/// 函数 `summarize_models_error_response_uses_stable_challenge_hint_and_debug_headers`
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
fn summarize_models_error_response_uses_stable_challenge_hint_and_debug_headers() {
    let mut headers = HeaderMap::new();
    headers.insert("x-oai-request-id", HeaderValue::from_static("req-models"));
    headers.insert("cf-ray", HeaderValue::from_static("ray-models"));
    headers.insert(
        "x-openai-authorization-error",
        HeaderValue::from_static("missing_authorization_header"),
    );
    headers.insert(
        "x-error-json",
        HeaderValue::from_static("{\"identity_error_code\":\"org_membership_required\"}"),
    );

    let message = summarize_models_error_response(
        StatusCode::FORBIDDEN,
        &headers,
        "<html><title>Just a moment...</title></html>",
        false,
    );

    assert!(message.contains("Cloudflare 安全验证页（title=Just a moment...）"));
    assert!(message.contains("request id: req-models"));
    assert!(message.contains("cf-ray: ray-models"));
    assert!(message.contains("auth error: missing_authorization_header"));
    assert!(message.contains("identity_error_code: org_membership_required"));
    assert!(!message.contains("<html>"));
}

/// 函数 `summarize_models_error_response_includes_identity_error_code`
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
fn summarize_models_error_response_includes_identity_error_code() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-error-json",
        HeaderValue::from_static("{\"identity_error_code\":\"access_denied\"}"),
    );

    let message = summarize_models_error_response(
        StatusCode::FORBIDDEN,
        &headers,
        "{\"error\":{\"message\":\"blocked\"}}",
        false,
    );

    assert!(message.contains("identity_error_code: access_denied"));
}
