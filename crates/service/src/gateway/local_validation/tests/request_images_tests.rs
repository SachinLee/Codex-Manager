use super::{
    adapt_openai_images_edits_body_to_responses, adapt_openai_images_generations_body_to_responses,
    ensure_codex_image_tool_model_not_used_for_text_request,
};
use crate::gateway::ResponseAdapter;
use serde_json::json;

#[test]
fn images_generation_request_builds_responses_image_generation_tool() {
    let body = json!({
        "model": "gpt-image-2",
        "prompt": "画一张极简风格的猫",
        "size": "1024x1024",
        "quality": "high",
        "background": "transparent",
        "output_format": "png",
        "response_format": "url",
        "stream": true,
        "partial_images": 1
    });

    let (mapped, adapter) =
        adapt_openai_images_generations_body_to_responses(serde_json::to_vec(&body).expect("body"))
            .expect("adapt images request");
    let value: serde_json::Value = serde_json::from_slice(&mapped).expect("parse mapped body");

    assert_eq!(adapter, ResponseAdapter::ImagesUrlFromResponses);
    assert_eq!(value["model"], "gpt-5.4-mini");
    assert_eq!(value["stream"], true);
    assert_eq!(value["store"], false);
    assert_eq!(value["tool_choice"]["type"], "image_generation");
    assert_eq!(value["tools"][0]["type"], "image_generation");
    assert_eq!(value["tools"][0]["model"], "gpt-image-2");
    assert_eq!(value["tools"][0]["size"], "1024x1024");
    assert_eq!(value["tools"][0]["quality"], "high");
    assert_eq!(value["tools"][0]["background"], "transparent");
    assert_eq!(value["tools"][0]["partial_images"], 1);
    assert_eq!(
        value["input"][0]["content"][0]["text"],
        "画一张极简风格的猫"
    );
}

#[test]
fn images_generation_request_defaults_b64_json_and_tool_model() {
    let body = json!({ "prompt": "cat" });

    let (mapped, adapter) =
        adapt_openai_images_generations_body_to_responses(serde_json::to_vec(&body).expect("body"))
            .expect("adapt images request");
    let value: serde_json::Value = serde_json::from_slice(&mapped).expect("parse mapped body");

    assert_eq!(adapter, ResponseAdapter::ImagesB64JsonFromResponses);
    assert_eq!(value["tools"][0]["model"], "gpt-image-2");
    assert_eq!(value["tools"][0]["output_format"], "png");
}

#[test]
fn images_generation_request_requires_prompt() {
    let body = json!({ "model": "gpt-image-2" });

    let err =
        adapt_openai_images_generations_body_to_responses(serde_json::to_vec(&body).expect("body"))
            .expect_err("prompt should be required");

    assert!(err.contains("prompt is required"));
}

#[test]
fn images_edits_json_request_builds_responses_with_input_images_and_mask() {
    let body = json!({
        "model": "gpt-image-2",
        "prompt": "把背景改成透明",
        "images": [{
            "image_url": "data:image/png;base64,aW1hZ2U="
        }],
        "mask": {
            "image_url": "data:image/png;base64,bWFzaw=="
        },
        "response_format": "b64_json"
    });

    let (mapped, adapter) = adapt_openai_images_edits_body_to_responses(
        serde_json::to_vec(&body).expect("body"),
        Some("application/json"),
    )
    .expect("adapt edits json request");
    let value: serde_json::Value = serde_json::from_slice(&mapped).expect("parse mapped body");

    assert_eq!(adapter, ResponseAdapter::ImagesB64JsonFromResponses);
    assert_eq!(value["tools"][0]["type"], "image_generation");
    assert_eq!(value["tools"][0]["model"], "gpt-image-2");
    assert_eq!(
        value["tools"][0]["input_image_mask"]["image_url"],
        "data:image/png;base64,bWFzaw=="
    );
    assert_eq!(value["input"][0]["content"][0]["text"], "把背景改成透明");
    assert_eq!(
        value["input"][0]["content"][1]["image_url"],
        "data:image/png;base64,aW1hZ2U="
    );
}

#[test]
fn images_edits_json_rejects_file_id() {
    let body = json!({
        "prompt": "edit",
        "images": [{ "file_id": "file_123" }]
    });

    let err = adapt_openai_images_edits_body_to_responses(
        serde_json::to_vec(&body).expect("body"),
        Some("application/json"),
    )
    .expect_err("file_id should be rejected");

    assert!(err.contains("file_id is not supported"));
}

#[test]
fn images_edits_json_rejects_invalid_base64_data_url() {
    let body = json!({
        "prompt": "edit",
        "images": [{ "image_url": "data:image/png;base64,***" }]
    });

    let err = adapt_openai_images_edits_body_to_responses(
        serde_json::to_vec(&body).expect("body"),
        Some("application/json"),
    )
    .expect_err("invalid base64 should be rejected");

    assert!(err.contains("invalid base64 image data"));
}

#[test]
fn images_edits_multipart_request_builds_data_urls() {
    let body = concat!(
        "--test-boundary\r\n",
        "Content-Disposition: form-data; name=\"prompt\"\r\n\r\n",
        "修图\r\n",
        "--test-boundary\r\n",
        "Content-Disposition: form-data; name=\"image\"; filename=\"a.png\"\r\n",
        "Content-Type: image/png\r\n\r\n",
        "IMG\r\n",
        "--test-boundary\r\n",
        "Content-Disposition: form-data; name=\"mask\"; filename=\"m.png\"\r\n",
        "Content-Type: image/png\r\n\r\n",
        "MSK\r\n",
        "--test-boundary--\r\n"
    )
    .as_bytes()
    .to_vec();

    let (mapped, adapter) = adapt_openai_images_edits_body_to_responses(
        body,
        Some("multipart/form-data; boundary=test-boundary"),
    )
    .expect("adapt edits multipart request");
    let value: serde_json::Value = serde_json::from_slice(&mapped).expect("parse mapped body");

    assert_eq!(adapter, ResponseAdapter::ImagesB64JsonFromResponses);
    assert_eq!(value["input"][0]["content"][0]["text"], "修图");
    assert_eq!(
        value["input"][0]["content"][1]["image_url"],
        "data:image/png;base64,SU1H"
    );
    assert_eq!(
        value["tools"][0]["input_image_mask"]["image_url"],
        "data:image/png;base64,TVNL"
    );
}

#[test]
fn rejects_gpt_image_model_on_text_generation_paths() {
    let err = ensure_codex_image_tool_model_not_used_for_text_request(
        "/v1/chat/completions",
        Some("gpt-image-2"),
    )
    .expect_err("text path should reject image tool model");

    assert_eq!(err.status_code, 400);
    assert!(err.message.contains("/v1/images/generations"));
}

#[test]
fn allows_gpt_image_model_on_images_paths() {
    assert!(ensure_codex_image_tool_model_not_used_for_text_request(
        "/v1/images/generations",
        Some("gpt-image-2"),
    )
    .is_ok());
    assert!(ensure_codex_image_tool_model_not_used_for_text_request(
        "/v1/images/edits",
        Some("gpt-image-2"),
    )
    .is_ok());
}
