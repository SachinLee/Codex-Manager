//! Typed pure converter: canonical Responses request -> OpenAI Chat Completions request.
//!
//! This is the single shared request-direction converter used by the Aggregate
//! Chat-upstream path. The client Chat/Compact request path reuses the mapping
//! helpers here instead of keeping a second conversion state machine.
//!
//! Conversion is strict: unsupported semantics are typed `Incompatible`
//! failures returned to candidate handling, never silent drops.

use serde_json::{json, Map, Value};

/// Typed conversion failure surfaced to Aggregate candidate handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChatConversionFailure {
    /// 上游 Chat Completions 无法表示该语义（稳定的有界原因，不透传原始输入）。
    Incompatible(String),
    /// 本地解析/序列化错误。
    Invalid(String),
}

/// Fields both protocols share and therefore pass through untouched.
const CHAT_PASSTHROUGH_KEYS: &[&str] = &[
    "model",
    "temperature",
    "top_p",
    "stop",
    "presence_penalty",
    "frequency_penalty",
    "logit_bias",
    "user",
    "metadata",
    "seed",
    "n",
    "logprobs",
    "top_logprobs",
    "service_tier",
    "stream",
    "stream_options",
    "parallel_tool_calls",
    "max_completion_tokens",
];

/// Convert a canonical Responses request body into a Chat Completions request.
///
/// - `instructions`/`input` -> `messages` (system/developer/user/assistant/tool,
///   order preserved)
/// - Responses function tools -> Chat `tools` (standard shape)
/// - `function_call` input items -> assistant `tool_calls`
/// - `function_call_output` items -> `tool` messages
/// - `max_output_tokens` -> `max_completion_tokens`
/// - `reasoning.effort` -> `reasoning_effort`
/// - representable `text.format` -> `response_format`
/// - `stream` -> `stream` + `stream_options.include_usage`
///
/// 续聊语义：
/// - `store` 是 Responses 服务端存储偏好，Chat Completions 无等价物，直接丢弃。
/// - `previous_response_id` 无 Chat 等价字段：若 `previous_messages` 携带该响应
///   的历史 assistant 上下文（由网关上下文缓存提供），合并到 `messages` 头部并
///   剥离该字段；若请求 `input` 本身自包含完整上下文（含 assistant 消息或
///   function_call 项），剥离后继续；两者皆无时返回 `Incompatible`（无法重建
///   对话历史，拒绝上游请求比静默丢上下文安全）。
pub(crate) fn convert_responses_request_to_chat_completions(
    body: &[u8],
    model_override: Option<&str>,
    previous_messages: Option<&[Value]>,
) -> Result<Vec<u8>, ChatConversionFailure> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| ChatConversionFailure::Invalid("invalid responses request json".to_string()))?;
    let obj = value
        .as_object()
        .ok_or_else(|| ChatConversionFailure::Invalid("responses request must be an object".to_string()))?;

    reject_unsupported_semantics(obj)?;

    let mut chat = Map::new();
    for key in CHAT_PASSTHROUGH_KEYS {
        if let Some(field) = obj.get(*key) {
            chat.insert((*key).to_string(), field.clone());
        }
    }

    let mut messages = Vec::<Value>::new();
    if let Some(instructions) = obj
        .get("instructions")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        messages.push(json!({ "role": "system", "content": instructions }));
    }
    if let Some(input) = obj.get("input") {
        match input {
            Value::String(text) if !text.trim().is_empty() => {
                messages.push(json!({ "role": "user", "content": text }));
            }
            Value::Array(items) => {
                for item in items {
                    convert_responses_input_item_to_chat_messages(item, &mut messages);
                }
            }
            other => convert_responses_input_item_to_chat_messages(other, &mut messages),
        }
    }

    if has_previous_response_id(obj) {
        match previous_messages {
            Some(history) if !history.is_empty() => {
                // 历史 assistant 上下文插到 system（若有）之后的对话头部：
                // 新 input 中的 function_call_output（tool 消息）与新增 user
                // 消息引用这些更早的输出，顺序因此保持正确。
                let insert_at = usize::from(
                    messages
                        .first()
                        .and_then(|message| message.get("role"))
                        .and_then(Value::as_str)
                        == Some("system"),
                );
                for (offset, message) in history.iter().enumerate() {
                    messages.insert(insert_at + offset, message.clone());
                }
            }
            _ if input_is_self_contained(obj) => {
                // input 已携带完整上下文，剥离 previous_response_id 继续。
            }
            _ => {
                let items = obj
                    .get("input")
                    .and_then(Value::as_array)
                    .map(|items| items.len())
                    .unwrap_or(0);
                let outputs = obj
                    .get("input")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter(|item| {
                                item.get("type").and_then(Value::as_str)
                                    == Some("function_call_output")
                            })
                            .count()
                    })
                    .unwrap_or(0);
                return Err(ChatConversionFailure::Incompatible(format!(
                    "response chaining unsupported: previous response context unavailable (input_items={items}, function_call_outputs={outputs})"
                )));
            }
        }
    }
    if !messages.is_empty() {
        chat.insert("messages".to_string(), Value::Array(messages));
    }

    if let Some(tools) = obj.get("tools").and_then(Value::as_array) {
        let mapped_tools = tools
            .iter()
            .filter_map(responses_tool_to_chat_function)
            .collect::<Vec<_>>();
        if !mapped_tools.is_empty() {
            chat.insert("tools".to_string(), Value::Array(mapped_tools));
        }
    }
    if has_previous_response_id(obj) {
        if let Some(history) = previous_messages.filter(|history| !history.is_empty()) {
            // 历史 tool_calls 引用的 function 必须出现在 tools 声明中，
            // 否则部分 Chat 上游拒绝 assistant tool_calls 消息。
            merge_history_tools_into_chat(&mut chat, history);
        }
    }
    if let Some(choice) = obj.get("tool_choice") {
        if let Some(mapped) = responses_tool_choice_to_chat(choice) {
            chat.insert("tool_choice".to_string(), mapped);
        }
    }

    if let Some(max_output_tokens) = obj.get("max_output_tokens") {
        chat.insert(
            "max_completion_tokens".to_string(),
            max_output_tokens.clone(),
        );
    }

    if let Some(effort) = obj
        .get("reasoning")
        .and_then(|value| value.get("effort"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        chat.insert("reasoning_effort".to_string(), Value::String(effort.to_string()));
    }

    if let Some(format) = responses_text_format_to_chat(obj.get("text")) {
        chat.insert("response_format".to_string(), format);
    }

    if chat
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let stream_options = chat
            .entry("stream_options".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(options) = stream_options.as_object_mut() {
            if !options
                .get("include_usage")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                options.insert("include_usage".to_string(), Value::Bool(true));
            }
        }
    }

    if let Some(model) = model_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        chat.insert("model".to_string(), Value::String(model.to_string()));
    }

    serde_json::to_vec(&Value::Object(chat))
        .map_err(|_| ChatConversionFailure::Invalid("serialize chat request failed".to_string()))
}

