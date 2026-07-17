use serde_json::Value;

use super::intent::IMAGE_GENERATION_CAPABILITY;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClassifiedCapabilityError {
    pub code: &'static str,
    pub capability_key: &'static str,
}

fn collect_error_fields(value: &Value, types: &mut Vec<String>, messages: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if key.eq_ignore_ascii_case("type") {
                    if let Some(value) = child.as_str() {
                        types.push(value.to_ascii_lowercase());
                    }
                } else if key.eq_ignore_ascii_case("message") {
                    if let Some(value) = child.as_str() {
                        messages.push(value.to_ascii_lowercase());
                    }
                }
                collect_error_fields(child, types, messages);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_error_fields(item, types, messages);
            }
        }
        _ => {}
    }
}

fn parsed_error_fields(body: &[u8]) -> (Vec<String>, Vec<String>) {
    let mut types = Vec::new();
    let mut messages = Vec::new();
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        collect_error_fields(&value, &mut types, &mut messages);
        return (types, messages);
    }
    if let Ok(text) = std::str::from_utf8(body) {
        for line in text.lines() {
            let Some(data) = line.trim().strip_prefix("data:") else {
                continue;
            };
            if let Ok(value) = serde_json::from_str::<Value>(data.trim()) {
                collect_error_fields(&value, &mut types, &mut messages);
            }
        }
    }
    (types, messages)
}

pub(crate) fn classify_capability_error(
    _status_code: u16,
    body: &[u8],
) -> Option<ClassifiedCapabilityError> {
    let (types, messages) = parsed_error_fields(body);
    let structured_match = types.iter().any(|value| value == "permission_error")
        && messages
            .iter()
            .any(|value| value == "image generation is not enabled for this group");

    // Some compatible relays concatenate a JSON error and an SSE failure frame.
    // Keep the fallback exact and require both stable fields.
    let normalized = String::from_utf8_lossy(body).to_ascii_lowercase();
    let exact_fallback = normalized.contains("\"type\":\"permission_error\"")
        && normalized.contains("image generation is not enabled for this group");
    if structured_match || exact_fallback {
        Some(ClassifiedCapabilityError {
            code: "capability.image_generation_not_enabled",
            capability_key: IMAGE_GENERATION_CAPABILITY,
        })
    } else {
        None
    }
}
