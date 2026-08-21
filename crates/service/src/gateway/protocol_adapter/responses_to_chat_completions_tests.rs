//! Phase 2 tests: typed shared Responses->Chat Completions converter.
//!
//! Covers the design mapping table (messages/tools/tool calls/results,
//! parallel_tool_calls, limits, reasoning, format, stream usage), typed
//! incompatibility, model overrides, long names, and passthrough provenance.

use super::*;
use serde_json::json;

fn convert(body: &Value) -> Result<Value, ChatConversionFailure> {
    let bytes = serde_json::to_vec(body).expect("serialize fixture");
    convert_responses_request_to_chat_completions(&bytes, None, None).map(|out| {
        serde_json::from_slice(&out).expect("converted output must re-parse as JSON")
    })
}

fn convert_with_model(body: &Value, model: &str) -> Result<Value, ChatConversionFailure> {
    let bytes = serde_json::to_vec(body).expect("serialize fixture");
    convert_responses_request_to_chat_completions(&bytes, Some(model), None).map(|out| {
        serde_json::from_slice(&out).expect("converted output must re-parse as JSON")
    })
}

/// 带历史 assistant 消息（`previous_messages`）与 `previous_response_id` 的转换。
fn convert_with_previous(body: &Value, history: &[Value]) -> Result<Value, ChatConversionFailure> {
    let bytes = serde_json::to_vec(body).expect("serialize fixture");
    convert_responses_request_to_chat_completions(&bytes, None, Some(history)).map(|out| {
        serde_json::from_slice(&out).expect("converted output must re-parse as JSON")
    })
}

