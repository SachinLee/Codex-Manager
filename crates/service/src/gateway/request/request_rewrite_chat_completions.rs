use serde_json::Value;

use super::request_rewrite_shared::{
    path_matches_template, retain_fields_by_templates, TemplateAllowlist,
};

/// 函数 `is_chat_completions_create_path`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - path: 参数 path
///
/// # 返回
/// 返回函数执行结果
fn is_chat_completions_create_path(path: &str) -> bool {
    path_matches_template(path, "/v1/chat/completions")
}

/// 函数 `is_stream_request`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - obj: 参数 obj
///
/// # 返回
/// 返回函数执行结果
fn is_stream_request(obj: &serde_json::Map<String, Value>) -> bool {
    obj.get("stream").and_then(Value::as_bool).unwrap_or(false)
}

/// 函数 `normalize_responses_payload`
///
/// 委托给 protocol_adapter 的共享 Responses->Chat 转换（单一状态机），
/// 避免客户端 Chat 路径与 Aggregate Chat 上游路径维护两套映射。
pub(super) fn normalize_responses_payload(
    path: &str,
    obj: &mut serde_json::Map<String, Value>,
) -> bool {
    if !is_chat_completions_create_path(path) || obj.contains_key("messages") {
        return false;
    }
    let before = serde_json::to_vec(&Value::Object(obj.clone())).unwrap_or_default();
    let converted = match super::super::protocol_adapter::convert_responses_request_to_chat_completions(
        &before,
        None,
        None, // 客户端 Chat 兼容路径不承接 previous_response_id 历史上下文
    ) {
        Ok(body) => body,
        Err(_) => return false,
    };
    let Ok(converted_value) = serde_json::from_slice::<Value>(&converted) else {
        return false;
    };
    let Some(converted_obj) = converted_value.as_object() else {
        return false;
    };
    let mut changed = false;
    // 保留原载荷中的 Chat 兼容字段并合并转换结果；messages 为转换产物，整体替换。
    if let Some(messages) = converted_obj.get("messages") {
        if obj.get("messages") != Some(messages) {
            obj.insert("messages".to_string(), messages.clone());
            changed = true;
        }
    }
    for (key, value) in converted_obj {
        if key == "messages" {
            continue;
        }
        if obj.get(key) != Some(value) {
            obj.insert(key.clone(), value.clone());
            changed = true;
        }
    }
    // 转换器已消费的 Responses 专属键必须从 Chat 载荷中移除，
    // 否则会以非官方字段透传（原有 normalize 契约）。
    if obj.remove("instructions").is_some() {
        changed = true;
    }
    if obj.remove("input").is_some() {
        changed = true;
    }
    changed
}

/// 函数 `ensure_stream_usage_override`
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
pub(super) fn ensure_stream_usage_override(
    path: &str,
    obj: &mut serde_json::Map<String, Value>,
) -> bool {
    if !is_chat_completions_create_path(path) {
        return false;
    }
    if !is_stream_request(obj) {
        return false;
    }
    let mut changed = false;
    let stream_options = obj
        .entry("stream_options".to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if !stream_options.is_object() {
        *stream_options = Value::Object(serde_json::Map::new());
        changed = true;
    }
    if let Some(stream_options_obj) = stream_options.as_object_mut() {
        let has_include_usage = stream_options_obj
            .get("include_usage")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !has_include_usage {
            stream_options_obj.insert("include_usage".to_string(), Value::Bool(true));
            changed = true;
        }
    }
    changed
}

/// 函数 `ensure_reasoning_effort`
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
pub(super) fn ensure_reasoning_effort(
    path: &str,
    obj: &mut serde_json::Map<String, Value>,
) -> bool {
    if !is_chat_completions_create_path(path) {
        return false;
    }

    let mut changed = false;
    if !obj.contains_key("reasoning_effort") {
        let effort = obj
            .get("reasoning")
            .and_then(|v| v.get("effort"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if let Some(effort) = effort {
            obj.insert("reasoning_effort".to_string(), Value::String(effort));
            changed = true;
        }
    }
    if obj.remove("reasoning").is_some() {
        changed = true;
    }
    changed
}

/// 函数 `apply_reasoning_override`
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
pub(super) fn apply_reasoning_override(
    path: &str,
    obj: &mut serde_json::Map<String, Value>,
    reasoning_effort: Option<&str>,
) -> bool {
    if !is_chat_completions_create_path(path) {
        return false;
    }
    let Some(level) = reasoning_effort else {
        return false;
    };
    obj.insert(
        "reasoning_effort".to_string(),
        Value::String(level.to_string()),
    );
    true
}

/// 函数 `is_supported_openai_chat_completions_create_key`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - key: 参数 key
///
/// # 返回
/// 返回函数执行结果
fn is_supported_openai_chat_completions_create_key(key: &str) -> bool {
    matches!(
        key,
        "messages"
            | "model"
            | "audio"
            | "frequency_penalty"
            | "function_call"
            | "functions"
            | "logit_bias"
            | "logprobs"
            | "max_completion_tokens"
            | "max_tokens"
            | "metadata"
            | "modalities"
            | "n"
            | "parallel_tool_calls"
            | "prediction"
            | "presence_penalty"
            | "reasoning_effort"
            | "response_format"
            | "seed"
            | "service_tier"
            | "stop"
            | "store"
            | "stream"
            | "stream_options"
            | "temperature"
            | "tool_choice"
            | "tools"
            | "text"
            | "top_logprobs"
            | "top_p"
            | "user"
            | "verbosity"
            | "web_search_options"
    )
}

/// 函数 `is_supported_openai_chat_completions_metadata_update_key`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - key: 参数 key
///
/// # 返回
/// 返回函数执行结果
fn is_supported_openai_chat_completions_metadata_update_key(key: &str) -> bool {
    matches!(key, "metadata")
}

const CHAT_COMPLETIONS_ALLOWLISTS: &[TemplateAllowlist] = &[
    TemplateAllowlist {
        template: "/v1/chat/completions",
        allow: is_supported_openai_chat_completions_create_key,
    },
    TemplateAllowlist {
        template: "/v1/chat/completions/{completion_id}",
        allow: is_supported_openai_chat_completions_metadata_update_key,
    },
];

/// 函数 `retain_official_fields`
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
pub(super) fn retain_official_fields(
    path: &str,
    obj: &mut serde_json::Map<String, Value>,
) -> Vec<String> {
    retain_fields_by_templates(path, obj, CHAT_COMPLETIONS_ALLOWLISTS)
}