/// Reject semantics Chat Completions cannot represent before any mapping runs.
///
/// `store` / `previous_response_id` 不在此拒绝：前者直接丢弃，后者由主流程
/// 依据历史上下文或自包含 input 决定合并或拒绝。
fn reject_unsupported_semantics(obj: &Map<String, Value>) -> Result<(), ChatConversionFailure> {
    if obj.contains_key("audio") {
        return Err(ChatConversionFailure::Incompatible(
            "audio modality unsupported".to_string(),
        ));
    }
    if let Some(tools) = obj.get("tools").and_then(Value::as_array) {
        for tool in tools {
            let tool_type = tool
                .get("type")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if !matches!(tool_type, "function" | "") {
                return Err(ChatConversionFailure::Incompatible(format!(
                    "tool type '{tool_type}' unsupported"
                )));
            }
            if tool_type == "" {
                return Err(ChatConversionFailure::Incompatible(
                    "tool without type unsupported".to_string(),
                ));
            }
        }
    }
    if let Some(choice) = obj.get("tool_choice") {
        match choice {
            Value::String(kind) => {
                if !matches!(kind.as_str(), "auto" | "none" | "required") {
                    return Err(ChatConversionFailure::Incompatible(format!(
                        "tool_choice '{kind}' unsupported"
                    )));
                }
            }
            Value::Object(_) => {}
            _ => {
                return Err(ChatConversionFailure::Incompatible(
                    "tool_choice shape unsupported".to_string(),
                ));
            }
        }
    }
    if let Some(format_type) = obj
        .get("text")
        .and_then(|text| text.get("format"))
        .and_then(|format| format.get("type"))
        .and_then(Value::as_str)
    {
        if !matches!(format_type, "text" | "json_object" | "json_schema") {
            return Err(ChatConversionFailure::Incompatible(format!(
                "response format '{format_type}' unsupported"
            )));
        }
    }
    Ok(())
}

