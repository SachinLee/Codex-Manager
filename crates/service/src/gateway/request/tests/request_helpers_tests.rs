use super::{
    parse_request_metadata, parse_request_metadata_from_value,
    request_log_session_id_candidate_from_value, validate_text_input_limit_for_path,
    validate_text_input_limit_for_value, MAX_TEXT_INPUT_CHARS,
};

#[test]
fn request_metadata_from_value_matches_byte_parser() {
    let value = serde_json::json!({
        "model": "gpt-5.4",
        "reasoning": { "effort": "high" },
        "service_tier": "priority",
        "stream": true,
        "prompt_cache_key": "thread-1",
        "previous_response_id": "resp-1",
        "input": "hello"
    });
    let body = serde_json::to_vec(&value).expect("serialize body");

    let from_body = parse_request_metadata(&body);
    let from_value = parse_request_metadata_from_value(&value);

    assert_eq!(from_value.model, from_body.model);
    assert_eq!(from_value.reasoning_effort, from_body.reasoning_effort);
    assert_eq!(from_value.service_tier, from_body.service_tier);
    assert_eq!(from_value.is_stream, from_body.is_stream);
    assert_eq!(from_value.stream_specified, from_body.stream_specified);
    assert_eq!(
        from_value.has_prompt_cache_key,
        from_body.has_prompt_cache_key
    );
    assert_eq!(from_value.prompt_cache_key, from_body.prompt_cache_key);
    assert_eq!(
        from_value.session_id_candidate,
        from_body.session_id_candidate
    );
    assert_eq!(
        from_value.has_previous_response_id,
        from_body.has_previous_response_id
    );
    assert_eq!(from_value.request_shape, from_body.request_shape);
}

#[test]
fn responses_text_limit_allows_small_payloads() {
    let body = serde_json::json!({
        "instructions": "system",
        "input": [
            {
                "role": "user",
                "content": [
                    { "type": "input_text", "text": "hello" },
                    { "type": "input_text", "text": "world" }
                ]
            }
        ]
    });
    let body = serde_json::to_vec(&body).expect("serialize body");

    let result = validate_text_input_limit_for_path("/v1/responses", &body);

    assert!(result.is_ok());
}

#[test]
fn responses_text_limit_rejects_oversized_payloads() {
    let body = serde_json::json!({
        "input": "x".repeat(MAX_TEXT_INPUT_CHARS + 1),
    });
    let body = serde_json::to_vec(&body).expect("serialize body");

    let err = validate_text_input_limit_for_path("/v1/responses", &body)
        .expect_err("oversized body should be rejected");

    assert_eq!(err.max_chars, MAX_TEXT_INPUT_CHARS);
    assert_eq!(err.actual_chars, MAX_TEXT_INPUT_CHARS + 1);
    assert!(err
        .message()
        .contains("Input exceeds the maximum length of 1048576 characters."));
}

#[test]
fn responses_text_limit_can_validate_preparsed_value() {
    let value = serde_json::json!({
        "input": "x".repeat(MAX_TEXT_INPUT_CHARS + 1),
    });

    let err = validate_text_input_limit_for_value("/v1/responses", &value)
        .expect_err("oversized body should be rejected");

    assert_eq!(err.max_chars, MAX_TEXT_INPUT_CHARS);
    assert_eq!(err.actual_chars, MAX_TEXT_INPUT_CHARS + 1);
}

#[test]
fn chat_completions_text_limit_counts_message_content_and_instructions() {
    let first = "x".repeat(MAX_TEXT_INPUT_CHARS / 2);
    let second = "y".repeat(MAX_TEXT_INPUT_CHARS / 2 + 1);
    let body = serde_json::json!({
        "instructions": first,
        "messages": [
            {
                "role": "user",
                "content": [
                    { "type": "text", "text": second }
                ]
            }
        ]
    });
    let body = serde_json::to_vec(&body).expect("serialize body");

    let err = validate_text_input_limit_for_path("/v1/chat/completions", &body)
        .expect_err("combined text length should be rejected");

    assert_eq!(err.actual_chars, MAX_TEXT_INPUT_CHARS + 1);
}

