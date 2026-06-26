use codexmanager_core::rpc::types::ModelsResponse;
use serde_json::json;

use super::parse_models_response;

/// 函数 `parse_models_response_preserves_official_models_payload`
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
fn parse_models_response_preserves_official_models_payload() {
    let body = json!({
        "models": [
            {
                "slug": "gpt-5.4-mini",
                "display_name": "GPT-5.4 Mini",
                "supported_in_api": true,
                "visibility": "list"
            }
        ]
    })
    .to_string();

    let response = parse_models_response(body.as_bytes());
    assert_eq!(response.models.len(), 1);
    assert_eq!(response.models[0].slug, "gpt-5.4-mini");
    assert_eq!(response.models[0].display_name, "GPT-5.4 Mini");
    assert!(response.models[0].supported_in_api);
    assert_eq!(response.models[0].visibility.as_deref(), Some("list"));
}

#[test]
fn parse_models_response_falls_back_to_legacy_data_array() {
    let body = json!({
        "data": [
            { "id": "gpt-5.4-mini", "display_name": "GPT-5.4 Mini" },
            { "slug": "o3", "title": "OpenAI o3" }
        ]
    })
    .to_string();

    let response: ModelsResponse = parse_models_response(body.as_bytes());
    assert_eq!(response.models.len(), 2);
    assert_eq!(response.models[0].slug, "gpt-5.4-mini");
    assert_eq!(response.models[0].display_name, "GPT-5.4 Mini");
    assert!(response.models[0].supported_in_api);
    assert_eq!(
        response.models[0].input_modalities,
        vec!["text".to_string(), "image".to_string()]
    );
    assert_eq!(response.models[1].slug, "o3");
    assert_eq!(response.models[1].display_name, "o3");
}

#[test]
fn parse_models_response_falls_back_to_items_array() {
    let body = json!({
        "items": [
            { "slug": "gpt-5.4", "display_name": "GPT-5.4" }
        ]
    })
    .to_string();

    let response: ModelsResponse = parse_models_response(body.as_bytes());
    assert_eq!(response.models.len(), 1);
    assert_eq!(response.models[0].slug, "gpt-5.4");
    assert_eq!(response.models[0].display_name, "GPT-5.4");
    assert!(response.models[0].supported_in_api);
}
