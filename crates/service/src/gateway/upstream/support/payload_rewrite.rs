use serde_json::Value;

const CONTINUATION_ENCRYPTED_INCLUDE: &str = "reasoning.encrypted_content";

/// 函数 `body_has_encrypted_content_hint`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - in super: 参数 in super
///
/// # 返回
/// 返回函数执行结果
pub(in crate::gateway) fn body_has_encrypted_content_hint(body: &[u8]) -> bool {
    // Fast path: avoid JSON parsing unless we hit a recovery path.
    std::str::from_utf8(body)
        .ok()
        .is_some_and(|text| text.contains("\"encrypted_content\""))
}

/// 函数 `strip_encrypted_content_value`
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
fn strip_encrypted_content_value(value: &mut Value) -> bool {
    match value {
        Value::Object(map) => {
            let mut changed = map.remove("encrypted_content").is_some();
            for child in map.values_mut() {
                if strip_encrypted_content_value(child) {
                    changed = true;
                }
            }
            changed
        }
        Value::Array(items) => {
            let mut changed = false;
            for item in items.iter_mut() {
                if strip_encrypted_content_value(item) {
                    changed = true;
                }
            }
            changed
        }
        _ => false,
    }
}

/// 函数 `strip_encrypted_content_from_body`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - in super: 参数 in super
///
/// # 返回
/// 返回函数执行结果
pub(in crate::gateway) fn strip_encrypted_content_from_body(body: &[u8]) -> Option<Vec<u8>> {
    let mut value: Value = serde_json::from_slice(body).ok()?;
    if !strip_encrypted_content_value(&mut value) {
        return None;
    }
    serde_json::to_vec(&value).ok()
}

fn remove_continuation_encrypted_include(value: &mut Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    let Some(include) = object.get_mut("include") else {
        return;
    };
    let Some(items) = include.as_array_mut() else {
        return;
    };
    items.retain(|item| item.as_str() != Some(CONTINUATION_ENCRYPTED_INCLUDE));
    if items.is_empty() {
        object.remove("include");
    }
}

fn normalize_continuation_input_item(item: &Value) -> Value {
    if let Some(text) = item.as_str() {
        serde_json::json!({
            "type": "message",
            "role": "user",
            "content": text,
        })
    } else {
        item.clone()
    }
}

fn sanitize_continuation_input_item(item: &Value) -> Option<Value> {
    let mut normalized = normalize_continuation_input_item(item);
    if normalized
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|item_type| item_type == "reasoning")
    {
        return None;
    }
    strip_encrypted_content_value(&mut normalized);
    Some(normalized)
}

fn continuation_input_items(input: Option<&Value>) -> Vec<Value> {
    match input {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(sanitize_continuation_input_item)
            .collect(),
        Some(value) => sanitize_continuation_input_item(value)
            .into_iter()
            .collect(),
        None => Vec::new(),
    }
}

pub(in super::super) fn build_continuation_recovery_body(
    body: &[u8],
    marker_text: &str,
) -> Option<Vec<u8>> {
    let mut value: Value = serde_json::from_slice(body).ok()?;
    let input = continuation_input_items(value.get("input"));
    let mut next_input = input;
    next_input.push(serde_json::json!({
        "type": "message",
        "role": "assistant",
        "phase": "commentary",
        "content": [{
            "type": "output_text",
            "text": marker_text,
        }],
    }));
    let object = value.as_object_mut()?;
    object.remove("previous_response_id");
    object.insert("stream".to_string(), Value::Bool(true));
    remove_continuation_encrypted_include(&mut value);
    value["input"] = Value::Array(next_input);
    serde_json::to_vec(&value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn contains_encrypted_content(value: &Value) -> bool {
        match value {
            Value::Object(map) => {
                map.contains_key("encrypted_content")
                    || map.values().any(contains_encrypted_content)
            }
            Value::Array(items) => items.iter().any(contains_encrypted_content),
            _ => false,
        }
    }

    #[test]
    fn build_continuation_recovery_body_replays_clean_input_and_marker() {
        let body = br#"{
            "stream": false,
            "previous_response_id": "resp_previous",
            "include": ["usage", "reasoning.encrypted_content"],
            "input": [
                "hello",
                {"type": "reasoning", "encrypted_content": "enc-1"},
                {"type": "message", "role": "user", "content": [
                    {"type": "input_text", "text": "world", "encrypted_content": "enc-2"}
                ]}
            ]
        }"#;

        let rewritten = build_continuation_recovery_body(body, "Continue").expect("body");
        let value: Value = serde_json::from_slice(&rewritten).expect("json");
        let input = value
            .get("input")
            .and_then(Value::as_array)
            .expect("input array");

        assert_eq!(value.get("stream").and_then(Value::as_bool), Some(true));
        assert!(value.get("previous_response_id").is_none());
        assert!(value
            .get("include")
            .and_then(Value::as_array)
            .is_some_and(|items| items
                .iter()
                .all(|item| item.as_str() != Some(CONTINUATION_ENCRYPTED_INCLUDE))));
        assert_eq!(input.len(), 3);
        assert_eq!(
            input[0].get("content").and_then(Value::as_str),
            Some("hello")
        );
        assert_eq!(
            input[1].get("type").and_then(Value::as_str),
            Some("message")
        );
        let input_json = serde_json::to_string(input).expect("input json");
        assert!(input_json.contains("world"));
        assert!(!input_json.contains("encrypted_content"));
        assert!(!input_json.contains("enc-1"));
        assert!(!input_json.contains("enc-2"));
        assert_eq!(
            input[2].pointer("/content/0/text").and_then(Value::as_str),
            Some("Continue")
        );
    }

    #[test]
    fn strip_encrypted_content_removes_items_that_require_the_field() {
        let body = json!({
            "model": "gpt-5.6-sol",
            "encrypted_content": "legacy-root-secret",
            "metadata": {
                "encrypted_content": "metadata-secret",
                "keep": "metadata",
                "reasoning_envelope": {
                    "type": "reasoning",
                    "id": "metadata-reasoning",
                    "summary": ["keep summary"],
                    "encrypted_content": "metadata-reasoning-secret"
                }
            },
            "input": [
                {
                    "type": "reasoning",
                    "id": "rs_1",
                    "summary": [],
                    "encrypted_content": "reasoning-secret"
                },
                {
                    "type": "agent_message",
                    "content": [
                        { "type": "input_text", "text": "keep me" },
                        {
                            "type": "encrypted_content",
                            "encrypted_content": "nested-secret"
                        }
                    ]
                },
                {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "continue" }]
                }
            ]
        });

        let rewritten = strip_encrypted_content_from_body(
            serde_json::to_vec(&body)
                .expect("serialize body")
                .as_slice(),
        )
        .expect("rewrite body");
        let value: Value = serde_json::from_slice(&rewritten).expect("parse rewritten body");

        assert!(!contains_encrypted_content(&value));
        assert_eq!(
            value["metadata"],
            json!({
                "keep": "metadata",
                "reasoning_envelope": {
                    "type": "reasoning",
                    "id": "metadata-reasoning",
                    "summary": ["keep summary"]
                }
            }),
            "ordinary object properties must remain after their encrypted field is removed"
        );

        let input = value["input"].as_array().expect("input array");
        assert_eq!(input.len(), 2, "reasoning item must be removed");
        assert_eq!(input[0]["type"], "agent_message");
        assert_eq!(
            input[0]["content"],
            json!([{ "type": "input_text", "text": "keep me" }]),
            "encrypted content part must be removed without dropping normal text"
        );
        assert_eq!(input[1]["type"], "message");
    }

}
