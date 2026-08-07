        }
        Ok(())
    }
}

fn cache_hit_rate(input_tokens: i64, cached_input_tokens: i64) -> f64 {
    if input_tokens <= 0 {
        return 0.0;
    }
    let cached = cached_input_tokens.clamp(0, input_tokens);
    cached as f64 / input_tokens as f64
}

fn final_attempted_aggregate_api_id(
    attempted_api_ids_json: Option<&str>,
    initial_api_id: Option<&str>,
) -> Option<String> {
    attempted_api_ids_json
        .and_then(|value| serde_json::from_str::<JsonValue>(value).ok())
        .and_then(|value| match value {
            JsonValue::Array(items) => items.into_iter().rev().find_map(|item| match item {
                JsonValue::String(value) => non_blank_str(value.as_str()),
                _ => None,
            }),
            _ => None,
        })
        .or_else(|| initial_api_id.and_then(non_blank_str))
}

fn non_blank_owned(value: Option<String>) -> Option<String> {
    value.and_then(|value| non_blank_str(value.as_str()))
}

fn non_blank_str(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
#[path = "tests/request_token_stats_tests.rs"]
mod tests;