/// 请求体是否带非空 `previous_response_id`。
fn has_previous_response_id(obj: &Map<String, Value>) -> bool {
    obj.get("previous_response_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
}

/// input 是否自包含完整上下文：仅已有 assistant 消息或 function_call 项时，
/// 才能在没有缓存的情况下剥离 previous_response_id。普通 user 输入与单独
/// function_call_output 都依赖先前响应，不能静默降级。
fn input_is_self_contained(obj: &Map<String, Value>) -> bool {
    let Some(Value::Array(items)) = obj.get("input") else {
        return false;
    };
    items.iter().any(|item| {
        item.get("type").and_then(Value::as_str) == Some("function_call")
            || (item.get("type").and_then(Value::as_str) == Some("message")
                && item.get("role").and_then(Value::as_str) == Some("assistant"))
    })
}

/// 把历史 assistant 消息里引用的函数合并进 responses `tools`，确保 Chat 上游
/// 不会因 `assistant` tool_calls 引用了未声明的 function 而拒绝请求。
fn merge_history_tools_into_chat(chat: &mut Map<String, Value>, history: &[Value]) {
    let mut declared = std::collections::HashSet::new();
    for tool in chat
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(name) = tool
            .get("function")
            .and_then(|function| function.get("name"))
            .and_then(Value::as_str)
        {
            declared.insert(name.to_string());
        }
    }
    let mut merged: Vec<Value> = chat
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for message in history {
        if message.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        for call in message
            .get("tool_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(name) = call
                .get("function")
                .and_then(|function| function.get("name"))
                .and_then(Value::as_str)
            else {
                continue;
            };
            if !declared.contains(name) {
                declared.insert(name.to_string());
                merged.push(json!({
                    "type": "function",
                    "function": { "name": name },
                }));
            }
        }
    }
    if !merged.is_empty() {
        chat.insert("tools".to_string(), Value::Array(merged));
    }
}

/// Responses role -> Chat role.
fn map_responses_role_to_chat(role: &str) -> &'static str {
    match role {
        "developer" | "system" => "system",
        "assistant" => "assistant",
        "tool" => "tool",
        _ => "user",
    }
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        other => serde_json::to_string(other).ok(),
    }
}

/// Flatten Responses content parts into Chat-compatible content.
fn flatten_responses_message_content(content: &Value) -> Option<Value> {
    match content {
        Value::String(text) => Some(Value::String(text.clone())),
        Value::Array(items) => {
            let mut text_parts = Vec::new();
            let mut multimodal_parts = Vec::new();
            for item in items {
                let Some(item_obj) = item.as_object() else {
                    continue;
                };
                let item_type = item_obj
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                match item_type {
                    "input_text" | "output_text" | "text" => {
                        if let Some(text) = item_obj.get("text").and_then(Value::as_str) {
                            text_parts.push(text.to_string());
                        }
                    }
                    "input_image" => {
                        if let Some(image_url) = item_obj.get("image_url").and_then(Value::as_str) {
                            multimodal_parts.push(json!({
                                "type": "image_url",
                                "image_url": { "url": image_url }
                            }));
                        }
                    }
                    _ => {}
                }
            }
            if !multimodal_parts.is_empty() {
                if !text_parts.is_empty() {
                    multimodal_parts.insert(
                        0,
                        json!({ "type": "text", "text": text_parts.join("\n") }),
                    );
                }
                return Some(Value::Array(multimodal_parts));
            }
            if text_parts.is_empty() {
                None
            } else {
                Some(Value::String(text_parts.join("\n")))
            }
        }
        _ => None,
    }
}