#[test]
fn maps_instructions_and_input_preserving_order() {
    let body = json!({
        "model": "gpt-4.1",
        "instructions": "be concise",
        "input": [
            { "type": "message", "role": "user", "content": [{ "type": "input_text", "text": "hi" }] },
            { "type": "message", "role": "assistant", "content": "hello there" },
            { "type": "message", "role": "developer", "content": "dev rule" }
        ]
    });
    let out = convert(&body).expect("convert");
    let messages = out["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(messages[0]["content"], "be concise");
    assert_eq!(messages[1]["role"], "user");
    assert_eq!(messages[1]["content"], "hi");
    assert_eq!(messages[2]["role"], "assistant");
    assert_eq!(messages[2]["content"], "hello there");
    // developer collapses to system, order preserved
    assert_eq!(messages[3]["role"], "system");
    assert_eq!(messages[3]["content"], "dev rule");
}

#[test]
fn maps_plain_string_input_to_user_message() {
    let out = convert(&json!({ "input": "direct text" })).expect("convert");
    let messages = out["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[0]["content"], "direct text");
}

#[test]
fn maps_function_call_and_output_items() {
    let body = json!({
        "input": [
            {
                "type": "function_call",
                "call_id": "call_1",
                "name": "get_weather",
                "arguments": "{\"city\":\"beijing\"}"
            },
            {
                "type": "function_call_output",
                "call_id": "call_1",
                "output": "{\"temp\":32}"
            }
        ]
    });
    let out = convert(&body).expect("convert");
    let messages = out["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["role"], "assistant");
    assert_eq!(messages[0]["tool_calls"][0]["id"], "call_1");
    assert_eq!(
        messages[0]["tool_calls"][0]["function"]["name"],
        "get_weather"
    );
    assert_eq!(
        messages[0]["tool_calls"][0]["function"]["arguments"],
        "{\"city\":\"beijing\"}"
    );
    assert_eq!(messages[1]["role"], "tool");
    assert_eq!(messages[1]["tool_call_id"], "call_1");
    assert_eq!(messages[1]["content"], "{\"temp\":32}");
}

#[test]
fn maps_function_tools_to_chat_tools() {
    let body = json!({
        "tools": [
            {
                "type": "function",
                "name": "get_weather",
                "description": "weather lookup",
                "parameters": { "type": "object", "properties": { "city": { "type": "string" } } },
                "strict": true
            }
        ]
    });
    let out = convert(&body).expect("convert");
    let tools = out["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["type"], "function");
    let function = &tools[0]["function"];
    assert_eq!(function["name"], "get_weather");
    assert_eq!(function["description"], "weather lookup");
    assert_eq!(function["parameters"]["type"], "object");
    assert_eq!(function["strict"], true);
}

#[test]
fn rejects_non_function_tools() {
    let err = convert(&json!({ "tools": [{ "type": "web_search_preview" }] }))
        .expect_err("must reject hosted tools");
    assert!(matches!(
        err,
        ChatConversionFailure::Incompatible(reason) if reason.contains("tool type")
    ));
}

#[test]
fn rejects_tool_without_type() {
    let err = convert(&json!({ "tools": [{ "name": "x" }] }))
        .expect_err("must reject typeless tool");
    assert!(matches!(
        err,
        ChatConversionFailure::Incompatible(reason) if reason.contains("without type")
    ));
}

#[test]
fn passes_through_parallel_tool_calls_and_shared_keys() {
    let out = convert(&json!({
        "parallel_tool_calls": false,
        "temperature": 0.2,
        "top_p": 0.9,
        "user": "u-1",
        "metadata": { "k": "v" },
        "seed": 42,
        "n": 1
    }))
    .expect("convert");
    assert_eq!(out["parallel_tool_calls"], false);
    assert_eq!(out["temperature"], 0.2);
    assert_eq!(out["top_p"], 0.9);
    assert_eq!(out["user"], "u-1");
    assert_eq!(out["metadata"]["k"], "v");
    assert_eq!(out["seed"], 42);
    assert_eq!(out["n"], 1);
}

#[test]
fn maps_max_output_tokens_and_reasoning_effort() {
    let out = convert(&json!({
        "max_output_tokens": 4096,
        "reasoning": { "effort": "high" }
    }))
    .expect("convert");
    assert_eq!(out["max_completion_tokens"], 4096);
    assert_eq!(out["reasoning_effort"], "high");
    assert!(out.get("max_output_tokens").is_none());
}

#[test]
fn maps_json_object_format() {
    let out = convert(&json!({
        "text": { "format": { "type": "json_object" } }
    }))
    .expect("convert");
    assert_eq!(out["response_format"]["type"], "json_object");
}

#[test]
fn maps_json_schema_format_with_schema_and_strict() {
    let out = convert(&json!({
        "text": {
            "format": {
                "type": "json_schema",
                "name": "step",
                "schema": { "type": "object" },
                "strict": true
            }
        }
    }))
    .expect("convert");
    assert_eq!(out["response_format"]["type"], "json_schema");
    assert_eq!(out["response_format"]["json_schema"]["name"], "step");
    assert_eq!(
        out["response_format"]["json_schema"]["schema"]["type"],
        "object"
    );
    assert_eq!(out["response_format"]["json_schema"]["strict"], true);
}

#[test]
fn rejects_unsupported_response_format() {
    let err = convert(&json!({
        "text": { "format": { "type": "garbled" } }
    }))
    .expect_err("must reject unknown format");
    assert!(matches!(
        err,
        ChatConversionFailure::Incompatible(reason) if reason.contains("response format")
    ));
}

#[test]
fn injects_stream_usage_when_streaming() {
    let out = convert(&json!({ "stream": true })).expect("convert");
    assert_eq!(out["stream"], true);
    assert_eq!(out["stream_options"]["include_usage"], true);
}

#[test]
fn preserves_explicit_stream_options() {
    let out = convert(&json!({
        "stream": true,
        "stream_options": { "include_usage": false }
    }))
    .expect("convert");
    assert_eq!(out["stream_options"]["include_usage"], true);
}

#[test]
fn no_stream_options_when_not_streaming() {
    let out = convert(&json!({ "stream": false })).expect("convert");
    assert!(out.get("stream_options").is_none());
}

#[test]
fn applies_model_override() {
    let out = convert_with_model(&json!({ "model": "original" }), "  fallback-model  ")
        .expect("convert");
    assert_eq!(out["model"], "fallback-model");
}

#[test]
fn rejects_audio_modality() {
    let err = convert(&json!({ "audio": { "input": "x" } }))
        .expect_err("must reject audio");
    assert!(matches!(
        err,
        ChatConversionFailure::Incompatible(reason) if reason.contains("audio")
    ));
}

#[test]
fn drops_store_without_rejecting_request() {
    let out = convert(&json!({
        "store": true,
        "input": "hello"
    }))
    .expect("store is a Responses-only preference");
    assert!(out.get("store").is_none());
    assert_eq!(out["messages"][0]["content"], "hello");
}

#[test]
fn merges_cached_assistant_context_for_previous_response() {
    let history = vec![json!({
        "role": "assistant",
        "content": "I will call the tool.",
        "tool_calls": [{
            "id": "call_weather",
            "type": "function",
            "function": { "name": "weather", "arguments": "{\"city\":\"Beijing\"}" }
        }]
    })];
    let out = convert_with_previous(
        &json!({
            "previous_response_id": "chatcmpl_prior",
            "input": [{
                "type": "function_call_output",
                "call_id": "call_weather",
                "output": "sunny"
            }]
        }),
        history.as_slice(),
    )
    .expect("cached history reconstructs Chat messages");
    let messages = out["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["role"], "assistant");
    assert_eq!(messages[0]["tool_calls"][0]["id"], "call_weather");
    assert_eq!(messages[1]["role"], "tool");
    assert_eq!(messages[1]["tool_call_id"], "call_weather");
    assert!(out.get("previous_response_id").is_none());
    assert_eq!(out["tools"][0]["function"]["name"], "weather");
}

#[test]
fn rejects_previous_response_without_context_or_self_contained_input() {
    let err = convert(&json!({
        "previous_response_id": "chatcmpl_missing",
        "input": "follow up"
    }))
    .expect_err("plain incremental input cannot safely lose history");
    assert!(matches!(
        err,
        ChatConversionFailure::Incompatible(reason)
            if reason.contains("context unavailable") && reason.contains("input_items=0")
    ));
}

#[test]
fn self_contained_history_can_continue_without_cache() {
    let out = convert(&json!({
        "previous_response_id": "chatcmpl_missing",
        "input": [
            { "type": "message", "role": "assistant", "content": "earlier answer" },
            { "type": "message", "role": "user", "content": "follow up" }
        ]
    }))
    .expect("full input can replace previous response linkage");
    assert!(out.get("previous_response_id").is_none());
    assert_eq!(out["messages"].as_array().expect("messages").len(), 2);
}

#[test]
fn rejects_unknown_tool_choice_string() {
    let err = convert(&json!({ "tool_choice": "always" }))
        .expect_err("must reject unknown tool_choice");
    assert!(matches!(
        err,
        ChatConversionFailure::Incompatible(reason) if reason.contains("tool_choice")
    ));
}

#[test]
fn passes_through_representable_tool_choices() {
    for kind in ["auto", "none", "required"] {
        let out = convert(&json!({ "tool_choice": kind })).expect("convert");
        assert_eq!(out["tool_choice"], kind);
    }
    let out = convert(&json!({ "tool_choice": { "type": "function", "name": "f1" } }))
        .expect("convert");
    assert_eq!(out["tool_choice"]["type"], "function");
    assert_eq!(out["tool_choice"]["function"]["name"], "f1");
}

#[test]
fn preserves_long_tool_names_untruncated() {
    let long_name = format!("very_long_function_name_{}", "x".repeat(200));
    let body = json!({
        "tools": [{ "type": "function", "name": long_name, "parameters": {} }]
    });
    let out = convert(&body).expect("convert");
    let tools = out["tools"].as_array().expect("tools array");
    assert_eq!(tools[0]["function"]["name"].as_str().unwrap().len(), 224);
}

#[test]
fn maps_image_input_to_multimodal_content() {
    let out = convert(&json!({
        "input": [{
            "type": "message",
            "role": "user",
            "content": [
                { "type": "input_text", "text": "what is this" },
                { "type": "input_image", "image_url": "https://example.com/a.png" }
            ]
        }]
    }))
    .expect("convert");
    let messages = out["messages"].as_array().expect("messages array");
    let content = messages[0]["content"].as_array().expect("content array");
    assert_eq!(content.len(), 2);
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[1]["type"], "image_url");
    assert_eq!(content[1]["image_url"]["url"], "https://example.com/a.png");
}

#[test]
fn invalid_json_is_typed_invalid() {
    let err = convert_responses_request_to_chat_completions(b"not json", None, None);
    assert!(matches!(err, Err(ChatConversionFailure::Invalid(_))));
}

#[test]
fn non_object_body_is_typed_invalid() {
    let err = convert(&json!([1, 2, 3]));
    assert!(matches!(err, Err(ChatConversionFailure::Invalid(_))));
}

#[test]
fn omits_empty_instructions() {
    let out = convert(&json!({ "instructions": "   ", "input": "hello" })).expect("convert");
    let messages = out["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");
}

#[test]
fn empty_body_converts_to_empty_chat_object() {
    let out = convert(&json!({})).expect("convert");
    assert!(out.get("messages").is_none());
    assert!(out.get("tools").is_none());
}
