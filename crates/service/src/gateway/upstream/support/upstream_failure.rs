//! 上游失败决策边界分类。

use crate::gateway::is_selected_model_capacity_error;

use serde_json::Value;
/// 上游失败后的决策类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::gateway) enum UpstreamFailureDecision {
    /// 请求级终止：明确的请求体或协议错误，不应在任何候选上重试。
    RequestTerminal,
    /// 候选级 failover：候选特有错误；若有后续候选且未交付内容，应推进。
    CandidateFailover,
    /// 同候选内部重试：可能瞬时恢复的传输故障。
    RetrySameCandidate,
    /// 独立容量恢复：精确匹配的容量错误，使用独立预算。
    CapacityRecovery,
    /// 独立能力降级：ChatGPT legacy 兼容重试。
    CapabilityRetry,
    /// 推理守卫内部重试。
    ReasoningGuardRetry,
}

/// 交付前 SSE 终态错误的有限投影。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::gateway) struct UpstreamFailureInfo {
    pub message: String,
    pub code: Option<String>,
}

const MAX_FAILURE_MESSAGE_CHARS: usize = 1024;

fn bounded_failure_text(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.chars().take(MAX_FAILURE_MESSAGE_CHARS).collect())
}

fn failure_info_from_error_value(error: &Value) -> Option<UpstreamFailureInfo> {
    let object = error.as_object()?;
    let code = object
        .get("code")
        .and_then(Value::as_str)
        .or_else(|| object.get("type").and_then(Value::as_str))
        .and_then(bounded_failure_text);
    let message = object
        .get("message")
        .and_then(Value::as_str)
        .and_then(bounded_failure_text)
        .or_else(|| code.clone())?;
    Some(UpstreamFailureInfo { message, code })
}

/// 从 OpenAI 风格 SSE/JSON payload 提取有限的错误事实。
///
/// 只读取显式 `error` 对象及其受支持的嵌套位置；不扫描任意 prompt/output
/// 字符串，避免把正常输出中的错误词误判为候选失败。
pub(in crate::gateway) fn failure_info_from_value(value: &Value) -> Option<UpstreamFailureInfo> {
    let nested_errors = [
        value.get("error"),
        value
            .get("response")
            .and_then(|response| response.get("error")),
        value
            .get("response")
            .and_then(|response| response.get("status_details"))
            .and_then(|details| details.get("error")),
    ];
    nested_errors
        .into_iter()
        .flatten()
        .find_map(failure_info_from_error_value)
        .or_else(|| {
            let message = value
                .get("message")
                .and_then(Value::as_str)
                .and_then(bounded_failure_text)?;
            let code = value
                .get("code")
                .and_then(Value::as_str)
                .and_then(bounded_failure_text);
            Some(UpstreamFailureInfo { message, code })
        })
}

/// 从响应体提取显式 `error.code`，缺失时回退到 `error.type`。
pub(in crate::gateway) fn failure_key_from_value(value: &Value) -> Option<String> {
    failure_info_from_value(value).and_then(|info| info.code)
}