#[test]
fn non_inference_path_skips_text_limit_validation() {
    let body = serde_json::json!({
        "input": "x".repeat(MAX_TEXT_INPUT_CHARS + 100),
    });
    let body = serde_json::to_vec(&body).expect("serialize body");

    let result = validate_text_input_limit_for_path("/v1/models", &body);

    assert!(result.is_ok());
}

#[test]
fn legacy_completions_path_no_longer_participates_in_text_limit_validation() {
    let body = serde_json::json!({
        "prompt": "x".repeat(MAX_TEXT_INPUT_CHARS + 100),
    });
    let body = serde_json::to_vec(&body).expect("serialize body");

    let result = validate_text_input_limit_for_path("/v1/completions", &body);

    assert!(result.is_ok());
}

#[test]
fn request_log_session_id_uses_codex_thread_prompt_cache_key() {
    let body = serde_json::json!({
        "prompt_cache_key": "019e6d9b-c5a1-72d2-a13d-e189680767e0"
    });

    let actual = request_log_session_id_candidate_from_value(&body);

    assert_eq!(
        actual.as_deref(),
        Some("019e6d9b-c5a1-72d2-a13d-e189680767e0")
    );
}

#[test]
fn request_log_session_id_rejects_route_anchor_prompt_cache_key() {
    let body = serde_json::json!({
        "prompt_cache_key": "pck:v1:88b88b2962ad13493615976027b41c92"
    });

    let actual = request_log_session_id_candidate_from_value(&body);

    assert_eq!(actual, None);
}

#[test]
fn request_log_session_id_ignores_generic_prompt_cache_key() {
    let body = serde_json::json!({
        "prompt_cache_key": "client-thread-alias"
    });

    let actual = request_log_session_id_candidate_from_value(&body);

    assert_eq!(actual, None);
}

#[test]
fn request_log_session_id_accepts_local_prompt_cache_key() {
    let body = serde_json::json!({
        "prompt_cache_key": "local:019e6d9b-c5a1-72d2-a13d-e189680767e0"
    });

    let actual = request_log_session_id_candidate_from_value(&body);

    assert_eq!(
        actual.as_deref(),
        Some("local:019e6d9b-c5a1-72d2-a13d-e189680767e0")
    );
}

#[test]
fn request_log_session_id_rejects_unsafe_or_unbounded_explicit_ids() {
    for thread_id in ["thread\nid".to_string(), "x".repeat(257)] {
        let body = serde_json::json!({
            "client_metadata": {
                "thread_id": thread_id
            }
        });

        assert_eq!(request_log_session_id_candidate_from_value(&body), None);
    }
}

#[test]
fn request_log_session_id_rejects_invalid_local_prompt_cache_keys() {
    for prompt_cache_key in ["local:".to_string(), format!("local:{}", "x".repeat(257))] {
        let body = serde_json::json!({
            "prompt_cache_key": prompt_cache_key
        });

        assert_eq!(request_log_session_id_candidate_from_value(&body), None);
    }
}

#[test]
fn request_log_session_id_accepts_explicit_client_metadata_thread_id() {
    let body = serde_json::json!({
        "client_metadata": {
            "thread_id": "019e6d9b-c5a1-72d2-a13d-e189680767e0"
        },
        "prompt_cache_key": "pck:v1:88b88b2962ad13493615976027b41c92"
    });

    let actual = request_log_session_id_candidate_from_value(&body);

    assert_eq!(
        actual.as_deref(),
        Some("019e6d9b-c5a1-72d2-a13d-e189680767e0")
    );
}

#[test]
fn parse_request_metadata_exposes_request_log_session_id_candidate() {
    let body = serde_json::json!({
        "prompt_cache_key": "019e6d9b-c5a1-72d2-a13d-e189680767e0"
    });
    let body = serde_json::to_vec(&body).expect("serialize body");

    let actual = parse_request_metadata(&body);

    assert_eq!(
        actual.session_id_candidate.as_deref(),
        Some("019e6d9b-c5a1-72d2-a13d-e189680767e0")
    );
}
