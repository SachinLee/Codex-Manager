use bytes::Bytes;
use serde_json::Value;

use super::intent::{inspect_responses_capabilities, CapabilityRequirement};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransformCode {
    DropOptionalImageGeneration,
}

impl TransformCode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::DropOptionalImageGeneration => "drop_optional_image_generation",
        }
    }
}

pub(crate) fn apply_transform(body: &[u8], transform: TransformCode) -> Option<Bytes> {
    match transform {
        TransformCode::DropOptionalImageGeneration => drop_optional_image_generation(body),
    }
}

fn drop_optional_image_generation(body: &[u8]) -> Option<Bytes> {
    if inspect_responses_capabilities("/v1/responses", body, false).image_generation
        != CapabilityRequirement::Optional
    {
        return None;
    }
    let mut value = serde_json::from_slice::<Value>(body).ok()?;
    let tools = value.get_mut("tools")?.as_array_mut()?;
    let original_len = tools.len();
    tools.retain(|tool| {
        !tool
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|tool_type| tool_type.eq_ignore_ascii_case("image_generation"))
    });
    if tools.len() == original_len {
        return None;
    }
    serde_json::to_vec(&value).ok().map(Bytes::from)
}