/// 从响应体分类失败决策。
///
/// # 参数
/// - `status_code`: HTTP 状态码
/// - `error_message`: 错误消息文本
/// - `error_code_from_body`: 若响应体为 JSON，提取的 `error.code`/`error.type`
/// - `has_delivered_content`: 是否已向客户端交付语义内容
/// - `is_reasoning_guard_internal_retry`: 是否为推理守卫内部重试
///
/// # 返回
/// 决策类型
pub(in crate::gateway) fn classify_upstream_failure(
    status_code: u16,
    error_message: &str,
    error_code_from_body: Option<&str>,
    has_delivered_content: bool,
    is_reasoning_guard_internal_retry: bool,
) -> UpstreamFailureDecision {
    // 已交付内容后，除推理守卫外不重试/failover
    if has_delivered_content && !is_reasoning_guard_internal_retry {
        return UpstreamFailureDecision::RequestTerminal;
    }

    // 推理守卫内部重试
    if is_reasoning_guard_internal_retry {
        return UpstreamFailureDecision::ReasoningGuardRetry;
    }

    // 容量错误
    if is_selected_model_capacity_error(error_message) {
        return UpstreamFailureDecision::CapacityRecovery;
    }

    // 能力重试
    if is_chatgpt_capability_retry_eligible(error_message) {
        return UpstreamFailureDecision::CapabilityRetry;
    }

    match error_code_from_body {
        Some("invalid_request_error" | "invalid_request") => {
            return UpstreamFailureDecision::RequestTerminal;
        }
        Some(
            "rate_limit_exceeded"
            | "authentication_error"
            | "invalid_api_key"
            | "permission_denied"
            | "model_not_found"
            | "model_not_supported",
        ) => {
            return UpstreamFailureDecision::CandidateFailover;
        }
        _ => {}
    }

    // 请求级终止 4xx
    if matches!(status_code, 400 | 422 | 413) {
        return UpstreamFailureDecision::RequestTerminal;
    }

    // 候选级 failover：401/403/404/405/429/501
    if matches!(status_code, 401 | 403 | 404 | 405 | 429 | 501) {
        return UpstreamFailureDecision::CandidateFailover;
    }

    // 默认为同候选传输重试（5xx、连接错误等）
    UpstreamFailureDecision::RetrySameCandidate
}

fn is_chatgpt_capability_retry_eligible(message: &str) -> bool {
    let normalized = message.trim().to_ascii_lowercase();
    normalized.contains("does not support")
        || normalized.contains("capability not available")
        || normalized.contains("unsupported")
}

/// 从响应体提取 error.code 字段（缺失时回退到 error.type）。
///
/// 仅在响应体已完全缓冲且为 JSON 时尝试解析；否则返回 None。
/// 不持久化完整错误体，避免敏感信息泄漏。
pub(in crate::gateway) fn error_code_from_response_body(body: &[u8]) -> Option<String> {
    if body.is_empty() {
        return None;
    }

    let parsed: Value = serde_json::from_slice(body).ok()?;
    failure_key_from_value(&parsed)
}

/// 从 SSE 终态错误消息中提取 error.code（若有）。
pub(in crate::gateway) fn extract_error_code_from_terminal(
    terminal_error: Option<&str>,
) -> Option<String> {
    let message = terminal_error?;

    if let Ok(parsed) = serde_json::from_str::<Value>(message) {
        return failure_key_from_value(&parsed);
    }

    let normalized = message.trim();
    normalized
        .strip_prefix("code=")
        .and_then(|rest| rest.split_whitespace().next())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_explicit_sse_failure_keys() {
        for key in [
            "rate_limit_exceeded",
            "authentication_error",
            "model_not_found",
        ] {
            assert_eq!(
                classify_upstream_failure(502, "upstream failure", Some(key), false, false),
                UpstreamFailureDecision::CandidateFailover,
                "{key} should advance to the next candidate"
            );
        }
        assert_eq!(
            classify_upstream_failure(
                502,
                "invalid input",
                Some("invalid_request_error"),
                false,
                false
            ),
            UpstreamFailureDecision::RequestTerminal
        );
    }

    #[test]
    fn failure_projection_prefers_code_and_falls_back_to_type() {
        let coded = serde_json::json!({
            "response": {"error": {"code": "rate_limit_exceeded", "type": "server_error", "message": "busy"}}
        });
        assert_eq!(
            failure_key_from_value(&coded).as_deref(),
            Some("rate_limit_exceeded")
        );

        let typed = serde_json::json!({
            "error": {"type": "authentication_error", "message": "invalid key"}
        });
        assert_eq!(
            failure_key_from_value(&typed).as_deref(),
            Some("authentication_error")
        );
    }

    #[test]
    fn failure_projection_bounds_message() {
        let value = serde_json::json!({"error": {"message": "x".repeat(2_000)}});
        assert_eq!(
            failure_info_from_value(&value)
                .unwrap()
                .message
                .chars()
                .count(),
            1024
        );
    }
}