/// Convert one Responses input item into Chat messages.
///
/// Supported item kinds: `message`, `function_call`, `function_call_output`.
/// Any item that cannot be represented is skipped by the caller-provided
/// representation checks; assistant tool calls must survive (mapped to
/// `tool_calls`), never silently dropped.
fn convert_responses_input_item_to_chat_messages(item: &Value, out: &mut Vec<Value>) {
    let Some(item_obj) = item.as_object() else {
        return;
    };
    let item_type = item_obj
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match item_type {
        "function_call_output" => {
            let call_id = item_obj
                .get("call_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let output = item_obj
                .get("output")
                .and_then(value_to_string)
                .unwrap_or_default();
            if call_id.is_empty() && output.is_empty() {
                return;
            }
            out.push(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": output
            }));
        }
        "function_call" => {
            let call_id = item_obj
                .get("call_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let name = item_obj
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let arguments = item_obj
                .get("arguments")
                .map(value_to_string)
                .flatten()
                .unwrap_or_else(|| "{}".to_string());
            if call_id.is_empty() && name.is_empty() {
                return;
            }
            out.push(json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": call_id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": arguments
                    }
                }]
            }));
        }
        "message" => {
            let role = item_obj
                .get("role")
                .and_then(Value::as_str)
                .map(map_responses_role_to_chat)
                .unwrap_or("user");
            let Some(content) = item_obj
                .get("content")
                .and_then(flatten_responses_message_content)
            else {
                return;
            };
            out.push(json!({ "role": role, "content": content }));
        }
        _ => {
            if item_obj.get("role").is_some() && item_obj.get("content").is_some() {
                let role = item_obj
                    .get("role")
                    .and_then(Value::as_str)
                    .map(map_responses_role_to_chat)
                    .unwrap_or("user");
                if let Some(content) = item_obj
                    .get("content")
                    .and_then(flatten_responses_message_content)
                {
                    out.push(json!({ "role": role, "content": content }));
                }
            }
        }
    }
}

/// Responses tool -> Chat `tools` entry, or None when not a function tool.
fn responses_tool_to_chat_function(tool: &Value) -> Option<Value> {
    let tool_obj = tool.as_object()?;
    let tool_type = tool_obj
        .get("type")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if tool_type != "function" {
        return None;
    }
    let mut fn_obj = Map::new();
    if let Some(name) = tool_obj.get("name") {
        fn_obj.insert("name".to_string(), name.clone());
    }
    if let Some(description) = tool_obj.get("description") {
        fn_obj.insert("description".to_string(), description.clone());
    }
    if let Some(parameters) = tool_obj.get("parameters") {
        fn_obj.insert("parameters".to_string(), parameters.clone());
    }
    if let Some(strict) = tool_obj.get("strict") {
        fn_obj.insert("strict".to_string(), strict.clone());
    }
    if fn_obj.is_empty() {
        return None;
    }
    Some(json!({ "type": "function", "function": Value::Object(fn_obj) }))
}

/// Responses tool_choice -> Chat tool_choice. `auto`/`none`/`required` strings
/// pass through; function objects are remapped.
fn responses_tool_choice_to_chat(choice: &Value) -> Option<Value> {
    match choice {
        Value::String(kind) => Some(Value::String(kind.clone())),
        Value::Object(obj) => {
            let is_function = obj
                .get("type")
                .and_then(Value::as_str)
                .map(|kind| kind == "function")
                .unwrap_or(false);
            if !is_function {
                return Some(choice.clone());
            }
            let name = obj.get("name")?;
            Some(json!({
                "type": "function",
                "function": { "name": name }
            }))
        }
        _ => Some(choice.clone()),
    }
}

/// Responses `text.format` -> Chat `response_format`.
fn responses_text_format_to_chat(text: Option<&Value>) -> Option<Value> {
    let format = text?.get("format")?;
    let format_type = format
        .get("type")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    match format_type {
        "json_object" => Some(json!({ "type": "json_object" })),
        "json_schema" => {
            let mut schema = Map::new();
            if let Some(name) = format.get("name") {
                schema.insert("name".to_string(), name.clone());
            }
            if let Some(schema_value) = format.get("schema") {
                schema.insert("schema".to_string(), schema_value.clone());
            }
            if let Some(strict) = format.get("strict") {
                schema.insert("strict".to_string(), strict.clone());
            }
            Some(json!({
                "type": "json_schema",
                "json_schema": Value::Object(schema)
            }))
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "responses_to_chat_completions_tests.rs"]
mod tests;
