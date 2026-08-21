use bytes::Bytes;
use tiny_http::Request;

use super::super::super::request_log::RequestLogUsage;
use super::super::support::payload_rewrite::build_continuation_recovery_body;
use super::super::GatewayUpstreamResponse;
use super::execution_context::GatewayUpstreamExecutionContext;

pub(super) enum FinalizeUpstreamResponseOutcome {
    Handled,
    RetrySameCandidate {
        request: Request,
        reason: RetrySameCandidateReason,
        body_override: Option<Bytes>,
    },
    Failover {
        request: Request,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RetrySameCandidateReason {
    ReasoningGuard,
    UpstreamCapacity,
}

/// 函数 `respond_terminal`
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
pub(in super::super) fn respond_terminal(
    request: Request,
    status_code: u16,
    message: String,
    trace_id: Option<&str>,
) -> Result<(), String> {
    let response_message = super::super::super::error_message_for_client(
        super::super::super::prefers_raw_errors_for_tiny_http_request(&request),
        message,
    );
    let response = super::super::super::error_response::terminal_text_response(
        status_code,
        response_message,
        trace_id,
    );
    let _ = request.respond(response);
    Ok(())
}

/// 函数 `is_client_disconnect_error`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - message: 参数 message
///
/// # 返回
/// 返回函数执行结果
fn is_client_disconnect_error(message: &str) -> bool {
    let normalized = message.trim().to_ascii_lowercase();
    normalized.contains("broken pipe")
        || normalized.contains("connection reset")
        || normalized.contains("connection aborted")
        || normalized.contains("connection was forcibly closed")
        || normalized.contains("os error 32")
        || normalized.contains("os error 54")
        || normalized.contains("os error 104")
}

fn derive_final_error(
    status_code: u16,
    last_attempt_error: Option<&str>,
    upstream_error_hint: Option<&str>,
    bridge_error_message: Option<String>,
) -> Option<String> {
    upstream_error_hint
        .map(str::to_string)
        .or_else(|| {
            (status_code >= 400)
                .then(|| last_attempt_error.map(str::to_string))
                .flatten()
        })
        .or(bridge_error_message)
}

fn derive_status_for_log(
    status_code: u16,
    delivered_status_code: Option<u16>,
    bridge_ok: bool,
    gateway_failover: bool,
    upstream_stream_failed: bool,
    client_delivery_failed: bool,
) -> u16 {
    if client_delivery_failed {
        499
    } else if let Some(delivered_status_code) = delivered_status_code {
        delivered_status_code
    } else if status_code >= 400 {
        status_code
    } else if upstream_stream_failed || gateway_failover || !bridge_ok {
        502
    } else {
        status_code
    }
}

/// 函数 `respond_total_timeout`
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
pub(super) fn respond_total_timeout(
    request: Request,
    context: &GatewayUpstreamExecutionContext<'_>,
    trace_id: &str,
    started_at: std::time::Instant,
    model_for_log: Option<&str>,
    attempted_account_ids: Option<&[String]>,
) -> Result<(), String> {
    let message = "upstream total timeout exceeded".to_string();
    context.log_final_result_with_model(
        None,
        None,
        model_for_log,
        504,
        RequestLogUsage::default(),
        Some(message.as_str()),
        started_at.elapsed().as_millis(),
        attempted_account_ids,
    );
    respond_terminal(request, 504, message, Some(trace_id))
}

/// 函数 `finalize_terminal_candidate`
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
pub(super) fn finalize_terminal_candidate(
    request: Request,
    context: &GatewayUpstreamExecutionContext<'_>,
    account_id: &str,
    last_attempt_url: Option<&str>,
    status_code: u16,
    message: String,
    trace_id: &str,
    started_at: std::time::Instant,
    model_for_log: Option<&str>,
    attempted_account_ids: Option<&[String]>,
) -> Result<(), String> {
    let _ = context.mark_account_unavailable_for_gateway_error(account_id, &message);
    context.log_final_result_with_model(
        Some(account_id),
        last_attempt_url,
        model_for_log,
        status_code,
        RequestLogUsage::default(),
        Some(message.as_str()),
        started_at.elapsed().as_millis(),
        attempted_account_ids,
    );
    respond_terminal(request, status_code, message, Some(trace_id))
}

/// 函数 `finalize_upstream_response`
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
#[allow(clippy::too_many_arguments)]
pub(super) fn finalize_upstream_response(
    request: Request,
    response: GatewayUpstreamResponse,
    inflight_guard: super::super::super::AccountInFlightGuard,
    context: &GatewayUpstreamExecutionContext<'_>,
    account_id: &str,
    last_attempt_url: Option<&str>,
    last_attempt_error: Option<&str>,
    response_adapter: super::super::super::ResponseAdapter,
    gemini_stream_output_mode: Option<super::super::super::GeminiStreamOutputMode>,
    tool_name_restore_map: &super::super::super::ToolNameRestoreMap,
    client_is_stream: bool,
    path: &str,
    trace_id: &str,
    started_at: std::time::Instant,
    request_deadline: Option<std::time::Instant>,
    model_for_log: Option<&str>,
    attempted_account_ids: Option<&[String]>,
    has_more_candidates: bool,
    reasoning_guard_retry_budget_remaining: usize,
    capacity_retry_budget_remaining: usize,
    attempt_body: &Bytes,
) -> Result<FinalizeUpstreamResponseOutcome, String> {
    let status_code = response.status().as_u16();
    // 在桥接消费响应前读取上游 Retry-After，供容量重放等待决策使用。
    // sleep_capacity_wait 内部做有界解析；这里只保留原始头值，避免重复解析。
    let upstream_retry_after = match &response {
        GatewayUpstreamResponse::Blocking(resp) => resp
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        GatewayUpstreamResponse::Stream(_) => None,
    };

    let mut bridge = super::super::super::respond_with_upstream(
        request,
        response,
        inflight_guard,
        response_adapter,
        None,
        gemini_stream_output_mode,
        path,
        Some(tool_name_restore_map),
        client_is_stream,
        // Once the bridge starts, tiny_http owns the request and it cannot be retried.
        // Retryable stream errors are therefore gated before this function is called.
        false,
        Some(trace_id),
        model_for_log,
        Some(account_id),
        reasoning_guard_retry_budget_remaining,
        started_at,
    )?;
    let bridge_output_text_len = bridge
        .usage
        .output_text
        .as_deref()
        .map(str::trim)
        .map(str::len)
        .unwrap_or(0);
    super::super::super::trace_log::log_bridge_result(
        super::super::super::trace_log::BridgeResultLog {
            trace_id,
            adapter: format!("{response_adapter:?}").as_str(),
            path,
            is_stream: client_is_stream,
            stream_terminal_seen: bridge.stream_terminal_seen,
            stream_terminal_error: bridge.stream_terminal_error.as_deref(),
            delivery_error: bridge.delivery_error.as_deref(),
            output_text_len: bridge_output_text_len,
            output_tokens: bridge.usage.output_tokens,
            first_response_ms: bridge.usage.first_response_ms,
            delivered_status_code: bridge.delivered_status_code,
            upstream_error_hint: bridge.upstream_error_hint.as_deref(),
            upstream_request_id: bridge.upstream_request_id.as_deref(),
            upstream_cf_ray: bridge.upstream_cf_ray.as_deref(),
            upstream_auth_error: bridge.upstream_auth_error.as_deref(),
            upstream_identity_error_code: bridge.upstream_identity_error_code.as_deref(),
            upstream_content_type: bridge.upstream_content_type.as_deref(),
            last_sse_event_type: bridge.last_sse_event_type.as_deref(),
        },
    );
    let bridge_ok = bridge.is_ok(client_is_stream);
    let bridge_error_message = (!bridge_ok).then(|| {
        bridge
            .error_message(client_is_stream)
            .unwrap_or_else(|| "upstream response incomplete".to_string())
    });
    let final_error = derive_final_error(
        status_code,
        last_attempt_error,
        bridge.upstream_error_hint.as_deref(),
        bridge_error_message,
    );
    let should_retry_reasoning_guard = matches!(
        bridge.reasoning_guard_action,
        Some(super::super::super::ReasoningGuardBridgeAction::InternalRetry)
            | Some(super::super::super::ReasoningGuardBridgeAction::ContinuationRecovery)
    );
    let should_retry_upstream_capacity = final_error
        .as_deref()
        .is_some_and(super::super::super::is_selected_model_capacity_error)
        && bridge.pending_failover_request.is_some();
    // 容量命中指标与 Aggregate API 路径语义一致：每次识别到容量错误计一次，
    // 无论是否具备重放条件（已交付流的容量错误同样计入命中）。
    if final_error
        .as_deref()
        .is_some_and(super::super::super::is_selected_model_capacity_error)
    {
        super::super::super::record_gateway_upstream_capacity_error();
    }
    let reasoning_guard_retry_attempt_index = crate::gateway::reasoning_guard_retry_attempts()
        .saturating_sub(reasoning_guard_retry_budget_remaining)
        as i64;
    if let Some(action) = bridge.reasoning_guard_action {
        context.record_reasoning_guard_event(
            Some(account_id),
            model_for_log,
            action,
            bridge.reasoning_guard_target_token,
            client_is_stream,
            reasoning_guard_retry_attempt_index,
            bridge.delivered_status_code,
            RequestLogUsage {
                input_tokens: bridge.usage.input_tokens,
                cached_input_tokens: bridge.usage.cached_input_tokens,
                cache_write_tokens: bridge.usage.cache_write_tokens,
                output_tokens: bridge.usage.output_tokens,
                total_tokens: bridge.usage.total_tokens,
                reasoning_output_tokens: bridge.usage.reasoning_output_tokens,
                provider_cost_usd_ticks: bridge.usage.provider_cost_usd_ticks,
                provider_cost_nano_usd: bridge.usage.provider_cost_nano_usd,
                first_response_ms: bridge.usage.first_response_ms,
                ..Default::default()
            },
        );
    }
    if should_retry_reasoning_guard {
        if let Some(request) = bridge.pending_failover_request.take() {
            let body_override = if bridge.reasoning_guard_action
                == Some(super::super::super::ReasoningGuardBridgeAction::ContinuationRecovery)
            {
                build_continuation_recovery_body(
                    attempt_body.as_ref(),
                    &crate::gateway::current_reasoning_guard_continuation_marker_text(),
                )
                .map(Bytes::from)
                .or_else(|| {
                    log::warn!(
                        "event=gateway_reasoning_guard_continuation_body_unavailable trace_id={} account_id={}",
                        trace_id,
                        account_id
                    );
                    None
                })
            } else {
                None
            };
            return Ok(FinalizeUpstreamResponseOutcome::RetrySameCandidate {
                request,
                reason: RetrySameCandidateReason::ReasoningGuard,
                body_override,
            });
        }
    }
    if should_retry_upstream_capacity && capacity_retry_budget_remaining > 0 {
        if let Some(request) = bridge.pending_failover_request.take() {
            // 与 Aggregate API 路径共享的容量等待决策：优先合法 Retry-After，
            // 否则有界全抖动；均受 request deadline 约束。
            let retry_attempt = super::candidate_executor::MAX_UPSTREAM_CAPACITY_RETRIES
                .saturating_sub(capacity_retry_budget_remaining)
                as u32;
            if !super::super::support::capacity::sleep_capacity_wait(
                upstream_retry_after.as_deref(),
                retry_attempt,
                request_deadline,
            ) {
                log::warn!(
                    "event=gateway_upstream_capacity_recovery_deadline trace_id={} account_id={} retry_attempt={}",
                    trace_id,
                    account_id,
                    retry_attempt.saturating_add(1),
                );
                respond_total_timeout(
                    request,
                    context,
                    trace_id,
                    started_at,
                    model_for_log,
                    attempted_account_ids,
                )?;
                return Ok(FinalizeUpstreamResponseOutcome::Handled);
            }
            super::super::super::record_gateway_upstream_capacity_internal_retry();
            return Ok(FinalizeUpstreamResponseOutcome::RetrySameCandidate {
                request,
                reason: RetrySameCandidateReason::UpstreamCapacity,
                body_override: None,
            });
        }
    }
    let gateway_error_follow_up = if should_retry_reasoning_guard || should_retry_upstream_capacity
    {
        None
    } else {
        final_error.as_deref().map(|error| {
            context.apply_gateway_error_follow_up(account_id, error, has_more_candidates)
        })
    };
    let gateway_failover =
        gateway_error_follow_up.is_some_and(|follow_up| follow_up.should_failover);

    let upstream_stream_failed = client_is_stream
        && (!bridge.stream_terminal_seen || bridge.stream_terminal_error.is_some());
    let client_delivery_failed = bridge
        .delivery_error
        .as_deref()
        .is_some_and(is_client_disconnect_error);
    let status_for_log = derive_status_for_log(
        status_code,
        bridge.delivered_status_code,
        bridge_ok,
        gateway_failover,
        upstream_stream_failed,
        client_delivery_failed,
    );
    // 容量预算耗尽时客户端收到一次 502 终态；日志记录同一状态，保持跨层一致。
    let status_for_log = if should_retry_upstream_capacity && capacity_retry_budget_remaining == 0 {
        502
    } else {
        status_for_log
    };

    if upstream_stream_failed {
        super::super::super::mark_account_cooldown(
            account_id,
            super::super::super::CooldownReason::Network,
        );
        super::super::super::record_route_quality(account_id, 502);
    }

    let usage = bridge.usage;
    if reasoning_guard_retry_attempt_index > 0
        && bridge.reasoning_guard_action.is_none()
        && bridge_ok
    {
        context.record_reasoning_guard_recovered_event(
            Some(account_id),
            model_for_log,
            client_is_stream,
            reasoning_guard_retry_attempt_index,
            Some(status_for_log),
        );
    }
    context.log_final_result_with_model(
        Some(account_id),
        last_attempt_url,
        model_for_log,
        status_for_log,
        RequestLogUsage {
            input_tokens: usage.input_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            cache_write_tokens: usage.cache_write_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.total_tokens,
            reasoning_output_tokens: usage.reasoning_output_tokens,
            provider_cost_usd_ticks: usage.provider_cost_usd_ticks,
            provider_cost_nano_usd: usage.provider_cost_nano_usd,
            first_response_ms: usage.first_response_ms,
            estimated_input_tokens: None,
        },
        final_error.as_deref(),
        started_at.elapsed().as_millis(),
        attempted_account_ids,
    );
    if should_retry_upstream_capacity {
        if let Some(request) = bridge.pending_failover_request.take() {
            // 容量预算耗尽：以 502 终态结束，保留原始上游诊断；
            // 不冷却账号、不进入网关错误后续处理。
            super::super::super::record_gateway_upstream_capacity_exhausted();
            return respond_terminal(
                request,
                502,
                final_error.unwrap_or_else(|| "upstream capacity error".to_string()),
                Some(trace_id),
            )
            .map(|_| FinalizeUpstreamResponseOutcome::Handled);
        }
    }
    if gateway_failover {
        if let Some(request) = bridge.pending_failover_request.take() {
            return Ok(FinalizeUpstreamResponseOutcome::Failover { request });
        }
    }
    Ok(FinalizeUpstreamResponseOutcome::Handled)
}

#[cfg(test)]
#[path = "response_finalize_tests.rs"]
mod tests;
