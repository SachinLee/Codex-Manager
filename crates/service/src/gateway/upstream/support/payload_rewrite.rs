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
pub(in super::super) fn body_has_encrypted_content_hint(body: &[u8]) -> bool {
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
pub(in super::super) fn strip_encrypted_content_from_body(body: &[u8]) -> Option<Vec<u8>> {
    let mut value: Value = serde_json::from_slice(body).ok()?;
    if !strip_encrypted_content_value(&mut value) {
        return None;
    }
    serde_json::to_vec(&value).ok()
}

fn request_stream_enabled(value: &Value) -> bool {
    value
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn merge_encrypted_include(value: &mut Value) -> bool {
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    let include = object
        .entry("include".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    if !include.is_array() {
        *include = Value::Array(Vec::new());
    }
    let Some(items) = include.as_array_mut() else {
        return false;
    };
    if items
        .iter()
        .any(|item| item.as_str() == Some(CONTINUATION_ENCRYPTED_INCLUDE))
    {
        return false;
    }
    items.push(Value::String(CONTINUATION_ENCRYPTED_INCLUDE.to_string()));
    true
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

fn continuation_input_items(input: Option<&Value>) -> Vec<Value> {
    match input {
        Some(Value::Array(items)) => items
            .iter()
            .map(normalize_continuation_input_item)
            .collect(),
        Some(value) => vec![normalize_continuation_input_item(value)],
        None => Vec::new(),
    }
}

pub(in super::super) fn add_reasoning_encrypted_include_to_stream_body(
    body: &[u8],
) -> Option<Vec<u8>> {
    let mut value: Value = serde_json::from_slice(body).ok()?;
    if !request_stream_enabled(&value) || !merge_encrypted_include(&mut value) {
        return None;
    }
    serde_json::to_vec(&value).ok()
}

pub(in super::super) fn build_continuation_recovery_body(
    body: &[u8],
    reasoning_items: &[Value],
    marker_text: &str,
) -> Option<Vec<u8>> {
    if reasoning_items.is_empty() {
        return None;
    }
    let mut value: Value = serde_json::from_slice(body).ok()?;
    let input = continuation_input_items(value.get("input"));
    let mut next_input = input;
    next_input.extend(reasoning_items.iter().cloned());
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
    object.insert("stream".to_string(), Value::Bool(true));
    merge_encrypted_include(&mut value);
    value["input"] = Value::Array(next_input);
    serde_json::to_vec(&value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_reasoning_encrypted_include_to_stream_body_preserves_existing_include() {
        let body = br#"{"stream":true,"include":["usage"]}"#;

        let rewritten = add_reasoning_encrypted_include_to_stream_body(body).expect("rewritten");
        let value: Value = serde_json::from_slice(&rewritten).expect("json");

        assert_eq!(
            value.get("include").and_then(Value::as_array).cloned(),
            Some(vec![
                Value::String("usage".to_string()),
                Value::String(CONTINUATION_ENCRYPTED_INCLUDE.to_string()),
            ])
        );
    }

    #[test]
    fn add_reasoning_encrypted_include_to_stream_body_skips_duplicate_include() {
        let body = br#"{"stream":true,"include":["reasoning.encrypted_content"]}"#;

        assert!(add_reasoning_encrypted_include_to_stream_body(body).is_none());
    }

    #[test]
    fn build_continuation_recovery_body_appends_reasoning_and_marker() {
        let body = br#"{"stream":false,"input":"hello"}"#;
        let reasoning_items = vec![serde_json::json!({
            "type": "reasoning",
            "encrypted_content": "enc-1"
        })];

        let rewritten =
            build_continuation_recovery_body(body, &reasoning_items, "Continue").expect("body");
        let value: Value = serde_json::from_slice(&rewritten).expect("json");
        let input = value
            .get("input")
            .and_then(Value::as_array)
            .expect("input array");

        assert_eq!(value.get("stream").and_then(Value::as_bool), Some(true));
        assert!(value
            .get("include")
            .and_then(Value::as_array)
            .is_some_and(|items| items
                .iter()
                .any(|item| item.as_str() == Some(CONTINUATION_ENCRYPTED_INCLUDE))));
        assert_eq!(input.len(), 3);
        assert_eq!(
            input[0].get("content").and_then(Value::as_str),
            Some("hello")
        );
        assert_eq!(
            input[1].get("encrypted_content").and_then(Value::as_str),
            Some("enc-1")
        );
        assert_eq!(
            input[2].pointer("/content/0/text").and_then(Value::as_str),
            Some("Continue")
        );
    }
}
