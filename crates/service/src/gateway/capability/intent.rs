use serde_json::Value;

pub(crate) const IMAGE_GENERATION_CAPABILITY: &str = "responses.hosted_tool.image_generation";
pub(crate) const REQUIRED_CAPABILITIES_HEADER: &str = "x-codexmanager-required-capabilities";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapabilityRequirement {
    Absent,
    Optional,
    Required,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResponsesCapabilityIntent {
    pub image_generation: CapabilityRequirement,
}

fn is_image_generation_tool(value: &Value) -> bool {
    value
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|tool_type| tool_type.eq_ignore_ascii_case("image_generation"))
}

fn tool_choice_requires_image_generation(value: Option<&Value>) -> bool {
    let Some(value) = value else {
        return false;
    };
    match value {
        Value::Object(object) => object
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|tool_type| tool_type.eq_ignore_ascii_case("image_generation")),
        Value::String(value) => value.eq_ignore_ascii_case("image_generation"),
        _ => false,
    }
}

pub(crate) fn inspect_responses_capabilities(
    path: &str,
    body: &[u8],
    image_generation_declared_required: bool,
) -> ResponsesCapabilityIntent {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return ResponsesCapabilityIntent {
            image_generation: CapabilityRequirement::Absent,
        };
    };
    let image_tool_present = value
        .get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| tools.iter().any(is_image_generation_tool));
    if !image_tool_present {
        return ResponsesCapabilityIntent {
            image_generation: CapabilityRequirement::Absent,
        };
    }

    let path_requires_image = path == "/v1/images/generations"
        || path == "/images/generations"
        || path.ends_with("/images/generations");
    let requirement = if image_generation_declared_required
        || path_requires_image
        || tool_choice_requires_image_generation(value.get("tool_choice"))
    {
        CapabilityRequirement::Required
    } else {
        CapabilityRequirement::Optional
    };
    ResponsesCapabilityIntent {
        image_generation: requirement,
    }
}

pub(crate) fn parse_required_capabilities(value: &str) -> Result<Vec<&'static str>, String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| match item {
            IMAGE_GENERATION_CAPABILITY => Ok(IMAGE_GENERATION_CAPABILITY),
            _ => Err(format!("unsupported required capability: {item}")),
        })
        .collect()
}

pub(crate) fn structural_contract_signature(body: &[u8]) -> String {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return "v1|json=invalid".to_string();
    };
    let tools = value.get("tools").and_then(Value::as_array);
    let tool_count = tools.map_or(0, Vec::len);
    let image_tool_count = tools.map_or(0, |items| {
        items
            .iter()
            .filter(|item| is_image_generation_tool(item))
            .count()
    });
    let function_tool_count = tools.map_or(0, |items| {
        items
            .iter()
            .filter(|item| item.get("type").and_then(Value::as_str) == Some("function"))
            .count()
    });
    let input_count = value
        .get("input")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let stream = value
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let previous = value.get("previous_response_id").is_some();
    let encrypted = value
        .get("include")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|item| {
                item.as_str()
                    .is_some_and(|value| value.contains("encrypted_content"))
            })
        });
    format!(
        "v1|input={input_count}|tools={tool_count}|image={image_tool_count}|function={function_tool_count}|stream={}|previous={}|encrypted={}",
        u8::from(stream),
        u8::from(previous),
        u8::from(encrypted)
    )
}
