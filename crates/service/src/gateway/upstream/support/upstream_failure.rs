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

/// 从上游响应分类失败决策。
///
/// # 参数
/// - `status_code`: HTTP 状态码
/// - `error_message`: 错误消息文本
/// - `error_code_from_body`: 若响应体为 JSON，提取的 `error.code` 字段
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

    // 请求级终止 4xx
    if matches!(status_code, 400 | 422 | 413) {
        return UpstreamFailureDecision::RequestTerminal;
    }

    // 候选级 failover：401/403/404/405/429/501
    // 或错误码为 rate_limit_exceeded（即使外层是 502）
    if matches!(status_code, 401 | 403 | 404 | 405 | 429 | 501)
        || error_code_from_body == Some("rate_limit_exceeded")
    {
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

/// 从响应体提取 error.code 字段（OpenAI/Anthropic 风格）。
///
/// 仅在响应体已完全缓冲且为 JSON 时尝试解析；否则返回 None。
/// 不持久化完整错误体，避免敏感信息泄漏。
pub(in crate::gateway) fn error_code_from_response_body(body: &[u8]) -> Option<String> {
    if body.is_empty() {
        return None;
    }

    let parsed: Value = serde_json::from_slice(body).ok()?;
    parsed
        .get("error")?
        .get("code")?
        .as_str()
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_request_terminal_4xx() {
        assert_eq!(
            classify_upstream_failure(400, "Bad Request", None, false, false),
            UpstreamFailureDecision::RequestTerminal
        );
        assert_eq!(
            classify_upstream_failure(422, "Unprocessable Entity", None, false, false),
            UpstreamFailureDecision::RequestTerminal
        );
        assert_eq!(
            classify_upstream_failure(413, "Request Too Large", None, false, false),
            UpstreamFailureDecision::RequestTerminal
        );
    }

    #[test]
    fn classify_candidate_failover_status_codes() {
        for code in [401, 403, 404, 405, 429, 501] {
            assert_eq!(
                classify_upstream_failure(code, "", None, false, false),
                UpstreamFailureDecision::CandidateFailover,
                "status {} should trigger CandidateFailover",
                code
            );
        }
    }

    #[test]
    fn classify_rate_limit_exceeded_from_body() {
        // 502 但错误码明确为 rate_limit_exceeded
        assert_eq!(
            classify_upstream_failure(502, "error", Some("rate_limit_exceeded"), false, false),
            UpstreamFailureDecision::CandidateFailover
        );

        // 502 且错误码为其他，应归为 RetrySameCandidate
        assert_eq!(
            classify_upstream_failure(502, "error", Some("internal_error"), false, false),
            UpstreamFailureDecision::RetrySameCandidate
        );
    }

    #[test]
    fn classify_capacity_error() {
        assert_eq!(
            classify_upstream_failure(
                502,
                "selected model is at capacity. please try a different model",
                None,
                false,
                false
            ),
            UpstreamFailureDecision::CapacityRecovery
        );
    }

    #[test]
    fn classify_retry_same_candidate_5xx() {
        assert_eq!(
            classify_upstream_failure(500, "Internal Server Error", None, false, false),
            UpstreamFailureDecision::RetrySameCandidate
        );
        assert_eq!(
            classify_upstream_failure(503, "Service Unavailable", None, false, false),
            UpstreamFailureDecision::RetrySameCandidate
        );
    }

    #[test]
    fn classify_terminal_after_delivery() {
        assert_eq!(
            classify_upstream_failure(500, "", None, true, false),
            UpstreamFailureDecision::RequestTerminal
        );
    }

    #[test]
    fn classify_reasoning_guard_retry() {
        assert_eq!(
            classify_upstream_failure(500, "", None, true, true),
            UpstreamFailureDecision::ReasoningGuardRetry
        );
    }

    #[test]
    fn extract_rate_limit_exceeded() {
        let body = r#"{"error":{"code":"rate_limit_exceeded","message":"Rate limit reached"}}"#;
        assert_eq!(
            error_code_from_response_body(body.as_bytes()),
            Some("rate_limit_exceeded".to_string())
        );
    }

    #[test]
    fn extract_none_for_non_json() {
        assert_eq!(error_code_from_response_body(b"plain text"), None);
    }

    #[test]
    fn extract_none_for_missing_code() {
        let body = r#"{"error":{"message":"something"}}"#;
        assert_eq!(error_code_from_response_body(body.as_bytes()), None);
    }

    #[test]
    fn extract_none_for_empty_body() {
        assert_eq!(error_code_from_response_body(b""), None);
    }
}
