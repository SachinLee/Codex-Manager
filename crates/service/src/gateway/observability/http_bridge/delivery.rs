use serde_json::Value;
use std::io::Read as _;
use std::sync::{Arc, Mutex};
use tiny_http::{Header, Request, Response, StatusCode};

use crate::gateway::upstream::GatewayStreamResponse;

use super::super::{GeminiStreamOutputMode, ResponseAdapter, ToolNameRestoreMap};
use super::body_conversion::{
    chat_completion_body_to_single_sse, compatibility_stream_content_type,
    convert_chat_completions_body_to_compact, convert_error_body_for_adapter,
    convert_responses_body_to_chat_completions, convert_success_body_for_adapter,
    gemini_cli_wrap_response_envelope, images_response_body_to_sse,
    merge_usage_from_body_without_output_text,
};
use super::compact_delivery::{
    respond_compact_success_body, respond_invalid_compact_non_success_body,
    respond_invalid_compact_success_body, respond_normalized_passthrough_non_success_body,
};
use super::compact_errors::{
    compact_non_success_body_should_be_normalized, compact_success_body_is_valid,
    extract_error_hint_from_body_or_headers, non_success_body_should_be_normalized,
    with_upstream_debug_suffix,
};
use super::manual_chunked::respond_streaming_chunked;
#[cfg(test)]
use super::manual_chunked::write_streaming_chunked_response;
use super::metadata::{
    copy_upstream_response_headers, log_bridge_stream_diagnostics,
    terminal_bridge_result_with_debug_meta, upstream_response_metadata, with_bridge_debug_meta,
};
use super::reasoning_guard::{
    decide, reset as reset_reasoning_guard, ReasoningGuardDecision, ReasoningGuardResponseMode,
    ReasoningGuardScope,
};
use super::response_helpers::{
    extract_error_message_from_json_bytes, force_openai_responses_stream_content_type,
    replace_content_type_header, respond_json_bytes,
};
use super::{
    collect_non_stream_json_from_sse_bytes, extract_error_hint_from_body, looks_like_sse_payload,
    parse_usage_from_json, reasoning_guard_error, reasoning_guard_target_token, usage_has_signal,
    AnthropicSseReader, ChatCompletionsFromResponsesSseReader, GeminiSseReader,
    ImagesFromResponsesSseReader, ImagesResponseFormat, OpenAIResponsesPassthroughSseReader,
    PassthroughSseCollector, PassthroughSseProtocol, PassthroughSseUsageReader,
    ReasoningGuardBridgeAction, ResponsesFromAnthropicSseReader, SseKeepAliveFrame,
    UpstreamResponseBridgeResult, UpstreamResponseUsage,
};

const REASONING_GUARD_STREAM_BUFFER_LIMIT_BYTES: usize = 16 * 1024 * 1024;

/// 函数 `is_compact_request_path`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - path: 参数 path
///
/// # 返回
/// 返回函数执行结果
fn is_compact_request_path(path: &str) -> bool {
    path == "/v1/responses/compact" || path.starts_with("/v1/responses/compact?")
}

#[cfg(test)]
fn response_adapter_uses_manual_chunked_streaming(response_adapter: ResponseAdapter) -> bool {
    matches!(
        response_adapter,
        ResponseAdapter::ResponsesFromAnthropicMessages
    )
}

/// 函数 `should_suppress_deactivation_delivery`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - upstream_error_hint: 参数 upstream_error_hint
/// - allow_failover_for_deactivation: 参数 allow_failover_for_deactivation
///
/// # 返回
/// 返回函数执行结果
fn should_suppress_deactivation_delivery(
    upstream_error_hint: Option<&str>,
    allow_failover_for_deactivation: bool,
) -> bool {
    allow_failover_for_deactivation
        && upstream_error_hint.is_some_and(|message| {
            crate::account_status::deactivation_reason_from_message(message).is_some()
        })
}

#[derive(Clone, Copy)]
struct UpstreamDebugMetaRefs<'a> {
    request_id: &'a Option<String>,
    cf_ray: &'a Option<String>,
    auth_error: &'a Option<String>,
    identity_error_code: &'a Option<String>,
    content_type: &'a Option<String>,
}

struct ReasoningGuardOutcome {
    message: String,
    action: ReasoningGuardBridgeAction,
    is_stream: bool,
    target_token: i64,
}

fn reasoning_guard_body(message: &str) -> Vec<u8> {
    serde_json::json!({
        "error": {
            "message": message,
            "type": "codexmanager_reasoning_guard",
            "code": "reasoning_guard_triggered"
        }
    })
    .to_string()
    .into_bytes()
}

fn reasoning_guard_bridge_result(
    usage: UpstreamResponseUsage,
    message: String,
    delivered_status_code: Option<u16>,
    pending_failover_request: Option<Request>,
    action: ReasoningGuardBridgeAction,
    target_token: Option<i64>,
    continuation_reasoning_items: Vec<Value>,
    meta: UpstreamDebugMetaRefs<'_>,
) -> UpstreamResponseBridgeResult {
    with_bridge_debug_meta(
        UpstreamResponseBridgeResult {
            usage,
            stream_terminal_seen: true,
            upstream_error_hint: Some(message),
            delivered_status_code,
            pending_failover_request,
            reasoning_guard_action: Some(action),
            reasoning_guard_target_token: target_token,
            continuation_reasoning_items,
            ..UpstreamResponseBridgeResult::default()
        },
        meta.request_id,
        meta.cf_ray,
        meta.auth_error,
        meta.identity_error_code,
        meta.content_type,
        None,
    )
}

fn upstream_capacity_bridge_result(
    request: Request,
    usage: UpstreamResponseUsage,
    message: String,
    meta: UpstreamDebugMetaRefs<'_>,
) -> UpstreamResponseBridgeResult {
    with_bridge_debug_meta(
        UpstreamResponseBridgeResult {
            usage,
            stream_terminal_seen: true,
            upstream_error_hint: Some(message),
            pending_failover_request: Some(request),
            ..UpstreamResponseBridgeResult::default()
        },
        meta.request_id,
        meta.cf_ray,
        meta.auth_error,
        meta.identity_error_code,
        meta.content_type,
        None,
    )
}

fn respond_reasoning_guard(
    request: Request,
    headers: Vec<Header>,
    usage: UpstreamResponseUsage,
    message: String,
    target_token: i64,
    meta: UpstreamDebugMetaRefs<'_>,
) -> UpstreamResponseBridgeResult {
    let status = StatusCode(502);
    let delivery_error =
        respond_json_bytes(request, status, headers, reasoning_guard_body(&message));
    let mut result = reasoning_guard_bridge_result(
        usage,
        message,
        Some(502),
        None,
        ReasoningGuardBridgeAction::Block,
        Some(target_token),
        Vec::new(),
        meta,
    );
    result.delivery_error = delivery_error;
    result
}

fn should_apply_reasoning_guard_to_path(request_path: &str) -> bool {
    let path = request_path.split('?').next().unwrap_or(request_path);
    matches!(
        path,
        "/responses" | "/chat/completions" | "/v1/responses" | "/v1/chat/completions"
    )
}

fn is_responses_request_path(request_path: &str) -> bool {
    let path = request_path.split('?').next().unwrap_or(request_path);
    matches!(path, "/responses" | "/v1/responses")
}

fn reasoning_guard_outcome(
    usage: &UpstreamResponseUsage,
    guard_scope: &ReasoningGuardScope,
    mode: ReasoningGuardResponseMode,
    retry_budget_remaining: usize,
) -> Option<ReasoningGuardOutcome> {
    let Some(target_token) = reasoning_guard_target_token(usage) else {
        reset_reasoning_guard(guard_scope);
        return None;
    };
    let message = format!("upstream reasoning guard triggered: reasoning_tokens={target_token}");
    let is_stream = matches!(mode, ReasoningGuardResponseMode::Stream);
    crate::gateway::record_gateway_reasoning_guard_match(is_stream);
    match decide(guard_scope, mode, retry_budget_remaining) {
        ReasoningGuardDecision::BypassDisabled => None,
        ReasoningGuardDecision::ObserveOnly { .. } => Some(ReasoningGuardOutcome {
            message,
            action: ReasoningGuardBridgeAction::ObserveOnly,
            is_stream,
            target_token,
        }),
        ReasoningGuardDecision::BypassAfterConsecutive { .. } => Some(ReasoningGuardOutcome {
            message,
            action: ReasoningGuardBridgeAction::BypassAfterConsecutive,
            is_stream,
            target_token,
        }),
        ReasoningGuardDecision::InternalRetry { .. } => Some(ReasoningGuardOutcome {
            message,
            action: ReasoningGuardBridgeAction::InternalRetry,
            is_stream,
            target_token,
        }),
        ReasoningGuardDecision::Block { .. } => Some(ReasoningGuardOutcome {
            message,
            action: ReasoningGuardBridgeAction::Block,
            is_stream,
            target_token,
        }),
    }
}

fn handle_reasoning_guard_outcome(
    request: Request,
    headers: Vec<Header>,
    usage: UpstreamResponseUsage,
    outcome: ReasoningGuardOutcome,
    continuation_reasoning_items: Vec<Value>,
    meta: UpstreamDebugMetaRefs<'_>,
) -> Result<UpstreamResponseBridgeResult, (Request, Vec<Header>)> {
    match outcome.action {
        ReasoningGuardBridgeAction::ObserveOnly
        | ReasoningGuardBridgeAction::BypassAfterConsecutive => Err((request, headers)),
        ReasoningGuardBridgeAction::InternalRetry
        | ReasoningGuardBridgeAction::ContinuationRecovery => Ok(reasoning_guard_bridge_result(
            usage,
            outcome.message,
            None,
            Some(request),
            outcome.action,
            Some(outcome.target_token),
            continuation_reasoning_items,
            meta,
        )),
        ReasoningGuardBridgeAction::Block => {
            crate::gateway::record_gateway_reasoning_guard_block(outcome.is_stream);
            Ok(respond_reasoning_guard(
                request,
                headers,
                usage,
                outcome.message,
                outcome.target_token,
                meta,
            ))
        }
    }
}

fn maybe_handle_reasoning_guard(
    request: Request,
    headers: Vec<Header>,
    usage: UpstreamResponseUsage,
    guard_scope: &ReasoningGuardScope,
    mode: ReasoningGuardResponseMode,
    retry_budget_remaining: usize,
    meta: UpstreamDebugMetaRefs<'_>,
) -> Result<UpstreamResponseBridgeResult, (Request, Vec<Header>)> {
    if let Some(outcome) =
        reasoning_guard_outcome(&usage, guard_scope, mode, retry_budget_remaining)
    {
        handle_reasoning_guard_outcome(request, headers, usage, outcome, Vec::new(), meta)
    } else {
        Err((request, headers))
    }
}

fn respond_usage_collector_stream(
    request: Request,
    status: StatusCode,
    headers: Vec<Header>,
    response_body: Box<dyn std::io::Read + Send>,
    usage_collector: Arc<Mutex<UpstreamResponseUsage>>,
    meta: UpstreamDebugMetaRefs<'_>,
) -> UpstreamResponseBridgeResult {
    let delivery_error = respond_streaming_chunked(request, status, headers, response_body)
        .err()
        .map(|err| err.to_string());
    let usage = usage_collector
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    terminal_bridge_result_with_debug_meta(
        usage,
        delivery_error,
        None,
        meta.request_id,
        meta.cf_ray,
        meta.auth_error,
        meta.identity_error_code,
        meta.content_type,
    )
}

fn respond_passthrough_collector_stream_strict_guard(
    mut request: Request,
    request_path: &str,
    status: StatusCode,
    mut headers: Vec<Header>,
    mut response_body: Box<dyn std::io::Read + Send>,
    usage_collector: Arc<Mutex<PassthroughSseCollector>>,
    _allow_failover: bool,
    guard_scope: &ReasoningGuardScope,
    reasoning_guard_retry_budget_remaining: usize,
    meta: UpstreamDebugMetaRefs<'_>,
) -> UpstreamResponseBridgeResult {
    let mut body = Vec::new();
    let mut read_error = None;
    let mut buf = [0_u8; 8192];
    loop {
        match response_body.read(&mut buf) {
            Ok(0) => break,
            Ok(read) => {
                if body.len().saturating_add(read) > REASONING_GUARD_STREAM_BUFFER_LIMIT_BYTES {
                    read_error = Some(format!(
                        "upstream response too large for reasoning guard scan: max {} bytes",
                        REASONING_GUARD_STREAM_BUFFER_LIMIT_BYTES
                    ));
                    break;
                }
                body.extend_from_slice(&buf[..read]);
            }
            Err(err) => {
                read_error = Some(err.to_string());
                break;
            }
        }
    }
    let collector = usage_collector
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();

    if read_error.is_none() {
        if let Some(mut outcome) = reasoning_guard_outcome(
            &collector.usage,
            guard_scope,
            ReasoningGuardResponseMode::Stream,
            reasoning_guard_retry_budget_remaining,
        ) {
            if outcome.action == ReasoningGuardBridgeAction::InternalRetry
                && crate::gateway::reasoning_guard_uses_continuation_recovery()
                && is_responses_request_path(request_path)
                && !collector.continuation_reasoning_items.is_empty()
                && reasoning_guard_retry_budget_remaining > 0
            {
                outcome.action = ReasoningGuardBridgeAction::ContinuationRecovery;
            }
            match handle_reasoning_guard_outcome(
                request,
                headers,
                collector.usage.clone(),
                outcome,
                collector.continuation_reasoning_items.clone(),
                meta,
            ) {
                Ok(result) => return result,
                Err((returned_request, returned_headers)) => {
                    request = returned_request;
                    headers = returned_headers;
                }
            }
        } else if reasoning_guard_error(&collector.usage).is_none() {
            reset_reasoning_guard(guard_scope);
        }
    }

    let delivery_error = if read_error.is_none() {
        respond_streaming_chunked(request, status, headers, std::io::Cursor::new(body))
            .err()
            .map(|err| err.to_string())
    } else {
        read_error
    };
    with_bridge_debug_meta(
        UpstreamResponseBridgeResult {
            usage: collector.usage,
            stream_terminal_seen: collector.saw_terminal,
            stream_terminal_error: collector.terminal_error,
            delivery_error,
            upstream_error_hint: collector.upstream_error_hint,
            delivered_status_code: None,
            upstream_request_id: None,
            upstream_cf_ray: None,
            upstream_auth_error: None,
            upstream_identity_error_code: None,
            upstream_content_type: None,
            last_sse_event_type: collector.last_event_type.clone(),
            pending_failover_request: None,
            reasoning_guard_action: None,
            reasoning_guard_target_token: None,
            continuation_reasoning_items: Vec::new(),
        },
        meta.request_id,
        meta.cf_ray,
        meta.auth_error,
        meta.identity_error_code,
        meta.content_type,
        collector.last_event_type,
    )
}

fn respond_passthrough_collector_stream(
    request: Request,
    status: StatusCode,
    headers: Vec<Header>,
    response_body: Box<dyn std::io::Read + Send>,
    usage_collector: Arc<Mutex<PassthroughSseCollector>>,
    meta: UpstreamDebugMetaRefs<'_>,
) -> UpstreamResponseBridgeResult {
    let delivery_error = respond_streaming_chunked(request, status, headers, response_body)
        .err()
        .map(|err| err.to_string());
    let collector = usage_collector
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    with_bridge_debug_meta(
        UpstreamResponseBridgeResult {
            usage: collector.usage,
            stream_terminal_seen: collector.saw_terminal,
            stream_terminal_error: collector.terminal_error,
            delivery_error,
            upstream_error_hint: collector.upstream_error_hint,
            delivered_status_code: None,
            upstream_request_id: None,
            upstream_cf_ray: None,
            upstream_auth_error: None,
            upstream_identity_error_code: None,
            upstream_content_type: None,
            last_sse_event_type: collector.last_event_type,
            pending_failover_request: None,
            reasoning_guard_action: None,
            reasoning_guard_target_token: None,
            continuation_reasoning_items: Vec::new(),
        },
        meta.request_id,
        meta.cf_ray,
        meta.auth_error,
        meta.identity_error_code,
        meta.content_type,
        None,
    )
}

/// 函数 `respond_with_upstream`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn respond_with_upstream(
    mut request: Request,
    upstream: reqwest::blocking::Response,
    _inflight_guard: super::super::AccountInFlightGuard,
    response_adapter: ResponseAdapter,
    passthrough_sse_protocol: Option<PassthroughSseProtocol>,
    gemini_stream_output_mode: Option<GeminiStreamOutputMode>,
    request_path: &str,
    tool_name_restore_map: Option<&ToolNameRestoreMap>,
    is_stream: bool,
    allow_failover_for_deactivation: bool,
    trace_id: Option<&str>,
    fallback_model: Option<&str>,
    reasoning_guard_source_id: Option<&str>,
    reasoning_guard_retry_budget_remaining: usize,
    request_started_at: std::time::Instant,
) -> Result<UpstreamResponseBridgeResult, String> {
    let reasoning_guard_scope =
        ReasoningGuardScope::new(reasoning_guard_source_id, fallback_model, request_path);
    let keepalive_frame = resolve_stream_keepalive_frame(response_adapter, request_path);
    let passthrough_sse_protocol =
        passthrough_sse_protocol.unwrap_or(PassthroughSseProtocol::Generic);
    let upstream_meta = upstream_response_metadata(upstream.headers());
    let upstream_request_id = upstream_meta.request_id;
    let upstream_cf_ray = upstream_meta.cf_ray;
    let upstream_auth_error = upstream_meta.auth_error;
    let upstream_identity_error_code = upstream_meta.identity_error_code;
    let upstream_content_type = upstream_meta.content_type;
    let is_sse = upstream_meta.is_sse;
    let is_json = upstream_meta.is_json;
    if response_adapter != ResponseAdapter::Passthrough {
        let status = StatusCode(upstream.status().as_u16());
        let mut headers = copy_upstream_response_headers(upstream.headers(), trace_id);

        if !is_stream {
            let upstream_body = upstream
                .bytes()
                .map_err(|err| format!("read upstream body failed: {err}"))?;
            let detected_sse =
                is_sse || (!is_json && looks_like_sse_payload(upstream_body.as_ref()));
            let (body, usage) = if detected_sse {
                let (synthesized, mut usage) =
                    collect_non_stream_json_from_sse_bytes(upstream_body.as_ref());
                let body = synthesized.unwrap_or_else(|| upstream_body.to_vec());
                merge_usage_from_body_without_output_text(&mut usage, &body);
                (body, usage)
            } else {
                let usage = serde_json::from_slice::<Value>(upstream_body.as_ref())
                    .ok()
                    .map(|value| parse_usage_from_json(&value))
                    .unwrap_or_default();
                (upstream_body.to_vec(), usage)
            };
            if should_apply_reasoning_guard_to_path(request_path) {
                match maybe_handle_reasoning_guard(
                    request,
                    headers,
                    usage.clone(),
                    &reasoning_guard_scope,
                    ReasoningGuardResponseMode::NonStream,
                    reasoning_guard_retry_budget_remaining,
                    UpstreamDebugMetaRefs {
                        request_id: &upstream_request_id,
                        cf_ray: &upstream_cf_ray,
                        auth_error: &upstream_auth_error,
                        identity_error_code: &upstream_identity_error_code,
                        content_type: &upstream_content_type,
                    },
                ) {
                    Ok(result) => return Ok(result),
                    Err((returned_request, returned_headers)) => {
                        request = returned_request;
                        headers = returned_headers;
                    }
                }
            }
            let response_body = if status.0 >= 400 {
                let message = with_upstream_debug_suffix(
                    extract_error_hint_from_body_or_headers(
                        status.0,
                        upstream_content_type.as_deref(),
                        &body,
                        upstream_cf_ray.as_deref(),
                        upstream_auth_error.as_deref(),
                        upstream_identity_error_code.as_deref(),
                    )
                    .or_else(|| extract_error_message_from_json_bytes(&body)),
                    None,
                    upstream_request_id.as_deref(),
                    upstream_cf_ray.as_deref(),
                    upstream_auth_error.as_deref(),
                    upstream_identity_error_code.as_deref(),
                )
                .unwrap_or_else(|| "upstream compatibility bridge failed".to_string());
                if crate::gateway::is_selected_model_capacity_error(&message) {
                    return Ok(upstream_capacity_bridge_result(
                        request,
                        usage,
                        message,
                        UpstreamDebugMetaRefs {
                            request_id: &upstream_request_id,
                            cf_ray: &upstream_cf_ray,
                            auth_error: &upstream_auth_error,
                            identity_error_code: &upstream_identity_error_code,
                            content_type: &upstream_content_type,
                        },
                    ));
                }
                convert_error_body_for_adapter(response_adapter, &message)
            } else {
                convert_success_body_for_adapter(
                    response_adapter,
                    &body,
                    request_path,
                    tool_name_restore_map,
                )
                .unwrap_or_else(|| body.clone())
            };
            let delivery_error = respond_json_bytes(request, status, headers, response_body);
            return Ok(terminal_bridge_result_with_debug_meta(
                usage,
                delivery_error,
                None,
                &upstream_request_id,
                &upstream_cf_ray,
                &upstream_auth_error,
                &upstream_identity_error_code,
                &upstream_content_type,
            ));
        }

        if status.0 >= 400 && !is_sse {
            let upstream_body = upstream
                .bytes()
                .map_err(|err| format!("read upstream body failed: {err}"))?;
            let message = with_upstream_debug_suffix(
                extract_error_hint_from_body_or_headers(
                    status.0,
                    upstream_content_type.as_deref(),
                    upstream_body.as_ref(),
                    upstream_cf_ray.as_deref(),
                    upstream_auth_error.as_deref(),
                    upstream_identity_error_code.as_deref(),
                )
                .or_else(|| extract_error_message_from_json_bytes(upstream_body.as_ref())),
                None,
                upstream_request_id.as_deref(),
                upstream_cf_ray.as_deref(),
                upstream_auth_error.as_deref(),
                upstream_identity_error_code.as_deref(),
            )
            .unwrap_or_else(|| "upstream compatibility bridge failed".to_string());
            if crate::gateway::is_selected_model_capacity_error(&message) {
                return Ok(upstream_capacity_bridge_result(
                    request,
                    UpstreamResponseUsage::default(),
                    message,
                    UpstreamDebugMetaRefs {
                        request_id: &upstream_request_id,
                        cf_ray: &upstream_cf_ray,
                        auth_error: &upstream_auth_error,
                        identity_error_code: &upstream_identity_error_code,
                        content_type: &upstream_content_type,
                    },
                ));
            }
            let response_body = convert_error_body_for_adapter(response_adapter, &message);
            let delivery_error = respond_json_bytes(request, status, headers, response_body);
            return Ok(terminal_bridge_result_with_debug_meta(
                UpstreamResponseUsage::default(),
                delivery_error,
                Some(message),
                &upstream_request_id,
                &upstream_cf_ray,
                &upstream_auth_error,
                &upstream_identity_error_code,
                &upstream_content_type,
            ));
        }

        replace_content_type_header(
            &mut headers,
            compatibility_stream_content_type(response_adapter, gemini_stream_output_mode),
        );
        match response_adapter {
            ResponseAdapter::AnthropicMessagesFromResponses => {
                let usage_collector = Arc::new(Mutex::new(UpstreamResponseUsage::default()));
                let response_body: Box<dyn std::io::Read + Send> =
                    Box::new(AnthropicSseReader::new(
                        upstream,
                        Arc::clone(&usage_collector),
                        fallback_model,
                        tool_name_restore_map.cloned(),
                        request_started_at,
                    ));
                return Ok(respond_usage_collector_stream(
                    request,
                    status,
                    headers,
                    response_body,
                    usage_collector,
                    UpstreamDebugMetaRefs {
                        request_id: &upstream_request_id,
                        cf_ray: &upstream_cf_ray,
                        auth_error: &upstream_auth_error,
                        identity_error_code: &upstream_identity_error_code,
                        content_type: &upstream_content_type,
                    },
                ));
            }
            ResponseAdapter::ResponsesFromAnthropicMessages => {
                let usage_collector = Arc::new(Mutex::new(UpstreamResponseUsage::default()));
                let response_body: Box<dyn std::io::Read + Send> =
                    Box::new(ResponsesFromAnthropicSseReader::new(
                        upstream,
                        Arc::clone(&usage_collector),
                        fallback_model,
                        request_started_at,
                    ));
                return Ok(respond_usage_collector_stream(
                    request,
                    status,
                    headers,
                    response_body,
                    usage_collector,
                    UpstreamDebugMetaRefs {
                        request_id: &upstream_request_id,
                        cf_ray: &upstream_cf_ray,
                        auth_error: &upstream_auth_error,
                        identity_error_code: &upstream_identity_error_code,
                        content_type: &upstream_content_type,
                    },
                ));
            }
            ResponseAdapter::ChatCompletionsFromResponses => {
                let usage_collector = Arc::new(Mutex::new(PassthroughSseCollector::default()));
                let response_body: Box<dyn std::io::Read + Send> =
                    Box::new(ChatCompletionsFromResponsesSseReader::new(
                        upstream,
                        Arc::clone(&usage_collector),
                        request_started_at,
                    ));
                return Ok(respond_passthrough_collector_stream(
                    request,
                    status,
                    headers,
                    response_body,
                    usage_collector,
                    UpstreamDebugMetaRefs {
                        request_id: &upstream_request_id,
                        cf_ray: &upstream_cf_ray,
                        auth_error: &upstream_auth_error,
                        identity_error_code: &upstream_identity_error_code,
                        content_type: &upstream_content_type,
                    },
                ));
            }
            ResponseAdapter::CompactFromChatCompletions => unreachable!(),
            ResponseAdapter::ImagesB64JsonFromResponses
            | ResponseAdapter::ImagesUrlFromResponses => {
                let response_format = if response_adapter == ResponseAdapter::ImagesUrlFromResponses
                {
                    ImagesResponseFormat::Url
                } else {
                    ImagesResponseFormat::B64Json
                };
                let usage_collector = Arc::new(Mutex::new(PassthroughSseCollector::default()));
                let response_body: Box<dyn std::io::Read + Send> =
                    Box::new(ImagesFromResponsesSseReader::new(
                        upstream,
                        Arc::clone(&usage_collector),
                        request_started_at,
                        response_format,
                    ));
                return Ok(respond_passthrough_collector_stream(
                    request,
                    status,
                    headers,
                    response_body,
                    usage_collector,
                    UpstreamDebugMetaRefs {
                        request_id: &upstream_request_id,
                        cf_ray: &upstream_cf_ray,
                        auth_error: &upstream_auth_error,
                        identity_error_code: &upstream_identity_error_code,
                        content_type: &upstream_content_type,
                    },
                ));
            }
            ResponseAdapter::GeminiJson | ResponseAdapter::GeminiCliJson => unreachable!(),
            ResponseAdapter::GeminiSse | ResponseAdapter::GeminiCliSse => {
                let usage_collector = Arc::new(Mutex::new(PassthroughSseCollector::default()));
                let response_body: Box<dyn std::io::Read + Send> = Box::new(GeminiSseReader::new(
                    upstream,
                    Arc::clone(&usage_collector),
                    tool_name_restore_map.cloned(),
                    gemini_stream_output_mode.unwrap_or(GeminiStreamOutputMode::Sse),
                    gemini_cli_wrap_response_envelope(response_adapter),
                    request_started_at,
                ));
                return Ok(respond_passthrough_collector_stream(
                    request,
                    status,
                    headers,
                    response_body,
                    usage_collector,
                    UpstreamDebugMetaRefs {
                        request_id: &upstream_request_id,
                        cf_ray: &upstream_cf_ray,
                        auth_error: &upstream_auth_error,
                        identity_error_code: &upstream_identity_error_code,
                        content_type: &upstream_content_type,
                    },
                ));
            }
            ResponseAdapter::Passthrough => {}
        }
    }
    match response_adapter {
        ResponseAdapter::Passthrough => {
            let status = StatusCode(upstream.status().as_u16());
            let mut headers = copy_upstream_response_headers(upstream.headers(), trace_id);
            if !is_stream {
                let upstream_body = upstream
                    .bytes()
                    .map_err(|err| format!("read upstream body failed: {err}"))?;
                let detected_sse =
                    is_sse || (!is_json && looks_like_sse_payload(upstream_body.as_ref()));
                let is_compact_request = is_compact_request_path(request_path);
                if detected_sse {
                    let (synthesized_body, mut usage) =
                        collect_non_stream_json_from_sse_bytes(upstream_body.as_ref());
                    let synthesized_response = synthesized_body.is_some();
                    let body = synthesized_body.unwrap_or_else(|| upstream_body.to_vec());
                    merge_usage_from_body_without_output_text(&mut usage, &body);
                    let upstream_error_hint = with_upstream_debug_suffix(
                        extract_error_hint_from_body_or_headers(
                            status.0,
                            upstream_content_type.as_deref(),
                            &body,
                            upstream_cf_ray.as_deref(),
                            upstream_auth_error.as_deref(),
                            upstream_identity_error_code.as_deref(),
                        )
                        .or_else(|| extract_error_message_from_json_bytes(&body)),
                        None,
                        upstream_request_id.as_deref(),
                        upstream_cf_ray.as_deref(),
                        upstream_auth_error.as_deref(),
                        upstream_identity_error_code.as_deref(),
                    );
                    if should_apply_reasoning_guard_to_path(request_path) {
                        match maybe_handle_reasoning_guard(
                            request,
                            headers,
                            usage.clone(),
                            &reasoning_guard_scope,
                            ReasoningGuardResponseMode::NonStream,
                            reasoning_guard_retry_budget_remaining,
                            UpstreamDebugMetaRefs {
                                request_id: &upstream_request_id,
                                cf_ray: &upstream_cf_ray,
                                auth_error: &upstream_auth_error,
                                identity_error_code: &upstream_identity_error_code,
                                content_type: &upstream_content_type,
                            },
                        ) {
                            Ok(result) => return Ok(result),
                            Err((returned_request, returned_headers)) => {
                                request = returned_request;
                                headers = returned_headers;
                            }
                        }
                    }
                    if should_suppress_deactivation_delivery(
                        upstream_error_hint.as_deref(),
                        allow_failover_for_deactivation,
                    ) {
                        return Ok(terminal_bridge_result_with_debug_meta(
                            usage,
                            None,
                            upstream_error_hint,
                            &upstream_request_id,
                            &upstream_cf_ray,
                            &upstream_auth_error,
                            &upstream_identity_error_code,
                            &upstream_content_type,
                        ));
                    }
                    if synthesized_response {
                        replace_content_type_header(&mut headers, "application/json");
                    }
                    if upstream_error_hint
                        .as_deref()
                        .is_some_and(crate::gateway::is_selected_model_capacity_error)
                    {
                        return Ok(upstream_capacity_bridge_result(
                            request,
                            usage,
                            upstream_error_hint.expect("checked capacity error hint"),
                            UpstreamDebugMetaRefs {
                                request_id: &upstream_request_id,
                                cf_ray: &upstream_cf_ray,
                                auth_error: &upstream_auth_error,
                                identity_error_code: &upstream_identity_error_code,
                                content_type: &upstream_content_type,
                            },
                        ));
                    }
                    if status.0 < 400
                        && is_compact_request
                        && !compact_success_body_is_valid(body.as_ref())
                    {
                        return Ok(respond_invalid_compact_success_body(
                            request,
                            usage,
                            body.as_ref(),
                            upstream_request_id.as_deref(),
                            upstream_cf_ray.as_deref(),
                            upstream_auth_error.as_deref(),
                            upstream_identity_error_code.as_deref(),
                            trace_id,
                        ));
                    }
                    if is_compact_request
                        && compact_non_success_body_should_be_normalized(
                            status.0,
                            upstream_content_type.as_deref(),
                            body.as_ref(),
                            upstream_auth_error.as_deref(),
                            upstream_identity_error_code.as_deref(),
                        )
                    {
                        return Ok(respond_invalid_compact_non_success_body(
                            request,
                            status.0,
                            usage,
                            body.as_ref(),
                            upstream_content_type.as_deref(),
                            upstream_request_id.as_deref(),
                            upstream_cf_ray.as_deref(),
                            upstream_auth_error.as_deref(),
                            upstream_identity_error_code.as_deref(),
                            trace_id,
                        ));
                    }
                    if status.0 >= 400
                        && non_success_body_should_be_normalized(
                            status.0,
                            upstream_content_type.as_deref(),
                            body.as_ref(),
                            upstream_auth_error.as_deref(),
                            upstream_identity_error_code.as_deref(),
                        )
                    {
                        return Ok(respond_normalized_passthrough_non_success_body(
                            request,
                            usage,
                            body.as_ref(),
                            upstream_content_type.as_deref(),
                            upstream_request_id.as_deref(),
                            upstream_cf_ray.as_deref(),
                            upstream_auth_error.as_deref(),
                            upstream_identity_error_code.as_deref(),
                            trace_id,
                        ));
                    }
                    let len = Some(body.len());
                    let response =
                        Response::new(status, headers, std::io::Cursor::new(body), len, None);
                    let delivery_error = request.respond(response).err().map(|err| err.to_string());
                    return Ok(terminal_bridge_result_with_debug_meta(
                        usage,
                        delivery_error,
                        upstream_error_hint,
                        &upstream_request_id,
                        &upstream_cf_ray,
                        &upstream_auth_error,
                        &upstream_identity_error_code,
                        &upstream_content_type,
                    ));
                }

                let (_, sse_usage) = collect_non_stream_json_from_sse_bytes(upstream_body.as_ref());
                let usage = if is_json {
                    serde_json::from_slice::<Value>(upstream_body.as_ref())
                        .ok()
                        .map(|value| parse_usage_from_json(&value))
                        .unwrap_or_default()
                } else if usage_has_signal(&sse_usage) {
                    sse_usage
                } else {
                    UpstreamResponseUsage::default()
                };
                if should_apply_reasoning_guard_to_path(request_path) {
                    match maybe_handle_reasoning_guard(
                        request,
                        headers,
                        usage.clone(),
                        &reasoning_guard_scope,
                        ReasoningGuardResponseMode::NonStream,
                        reasoning_guard_retry_budget_remaining,
                        UpstreamDebugMetaRefs {
                            request_id: &upstream_request_id,
                            cf_ray: &upstream_cf_ray,
                            auth_error: &upstream_auth_error,
                            identity_error_code: &upstream_identity_error_code,
                            content_type: &upstream_content_type,
                        },
                    ) {
                        Ok(result) => return Ok(result),
                        Err((returned_request, returned_headers)) => {
                            request = returned_request;
                            headers = returned_headers;
                        }
                    }
                }
                let upstream_error_hint = with_upstream_debug_suffix(
                    extract_error_hint_from_body_or_headers(
                        status.0,
                        upstream_content_type.as_deref(),
                        upstream_body.as_ref(),
                        upstream_cf_ray.as_deref(),
                        upstream_auth_error.as_deref(),
                        upstream_identity_error_code.as_deref(),
                    )
                    .or_else(|| extract_error_message_from_json_bytes(upstream_body.as_ref())),
                    None,
                    upstream_request_id.as_deref(),
                    upstream_cf_ray.as_deref(),
                    upstream_auth_error.as_deref(),
                    upstream_identity_error_code.as_deref(),
                );
                if status.0 < 400
                    && is_compact_request
                    && !compact_success_body_is_valid(upstream_body.as_ref())
                {
                    return Ok(respond_invalid_compact_success_body(
                        request,
                        usage,
                        upstream_body.as_ref(),
                        upstream_request_id.as_deref(),
                        upstream_cf_ray.as_deref(),
                        upstream_auth_error.as_deref(),
                        upstream_identity_error_code.as_deref(),
                        trace_id,
                    ));
                }
                if upstream_error_hint
                    .as_deref()
                    .is_some_and(crate::gateway::is_selected_model_capacity_error)
                {
                    return Ok(upstream_capacity_bridge_result(
                        request,
                        usage,
                        upstream_error_hint.expect("checked capacity error hint"),
                        UpstreamDebugMetaRefs {
                            request_id: &upstream_request_id,
                            cf_ray: &upstream_cf_ray,
                            auth_error: &upstream_auth_error,
                            identity_error_code: &upstream_identity_error_code,
                            content_type: &upstream_content_type,
                        },
                    ));
                }
                if is_compact_request
                    && compact_non_success_body_should_be_normalized(
                        status.0,
                        upstream_content_type.as_deref(),
                        upstream_body.as_ref(),
                        upstream_auth_error.as_deref(),
                        upstream_identity_error_code.as_deref(),
                    )
                {
                    return Ok(respond_invalid_compact_non_success_body(
                        request,
                        status.0,
                        usage,
                        upstream_body.as_ref(),
                        upstream_content_type.as_deref(),
                        upstream_request_id.as_deref(),
                        upstream_cf_ray.as_deref(),
                        upstream_auth_error.as_deref(),
                        upstream_identity_error_code.as_deref(),
                        trace_id,
                    ));
                }
                if status.0 >= 400
                    && non_success_body_should_be_normalized(
                        status.0,
                        upstream_content_type.as_deref(),
                        upstream_body.as_ref(),
                        upstream_auth_error.as_deref(),
                        upstream_identity_error_code.as_deref(),
                    )
                {
                    return Ok(respond_normalized_passthrough_non_success_body(
                        request,
                        usage,
                        upstream_body.as_ref(),
                        upstream_content_type.as_deref(),
                        upstream_request_id.as_deref(),
                        upstream_cf_ray.as_deref(),
                        upstream_auth_error.as_deref(),
                        upstream_identity_error_code.as_deref(),
                        trace_id,
                    ));
                }
                if should_suppress_deactivation_delivery(
                    upstream_error_hint.as_deref(),
                    allow_failover_for_deactivation,
                ) {
                    return Ok(terminal_bridge_result_with_debug_meta(
                        usage,
                        None,
                        upstream_error_hint,
                        &upstream_request_id,
                        &upstream_cf_ray,
                        &upstream_auth_error,
                        &upstream_identity_error_code,
                        &upstream_content_type,
                    ));
                }
                let len = Some(upstream_body.len());
                let response = Response::new(
                    status,
                    headers,
                    std::io::Cursor::new(upstream_body.to_vec()),
                    len,
                    None,
                );
                let delivery_error = request.respond(response).err().map(|err| err.to_string());
                return Ok(terminal_bridge_result_with_debug_meta(
                    usage,
                    delivery_error,
                    upstream_error_hint,
                    &upstream_request_id,
                    &upstream_cf_ray,
                    &upstream_auth_error,
                    &upstream_identity_error_code,
                    &upstream_content_type,
                ));
            }
            if is_stream && !is_sse && status.0 >= 400 {
                let upstream_body = upstream
                    .bytes()
                    .map_err(|err| format!("read upstream body failed: {err}"))?;
                let usage = if is_json {
                    serde_json::from_slice::<Value>(upstream_body.as_ref())
                        .ok()
                        .map(|value| parse_usage_from_json(&value))
                        .unwrap_or_default()
                } else {
                    UpstreamResponseUsage::default()
                };
                let upstream_error_hint = with_upstream_debug_suffix(
                    extract_error_hint_from_body_or_headers(
                        status.0,
                        upstream_content_type.as_deref(),
                        upstream_body.as_ref(),
                        upstream_cf_ray.as_deref(),
                        upstream_auth_error.as_deref(),
                        upstream_identity_error_code.as_deref(),
                    )
                    .or_else(|| extract_error_message_from_json_bytes(upstream_body.as_ref())),
                    None,
                    upstream_request_id.as_deref(),
                    upstream_cf_ray.as_deref(),
                    upstream_auth_error.as_deref(),
                    upstream_identity_error_code.as_deref(),
                );
                if upstream_error_hint
                    .as_deref()
                    .is_some_and(crate::gateway::is_selected_model_capacity_error)
                {
                    return Ok(upstream_capacity_bridge_result(
                        request,
                        usage,
                        upstream_error_hint.expect("checked capacity error hint"),
                        UpstreamDebugMetaRefs {
                            request_id: &upstream_request_id,
                            cf_ray: &upstream_cf_ray,
                            auth_error: &upstream_auth_error,
                            identity_error_code: &upstream_identity_error_code,
                            content_type: &upstream_content_type,
                        },
                    ));
                }
                if non_success_body_should_be_normalized(
                    status.0,
                    upstream_content_type.as_deref(),
                    upstream_body.as_ref(),
                    upstream_auth_error.as_deref(),
                    upstream_identity_error_code.as_deref(),
                ) {
                    return Ok(respond_normalized_passthrough_non_success_body(
                        request,
                        usage,
                        upstream_body.as_ref(),
                        upstream_content_type.as_deref(),
                        upstream_request_id.as_deref(),
                        upstream_cf_ray.as_deref(),
                        upstream_auth_error.as_deref(),
                        upstream_identity_error_code.as_deref(),
                        trace_id,
                    ));
                }
                let len = Some(upstream_body.len());
                let response = Response::new(
                    status,
                    headers,
                    std::io::Cursor::new(upstream_body.to_vec()),
                    len,
                    None,
                );
                let delivery_error = request.respond(response).err().map(|err| err.to_string());
                return Ok(terminal_bridge_result_with_debug_meta(
                    usage,
                    delivery_error,
                    upstream_error_hint,
                    &upstream_request_id,
                    &upstream_cf_ray,
                    &upstream_auth_error,
                    &upstream_identity_error_code,
                    &upstream_content_type,
                ));
            }
            if is_stream && !is_sse && status.0 < 400 && is_compact_request_path(request_path) {
                let upstream_body = upstream
                    .bytes()
                    .map_err(|err| format!("read upstream body failed: {err}"))?;
                let usage = if is_json {
                    serde_json::from_slice::<Value>(upstream_body.as_ref())
                        .ok()
                        .map(|value| parse_usage_from_json(&value))
                        .unwrap_or_default()
                } else {
                    UpstreamResponseUsage::default()
                };
                return Ok(respond_compact_success_body(
                    request,
                    status,
                    headers,
                    usage,
                    upstream_body.as_ref(),
                    upstream_request_id.as_deref(),
                    upstream_cf_ray.as_deref(),
                    upstream_auth_error.as_deref(),
                    upstream_identity_error_code.as_deref(),
                    &upstream_content_type,
                    trace_id,
                ));
            }
            if is_sse || is_stream {
                let usage_collector = Arc::new(Mutex::new(PassthroughSseCollector::default()));
                let response_body: Box<dyn std::io::Read + Send> =
                    if request_path.starts_with("/v1/responses") {
                        Box::new(OpenAIResponsesPassthroughSseReader::new(
                            upstream,
                            Arc::clone(&usage_collector),
                            keepalive_frame,
                            request_started_at,
                        ))
                    } else {
                        Box::new(PassthroughSseUsageReader::new(
                            upstream,
                            Arc::clone(&usage_collector),
                            keepalive_frame,
                            passthrough_sse_protocol,
                            request_started_at,
                        ))
                    };
                force_openai_responses_stream_content_type(&mut headers, request_path, is_stream);
                if should_apply_reasoning_guard_to_path(request_path) {
                    let result = respond_passthrough_collector_stream_strict_guard(
                        request,
                        request_path,
                        status,
                        headers,
                        response_body,
                        usage_collector,
                        allow_failover_for_deactivation,
                        &reasoning_guard_scope,
                        reasoning_guard_retry_budget_remaining,
                        UpstreamDebugMetaRefs {
                            request_id: &upstream_request_id,
                            cf_ray: &upstream_cf_ray,
                            auth_error: &upstream_auth_error,
                            identity_error_code: &upstream_identity_error_code,
                            content_type: &upstream_content_type,
                        },
                    );
                    log_bridge_stream_diagnostics(response_adapter, request_path, &result);
                    return Ok(result);
                }
                let delivery_error =
                    respond_streaming_chunked(request, status, headers, response_body)
                        .err()
                        .map(|err| err.to_string());
                let collector = usage_collector
                    .lock()
                    .map(|guard| guard.clone())
                    .unwrap_or_default();
                let last_sse_event_type = collector.last_event_type.clone();
                let result = with_bridge_debug_meta(
                    UpstreamResponseBridgeResult {
                        usage: collector.usage,
                        stream_terminal_seen: collector.saw_terminal,
                        stream_terminal_error: collector.terminal_error,
                        delivery_error,
                        upstream_error_hint: with_upstream_debug_suffix(
                            collector.upstream_error_hint,
                            None,
                            upstream_request_id.as_deref(),
                            upstream_cf_ray.as_deref(),
                            upstream_auth_error.as_deref(),
                            upstream_identity_error_code.as_deref(),
                        ),
                        delivered_status_code: None,
                        upstream_request_id: None,
                        upstream_cf_ray: None,
                        upstream_auth_error: None,
                        upstream_identity_error_code: None,
                        upstream_content_type: None,
                        last_sse_event_type: None,
                        pending_failover_request: None,
                        reasoning_guard_action: None,
                        reasoning_guard_target_token: None,
                        continuation_reasoning_items: Vec::new(),
                    },
                    &upstream_request_id,
                    &upstream_cf_ray,
                    &upstream_auth_error,
                    &upstream_identity_error_code,
                    &upstream_content_type,
                    last_sse_event_type,
                );
                log_bridge_stream_diagnostics(response_adapter, request_path, &result);
                return Ok(result);
            }
            let len = upstream.content_length().map(|v| v as usize);
            let response = Response::new(status, headers, upstream, len, None);
            let delivery_error = request.respond(response).err().map(|err| err.to_string());
            Ok(terminal_bridge_result_with_debug_meta(
                UpstreamResponseUsage::default(),
                delivery_error,
                None,
                &upstream_request_id,
                &upstream_cf_ray,
                &upstream_auth_error,
                &upstream_identity_error_code,
                &upstream_content_type,
            ))
        }
        ResponseAdapter::CompactFromChatCompletions => {
            let status = StatusCode(upstream.status().as_u16());
            let mut headers = copy_upstream_response_headers(upstream.headers(), trace_id);
            let upstream_body = upstream
                .bytes()
                .map_err(|err| format!("read upstream body failed: {err}"))?;
            let usage = serde_json::from_slice::<Value>(upstream_body.as_ref())
                .ok()
                .map(|value| parse_usage_from_json(&value))
                .unwrap_or_default();
            if should_apply_reasoning_guard_to_path(request_path) {
                match maybe_handle_reasoning_guard(
                    request,
                    headers,
                    usage.clone(),
                    &reasoning_guard_scope,
                    ReasoningGuardResponseMode::NonStream,
                    reasoning_guard_retry_budget_remaining,
                    UpstreamDebugMetaRefs {
                        request_id: &upstream_request_id,
                        cf_ray: &upstream_cf_ray,
                        auth_error: &upstream_auth_error,
                        identity_error_code: &upstream_identity_error_code,
                        content_type: &upstream_content_type,
                    },
                ) {
                    Ok(result) => return Ok(result),
                    Err((returned_request, returned_headers)) => {
                        request = returned_request;
                        headers = returned_headers;
                    }
                }
            }
            let response_body = if status.0 < 400 {
                convert_chat_completions_body_to_compact(upstream_body.as_ref())
                    .unwrap_or_else(|| upstream_body.to_vec())
            } else {
                upstream_body.to_vec()
            };
            let upstream_error_hint = (status.0 >= 400)
                .then(|| {
                    with_upstream_debug_suffix(
                        extract_error_hint_from_body(status.0, upstream_body.as_ref()),
                        None,
                        upstream_request_id.as_deref(),
                        upstream_cf_ray.as_deref(),
                        upstream_auth_error.as_deref(),
                        upstream_identity_error_code.as_deref(),
                    )
                })
                .flatten();
            if upstream_error_hint
                .as_deref()
                .is_some_and(crate::gateway::is_selected_model_capacity_error)
            {
                return Ok(upstream_capacity_bridge_result(
                    request,
                    usage,
                    upstream_error_hint.expect("checked capacity error hint"),
                    UpstreamDebugMetaRefs {
                        request_id: &upstream_request_id,
                        cf_ray: &upstream_cf_ray,
                        auth_error: &upstream_auth_error,
                        identity_error_code: &upstream_identity_error_code,
                        content_type: &upstream_content_type,
                    },
                ));
            }
            let delivery_error = respond_json_bytes(request, status, headers, response_body);
            Ok(terminal_bridge_result_with_debug_meta(
                usage,
                delivery_error,
                upstream_error_hint,
                &upstream_request_id,
                &upstream_cf_ray,
                &upstream_auth_error,
                &upstream_identity_error_code,
                &upstream_content_type,
            ))
        }
        ResponseAdapter::AnthropicMessagesFromResponses
        | ResponseAdapter::ResponsesFromAnthropicMessages
        | ResponseAdapter::ChatCompletionsFromResponses
        | ResponseAdapter::ImagesB64JsonFromResponses
        | ResponseAdapter::ImagesUrlFromResponses
        | ResponseAdapter::GeminiJson
        | ResponseAdapter::GeminiCliJson
        | ResponseAdapter::GeminiSse
        | ResponseAdapter::GeminiCliSse => unreachable!(),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn respond_with_stream_upstream(
    mut request: Request,
    upstream: GatewayStreamResponse,
    _inflight_guard: super::super::AccountInFlightGuard,
    response_adapter: ResponseAdapter,
    _passthrough_sse_protocol: Option<PassthroughSseProtocol>,
    gemini_stream_output_mode: Option<GeminiStreamOutputMode>,
    request_path: &str,
    tool_name_restore_map: Option<&ToolNameRestoreMap>,
    is_stream: bool,
    _allow_failover_for_deactivation: bool,
    trace_id: Option<&str>,
    fallback_model: Option<&str>,
    reasoning_guard_source_id: Option<&str>,
    reasoning_guard_retry_budget_remaining: usize,
    request_started_at: std::time::Instant,
) -> Result<UpstreamResponseBridgeResult, String> {
    let reasoning_guard_scope =
        ReasoningGuardScope::new(reasoning_guard_source_id, fallback_model, request_path);
    let keepalive_frame = resolve_stream_keepalive_frame(response_adapter, request_path);
    let upstream_meta = upstream_response_metadata(upstream.headers());
    let upstream_request_id = upstream_meta.request_id;
    let upstream_cf_ray = upstream_meta.cf_ray;
    let upstream_auth_error = upstream_meta.auth_error;
    let upstream_identity_error_code = upstream_meta.identity_error_code;
    let upstream_content_type = upstream_meta.content_type;
    let is_sse = upstream_meta.is_sse;
    let is_json = upstream_meta.is_json;
    if response_adapter != ResponseAdapter::Passthrough {
        let status = StatusCode(upstream.status().as_u16());
        let mut headers = copy_upstream_response_headers(upstream.headers(), trace_id);

        if !is_stream {
            let upstream_body = upstream
                .read_all_bytes()
                .map_err(|err| format!("read upstream body failed: {err}"))?;
            let detected_sse =
                is_sse || (!is_json && looks_like_sse_payload(upstream_body.as_ref()));
            let (body, usage) = if detected_sse {
                let (synthesized, mut usage) =
                    collect_non_stream_json_from_sse_bytes(upstream_body.as_ref());
                let body = synthesized.unwrap_or_else(|| upstream_body.to_vec());
                merge_usage_from_body_without_output_text(&mut usage, &body);
                (body, usage)
            } else {
                let usage = serde_json::from_slice::<Value>(upstream_body.as_ref())
                    .ok()
                    .map(|value| parse_usage_from_json(&value))
                    .unwrap_or_default();
                (upstream_body.to_vec(), usage)
            };
            if should_apply_reasoning_guard_to_path(request_path) {
                match maybe_handle_reasoning_guard(
                    request,
                    headers,
                    usage.clone(),
                    &reasoning_guard_scope,
                    ReasoningGuardResponseMode::NonStream,
                    reasoning_guard_retry_budget_remaining,
                    UpstreamDebugMetaRefs {
                        request_id: &upstream_request_id,
                        cf_ray: &upstream_cf_ray,
                        auth_error: &upstream_auth_error,
                        identity_error_code: &upstream_identity_error_code,
                        content_type: &upstream_content_type,
                    },
                ) {
                    Ok(result) => return Ok(result),
                    Err((returned_request, returned_headers)) => {
                        request = returned_request;
                        headers = returned_headers;
                    }
                }
            }
            let response_body = if status.0 >= 400 {
                let message = with_upstream_debug_suffix(
                    extract_error_hint_from_body_or_headers(
                        status.0,
                        upstream_content_type.as_deref(),
                        &body,
                        upstream_cf_ray.as_deref(),
                        upstream_auth_error.as_deref(),
                        upstream_identity_error_code.as_deref(),
                    )
                    .or_else(|| extract_error_message_from_json_bytes(&body)),
                    None,
                    upstream_request_id.as_deref(),
                    upstream_cf_ray.as_deref(),
                    upstream_auth_error.as_deref(),
                    upstream_identity_error_code.as_deref(),
                )
                .unwrap_or_else(|| "upstream compatibility bridge failed".to_string());
                if crate::gateway::is_selected_model_capacity_error(&message) {
                    return Ok(upstream_capacity_bridge_result(
                        request,
                        usage,
                        message,
                        UpstreamDebugMetaRefs {
                            request_id: &upstream_request_id,
                            cf_ray: &upstream_cf_ray,
                            auth_error: &upstream_auth_error,
                            identity_error_code: &upstream_identity_error_code,
                            content_type: &upstream_content_type,
                        },
                    ));
                }
                convert_error_body_for_adapter(response_adapter, &message)
            } else {
                convert_success_body_for_adapter(
                    response_adapter,
                    &body,
                    request_path,
                    tool_name_restore_map,
                )
                .unwrap_or_else(|| body.clone())
            };
            let delivery_error = respond_json_bytes(request, status, headers, response_body);
            return Ok(terminal_bridge_result_with_debug_meta(
                usage,
                delivery_error,
                None,
                &upstream_request_id,
                &upstream_cf_ray,
                &upstream_auth_error,
                &upstream_identity_error_code,
                &upstream_content_type,
            ));
        }

        if status.0 >= 400 && !is_sse {
            let upstream_body = upstream
                .read_all_bytes()
                .map_err(|err| format!("read upstream body failed: {err}"))?;
            let message = with_upstream_debug_suffix(
                extract_error_hint_from_body_or_headers(
                    status.0,
                    upstream_content_type.as_deref(),
                    upstream_body.as_ref(),
                    upstream_cf_ray.as_deref(),
                    upstream_auth_error.as_deref(),
                    upstream_identity_error_code.as_deref(),
                )
                .or_else(|| extract_error_message_from_json_bytes(upstream_body.as_ref())),
                None,
                upstream_request_id.as_deref(),
                upstream_cf_ray.as_deref(),
                upstream_auth_error.as_deref(),
                upstream_identity_error_code.as_deref(),
            )
            .unwrap_or_else(|| "upstream compatibility bridge failed".to_string());
            if crate::gateway::is_selected_model_capacity_error(&message) {
                return Ok(upstream_capacity_bridge_result(
                    request,
                    UpstreamResponseUsage::default(),
                    message,
                    UpstreamDebugMetaRefs {
                        request_id: &upstream_request_id,
                        cf_ray: &upstream_cf_ray,
                        auth_error: &upstream_auth_error,
                        identity_error_code: &upstream_identity_error_code,
                        content_type: &upstream_content_type,
                    },
                ));
            }
            let response_body = convert_error_body_for_adapter(response_adapter, &message);
            let delivery_error = respond_json_bytes(request, status, headers, response_body);
            return Ok(terminal_bridge_result_with_debug_meta(
                UpstreamResponseUsage::default(),
                delivery_error,
                Some(message),
                &upstream_request_id,
                &upstream_cf_ray,
                &upstream_auth_error,
                &upstream_identity_error_code,
                &upstream_content_type,
            ));
        }

        replace_content_type_header(
            &mut headers,
            compatibility_stream_content_type(response_adapter, gemini_stream_output_mode),
        );
        match response_adapter {
            ResponseAdapter::AnthropicMessagesFromResponses => {
                let upstream_body = upstream
                    .read_all_bytes()
                    .map_err(|err| format!("read upstream body failed: {err}"))?;
                let usage_collector = Arc::new(Mutex::new(UpstreamResponseUsage::default()));
                let response_body: Box<dyn std::io::Read + Send> =
                    Box::new(AnthropicSseReader::from_reader(
                        std::io::Cursor::new(upstream_body.to_vec()),
                        Arc::clone(&usage_collector),
                        fallback_model,
                        tool_name_restore_map.cloned(),
                        request_started_at,
                    ));
                return Ok(respond_usage_collector_stream(
                    request,
                    status,
                    headers,
                    response_body,
                    usage_collector,
                    UpstreamDebugMetaRefs {
                        request_id: &upstream_request_id,
                        cf_ray: &upstream_cf_ray,
                        auth_error: &upstream_auth_error,
                        identity_error_code: &upstream_identity_error_code,
                        content_type: &upstream_content_type,
                    },
                ));
            }
            ResponseAdapter::ResponsesFromAnthropicMessages => {
                let upstream_body = upstream
                    .read_all_bytes()
                    .map_err(|err| format!("read upstream body failed: {err}"))?;
                let usage_collector = Arc::new(Mutex::new(UpstreamResponseUsage::default()));
                let response_body: Box<dyn std::io::Read + Send> =
                    Box::new(ResponsesFromAnthropicSseReader::from_reader(
                        std::io::Cursor::new(upstream_body.to_vec()),
                        Arc::clone(&usage_collector),
                        fallback_model,
                        request_started_at,
                    ));
                return Ok(respond_usage_collector_stream(
                    request,
                    status,
                    headers,
                    response_body,
                    usage_collector,
                    UpstreamDebugMetaRefs {
                        request_id: &upstream_request_id,
                        cf_ray: &upstream_cf_ray,
                        auth_error: &upstream_auth_error,
                        identity_error_code: &upstream_identity_error_code,
                        content_type: &upstream_content_type,
                    },
                ));
            }
            ResponseAdapter::ChatCompletionsFromResponses => {
                let upstream_body = upstream
                    .read_all_bytes()
                    .map_err(|err| format!("read upstream body failed: {err}"))?;
                let (synthesized, mut usage) =
                    collect_non_stream_json_from_sse_bytes(upstream_body.as_ref());
                let body = synthesized.unwrap_or_else(|| upstream_body.to_vec());
                merge_usage_from_body_without_output_text(&mut usage, &body);
                if should_apply_reasoning_guard_to_path(request_path) {
                    match maybe_handle_reasoning_guard(
                        request,
                        headers,
                        usage.clone(),
                        &reasoning_guard_scope,
                        ReasoningGuardResponseMode::NonStream,
                        reasoning_guard_retry_budget_remaining,
                        UpstreamDebugMetaRefs {
                            request_id: &upstream_request_id,
                            cf_ray: &upstream_cf_ray,
                            auth_error: &upstream_auth_error,
                            identity_error_code: &upstream_identity_error_code,
                            content_type: &upstream_content_type,
                        },
                    ) {
                        Ok(result) => return Ok(result),
                        Err((returned_request, returned_headers)) => {
                            request = returned_request;
                            headers = returned_headers;
                        }
                    }
                }
                let chat_body =
                    convert_responses_body_to_chat_completions(&body).unwrap_or_else(|| body);
                let response_body = chat_completion_body_to_single_sse(&chat_body);
                let len = Some(response_body.len());
                let response = Response::new(
                    status,
                    headers,
                    std::io::Cursor::new(response_body),
                    len,
                    None,
                );
                let delivery_error = request.respond(response).err().map(|err| err.to_string());
                return Ok(terminal_bridge_result_with_debug_meta(
                    usage,
                    delivery_error,
                    None,
                    &upstream_request_id,
                    &upstream_cf_ray,
                    &upstream_auth_error,
                    &upstream_identity_error_code,
                    &upstream_content_type,
                ));
            }
            ResponseAdapter::CompactFromChatCompletions => unreachable!(),
            ResponseAdapter::ImagesB64JsonFromResponses
            | ResponseAdapter::ImagesUrlFromResponses => {
                let response_format = if response_adapter == ResponseAdapter::ImagesUrlFromResponses
                {
                    ImagesResponseFormat::Url
                } else {
                    ImagesResponseFormat::B64Json
                };
                let upstream_body = upstream
                    .read_all_bytes()
                    .map_err(|err| format!("read upstream body failed: {err}"))?;
                let (synthesized, mut usage) =
                    collect_non_stream_json_from_sse_bytes(upstream_body.as_ref());
                let body = synthesized.unwrap_or_else(|| upstream_body.to_vec());
                merge_usage_from_body_without_output_text(&mut usage, &body);
                if should_apply_reasoning_guard_to_path(request_path) {
                    match maybe_handle_reasoning_guard(
                        request,
                        headers,
                        usage.clone(),
                        &reasoning_guard_scope,
                        ReasoningGuardResponseMode::NonStream,
                        reasoning_guard_retry_budget_remaining,
                        UpstreamDebugMetaRefs {
                            request_id: &upstream_request_id,
                            cf_ray: &upstream_cf_ray,
                            auth_error: &upstream_auth_error,
                            identity_error_code: &upstream_identity_error_code,
                            content_type: &upstream_content_type,
                        },
                    ) {
                        Ok(result) => return Ok(result),
                        Err((returned_request, returned_headers)) => {
                            request = returned_request;
                            headers = returned_headers;
                        }
                    }
                }
                let response_body = images_response_body_to_sse(&body, response_format);
                let len = Some(response_body.len());
                let response = Response::new(
                    status,
                    headers,
                    std::io::Cursor::new(response_body),
                    len,
                    None,
                );
                let delivery_error = request.respond(response).err().map(|err| err.to_string());
                return Ok(terminal_bridge_result_with_debug_meta(
                    usage,
                    delivery_error,
                    None,
                    &upstream_request_id,
                    &upstream_cf_ray,
                    &upstream_auth_error,
                    &upstream_identity_error_code,
                    &upstream_content_type,
                ));
            }
            ResponseAdapter::GeminiJson | ResponseAdapter::GeminiCliJson => unreachable!(),
            ResponseAdapter::GeminiSse | ResponseAdapter::GeminiCliSse => {
                let upstream_body = upstream
                    .read_all_bytes()
                    .map_err(|err| format!("read upstream body failed: {err}"))?;
                let usage_collector = Arc::new(Mutex::new(PassthroughSseCollector::default()));
                let response_body: Box<dyn std::io::Read + Send> =
                    Box::new(GeminiSseReader::from_reader(
                        std::io::Cursor::new(upstream_body.to_vec()),
                        Arc::clone(&usage_collector),
                        tool_name_restore_map.cloned(),
                        gemini_stream_output_mode.unwrap_or(GeminiStreamOutputMode::Sse),
                        gemini_cli_wrap_response_envelope(response_adapter),
                        request_started_at,
                    ));
                return Ok(respond_passthrough_collector_stream(
                    request,
                    status,
                    headers,
                    response_body,
                    usage_collector,
                    UpstreamDebugMetaRefs {
                        request_id: &upstream_request_id,
                        cf_ray: &upstream_cf_ray,
                        auth_error: &upstream_auth_error,
                        identity_error_code: &upstream_identity_error_code,
                        content_type: &upstream_content_type,
                    },
                ));
            }
            ResponseAdapter::Passthrough => {}
        }
    }

    match response_adapter {
        ResponseAdapter::Passthrough => {
            let status = StatusCode(upstream.status().as_u16());
            let mut headers = copy_upstream_response_headers(upstream.headers(), trace_id);

            if !is_stream {
                let upstream_body = upstream
                    .read_all_bytes()
                    .map_err(|err| format!("read upstream body failed: {err}"))?;
                let detected_sse =
                    is_sse || (!is_json && looks_like_sse_payload(upstream_body.as_ref()));
                let is_compact_request = is_compact_request_path(request_path);
                if detected_sse {
                    let (synthesized_body, mut usage) =
                        collect_non_stream_json_from_sse_bytes(upstream_body.as_ref());
                    let synthesized_response = synthesized_body.is_some();
                    let body = synthesized_body.unwrap_or_else(|| upstream_body.to_vec());
                    merge_usage_from_body_without_output_text(&mut usage, &body);
                    let upstream_error_hint = with_upstream_debug_suffix(
                        extract_error_hint_from_body_or_headers(
                            status.0,
                            upstream_content_type.as_deref(),
                            &body,
                            upstream_cf_ray.as_deref(),
                            upstream_auth_error.as_deref(),
                            upstream_identity_error_code.as_deref(),
                        )
                        .or_else(|| extract_error_message_from_json_bytes(&body)),
                        None,
                        upstream_request_id.as_deref(),
                        upstream_cf_ray.as_deref(),
                        upstream_auth_error.as_deref(),
                        upstream_identity_error_code.as_deref(),
                    );
                    if should_apply_reasoning_guard_to_path(request_path) {
                        match maybe_handle_reasoning_guard(
                            request,
                            headers,
                            usage.clone(),
                            &reasoning_guard_scope,
                            ReasoningGuardResponseMode::NonStream,
                            reasoning_guard_retry_budget_remaining,
                            UpstreamDebugMetaRefs {
                                request_id: &upstream_request_id,
                                cf_ray: &upstream_cf_ray,
                                auth_error: &upstream_auth_error,
                                identity_error_code: &upstream_identity_error_code,
                                content_type: &upstream_content_type,
                            },
                        ) {
                            Ok(result) => return Ok(result),
                            Err((returned_request, returned_headers)) => {
                                request = returned_request;
                                headers = returned_headers;
                            }
                        }
                    }
                    if should_suppress_deactivation_delivery(
                        upstream_error_hint.as_deref(),
                        _allow_failover_for_deactivation,
                    ) {
                        return Ok(terminal_bridge_result_with_debug_meta(
                            usage,
                            None,
                            upstream_error_hint,
                            &upstream_request_id,
                            &upstream_cf_ray,
                            &upstream_auth_error,
                            &upstream_identity_error_code,
                            &upstream_content_type,
                        ));
                    }
                    if synthesized_response {
                        replace_content_type_header(&mut headers, "application/json");
                    }
                    if status.0 < 400
                        && is_compact_request
                        && !compact_success_body_is_valid(body.as_ref())
                    {
                        return Ok(respond_invalid_compact_success_body(
                            request,
                            usage,
                            body.as_ref(),
                            upstream_request_id.as_deref(),
                            upstream_cf_ray.as_deref(),
                            upstream_auth_error.as_deref(),
                            upstream_identity_error_code.as_deref(),
                            trace_id,
                        ));
                    }
                    if is_compact_request
                        && compact_non_success_body_should_be_normalized(
                            status.0,
                            upstream_content_type.as_deref(),
                            body.as_ref(),
                            upstream_auth_error.as_deref(),
                            upstream_identity_error_code.as_deref(),
                        )
                    {
                        return Ok(respond_invalid_compact_non_success_body(
                            request,
                            status.0,
                            usage,
                            body.as_ref(),
                            upstream_content_type.as_deref(),
                            upstream_request_id.as_deref(),
                            upstream_cf_ray.as_deref(),
                            upstream_auth_error.as_deref(),
                            upstream_identity_error_code.as_deref(),
                            trace_id,
                        ));
                    }
                    if status.0 >= 400
                        && non_success_body_should_be_normalized(
                            status.0,
                            upstream_content_type.as_deref(),
                            body.as_ref(),
                            upstream_auth_error.as_deref(),
                            upstream_identity_error_code.as_deref(),
                        )
                    {
                        return Ok(respond_normalized_passthrough_non_success_body(
                            request,
                            usage,
                            body.as_ref(),
                            upstream_content_type.as_deref(),
                            upstream_request_id.as_deref(),
                            upstream_cf_ray.as_deref(),
                            upstream_auth_error.as_deref(),
                            upstream_identity_error_code.as_deref(),
                            trace_id,
                        ));
                    }
                    let len = Some(body.len());
                    let response =
                        Response::new(status, headers, std::io::Cursor::new(body), len, None);
                    let delivery_error = request.respond(response).err().map(|err| err.to_string());
                    return Ok(terminal_bridge_result_with_debug_meta(
                        usage,
                        delivery_error,
                        upstream_error_hint,
                        &upstream_request_id,
                        &upstream_cf_ray,
                        &upstream_auth_error,
                        &upstream_identity_error_code,
                        &upstream_content_type,
                    ));
                }

                let (_, sse_usage) = collect_non_stream_json_from_sse_bytes(upstream_body.as_ref());
                let usage = if is_json {
                    serde_json::from_slice::<Value>(upstream_body.as_ref())
                        .ok()
                        .map(|value| parse_usage_from_json(&value))
                        .unwrap_or_default()
                } else if usage_has_signal(&sse_usage) {
                    sse_usage
                } else {
                    UpstreamResponseUsage::default()
                };
                if should_apply_reasoning_guard_to_path(request_path) {
                    match maybe_handle_reasoning_guard(
                        request,
                        headers,
                        usage.clone(),
                        &reasoning_guard_scope,
                        ReasoningGuardResponseMode::NonStream,
                        reasoning_guard_retry_budget_remaining,
                        UpstreamDebugMetaRefs {
                            request_id: &upstream_request_id,
                            cf_ray: &upstream_cf_ray,
                            auth_error: &upstream_auth_error,
                            identity_error_code: &upstream_identity_error_code,
                            content_type: &upstream_content_type,
                        },
                    ) {
                        Ok(result) => return Ok(result),
                        Err((returned_request, returned_headers)) => {
                            request = returned_request;
                            headers = returned_headers;
                        }
                    }
                }
                let upstream_error_hint = with_upstream_debug_suffix(
                    extract_error_hint_from_body_or_headers(
                        status.0,
                        upstream_content_type.as_deref(),
                        upstream_body.as_ref(),
                        upstream_cf_ray.as_deref(),
                        upstream_auth_error.as_deref(),
                        upstream_identity_error_code.as_deref(),
                    )
                    .or_else(|| extract_error_message_from_json_bytes(upstream_body.as_ref())),
                    None,
                    upstream_request_id.as_deref(),
                    upstream_cf_ray.as_deref(),
                    upstream_auth_error.as_deref(),
                    upstream_identity_error_code.as_deref(),
                );
                if upstream_error_hint
                    .as_deref()
                    .is_some_and(crate::gateway::is_selected_model_capacity_error)
                {
                    return Ok(upstream_capacity_bridge_result(
                        request,
                        usage,
                        upstream_error_hint.expect("checked capacity error hint"),
                        UpstreamDebugMetaRefs {
                            request_id: &upstream_request_id,
                            cf_ray: &upstream_cf_ray,
                            auth_error: &upstream_auth_error,
                            identity_error_code: &upstream_identity_error_code,
                            content_type: &upstream_content_type,
                        },
                    ));
                }
                if status.0 >= 400
                    && non_success_body_should_be_normalized(
                        status.0,
                        upstream_content_type.as_deref(),
                        upstream_body.as_ref(),
                        upstream_auth_error.as_deref(),
                        upstream_identity_error_code.as_deref(),
                    )
                {
                    return Ok(respond_normalized_passthrough_non_success_body(
                        request,
                        usage,
                        upstream_body.as_ref(),
                        upstream_content_type.as_deref(),
                        upstream_request_id.as_deref(),
                        upstream_cf_ray.as_deref(),
                        upstream_auth_error.as_deref(),
                        upstream_identity_error_code.as_deref(),
                        trace_id,
                    ));
                }
                if should_suppress_deactivation_delivery(
                    upstream_error_hint.as_deref(),
                    _allow_failover_for_deactivation,
                ) {
                    return Ok(terminal_bridge_result_with_debug_meta(
                        usage,
                        None,
                        upstream_error_hint,
                        &upstream_request_id,
                        &upstream_cf_ray,
                        &upstream_auth_error,
                        &upstream_identity_error_code,
                        &upstream_content_type,
                    ));
                }
                let len = Some(upstream_body.len());
                let response = Response::new(
                    status,
                    headers,
                    std::io::Cursor::new(upstream_body.to_vec()),
                    len,
                    None,
                );
                let delivery_error = request.respond(response).err().map(|err| err.to_string());
                return Ok(terminal_bridge_result_with_debug_meta(
                    usage,
                    delivery_error,
                    upstream_error_hint,
                    &upstream_request_id,
                    &upstream_cf_ray,
                    &upstream_auth_error,
                    &upstream_identity_error_code,
                    &upstream_content_type,
                ));
            }

            if is_stream && !is_sse && status.0 >= 400 {
                let upstream_body = upstream
                    .read_all_bytes()
                    .map_err(|err| format!("read upstream body failed: {err}"))?;
                let usage = UpstreamResponseUsage::default();
                let upstream_error_hint = with_upstream_debug_suffix(
                    extract_error_hint_from_body_or_headers(
                        status.0,
                        upstream_content_type.as_deref(),
                        upstream_body.as_ref(),
                        upstream_cf_ray.as_deref(),
                        upstream_auth_error.as_deref(),
                        upstream_identity_error_code.as_deref(),
                    )
                    .or_else(|| extract_error_message_from_json_bytes(upstream_body.as_ref())),
                    None,
                    upstream_request_id.as_deref(),
                    upstream_cf_ray.as_deref(),
                    upstream_auth_error.as_deref(),
                    upstream_identity_error_code.as_deref(),
                );
                if upstream_error_hint
                    .as_deref()
                    .is_some_and(crate::gateway::is_selected_model_capacity_error)
                {
                    return Ok(upstream_capacity_bridge_result(
                        request,
                        usage,
                        upstream_error_hint.expect("checked capacity error hint"),
                        UpstreamDebugMetaRefs {
                            request_id: &upstream_request_id,
                            cf_ray: &upstream_cf_ray,
                            auth_error: &upstream_auth_error,
                            identity_error_code: &upstream_identity_error_code,
                            content_type: &upstream_content_type,
                        },
                    ));
                }
                if non_success_body_should_be_normalized(
                    status.0,
                    upstream_content_type.as_deref(),
                    upstream_body.as_ref(),
                    upstream_auth_error.as_deref(),
                    upstream_identity_error_code.as_deref(),
                ) {
                    return Ok(respond_normalized_passthrough_non_success_body(
                        request,
                        usage,
                        upstream_body.as_ref(),
                        upstream_content_type.as_deref(),
                        upstream_request_id.as_deref(),
                        upstream_cf_ray.as_deref(),
                        upstream_auth_error.as_deref(),
                        upstream_identity_error_code.as_deref(),
                        trace_id,
                    ));
                }
                let len = Some(upstream_body.len());
                let response = Response::new(
                    status,
                    headers,
                    std::io::Cursor::new(upstream_body.to_vec()),
                    len,
                    None,
                );
                let delivery_error = request.respond(response).err().map(|err| err.to_string());
                return Ok(terminal_bridge_result_with_debug_meta(
                    usage,
                    delivery_error,
                    upstream_error_hint,
                    &upstream_request_id,
                    &upstream_cf_ray,
                    &upstream_auth_error,
                    &upstream_identity_error_code,
                    &upstream_content_type,
                ));
            }

            if is_stream && !is_sse && status.0 < 400 && is_compact_request_path(request_path) {
                let upstream_body = upstream
                    .read_all_bytes()
                    .map_err(|err| format!("read upstream body failed: {err}"))?;
                let usage = UpstreamResponseUsage::default();
                return Ok(respond_compact_success_body(
                    request,
                    status,
                    headers,
                    usage,
                    upstream_body.as_ref(),
                    upstream_request_id.as_deref(),
                    upstream_cf_ray.as_deref(),
                    upstream_auth_error.as_deref(),
                    upstream_identity_error_code.as_deref(),
                    &upstream_content_type,
                    trace_id,
                ));
            }

            if is_sse || is_stream {
                let usage_collector = Arc::new(Mutex::new(PassthroughSseCollector::default()));
                let response_body: Box<dyn std::io::Read + Send> =
                    if request_path.starts_with("/v1/responses") {
                        Box::new(OpenAIResponsesPassthroughSseReader::from_stream_response(
                            upstream,
                            Arc::clone(&usage_collector),
                            keepalive_frame,
                            request_started_at,
                        ))
                    } else {
                        return Err(format!(
                            "stream upstream response is not supported for path {request_path}"
                        ));
                    };
                force_openai_responses_stream_content_type(&mut headers, request_path, is_stream);
                if should_apply_reasoning_guard_to_path(request_path) {
                    let result = respond_passthrough_collector_stream_strict_guard(
                        request,
                        request_path,
                        status,
                        headers,
                        response_body,
                        usage_collector,
                        _allow_failover_for_deactivation,
                        &reasoning_guard_scope,
                        reasoning_guard_retry_budget_remaining,
                        UpstreamDebugMetaRefs {
                            request_id: &upstream_request_id,
                            cf_ray: &upstream_cf_ray,
                            auth_error: &upstream_auth_error,
                            identity_error_code: &upstream_identity_error_code,
                            content_type: &upstream_content_type,
                        },
                    );
                    log_bridge_stream_diagnostics(response_adapter, request_path, &result);
                    return Ok(result);
                }
                let delivery_error =
                    respond_streaming_chunked(request, status, headers, response_body)
                        .err()
                        .map(|err| err.to_string());
                let collector = usage_collector
                    .lock()
                    .map(|guard| guard.clone())
                    .unwrap_or_default();
                let last_sse_event_type = collector.last_event_type.clone();
                let result = with_bridge_debug_meta(
                    UpstreamResponseBridgeResult {
                        usage: collector.usage,
                        stream_terminal_seen: collector.saw_terminal,
                        stream_terminal_error: collector.terminal_error,
                        delivery_error,
                        upstream_error_hint: with_upstream_debug_suffix(
                            collector.upstream_error_hint,
                            None,
                            upstream_request_id.as_deref(),
                            upstream_cf_ray.as_deref(),
                            upstream_auth_error.as_deref(),
                            upstream_identity_error_code.as_deref(),
                        ),
                        delivered_status_code: None,
                        upstream_request_id: None,
                        upstream_cf_ray: None,
                        upstream_auth_error: None,
                        upstream_identity_error_code: None,
                        upstream_content_type: None,
                        last_sse_event_type: None,
                        pending_failover_request: None,
                        reasoning_guard_action: None,
                        reasoning_guard_target_token: None,
                        continuation_reasoning_items: Vec::new(),
                    },
                    &upstream_request_id,
                    &upstream_cf_ray,
                    &upstream_auth_error,
                    &upstream_identity_error_code,
                    &upstream_content_type,
                    last_sse_event_type,
                );
                log_bridge_stream_diagnostics(response_adapter, request_path, &result);
                return Ok(result);
            }

            let upstream_body = upstream
                .read_all_bytes()
                .map_err(|err| format!("read upstream body failed: {err}"))?;
            let len = Some(upstream_body.len());
            let response = Response::new(
                status,
                headers,
                std::io::Cursor::new(upstream_body.to_vec()),
                len,
                None,
            );
            let delivery_error = request.respond(response).err().map(|err| err.to_string());
            Ok(terminal_bridge_result_with_debug_meta(
                UpstreamResponseUsage::default(),
                delivery_error,
                None,
                &upstream_request_id,
                &upstream_cf_ray,
                &upstream_auth_error,
                &upstream_identity_error_code,
                &upstream_content_type,
            ))
        }
        ResponseAdapter::CompactFromChatCompletions => {
            let status = StatusCode(upstream.status().as_u16());
            let headers = copy_upstream_response_headers(upstream.headers(), trace_id);
            let upstream_body = upstream
                .read_all_bytes()
                .map_err(|err| format!("read upstream body failed: {err}"))?;
            let usage = serde_json::from_slice::<Value>(upstream_body.as_ref())
                .ok()
                .map(|value| parse_usage_from_json(&value))
                .unwrap_or_default();
            let response_body = if status.0 < 400 {
                convert_chat_completions_body_to_compact(upstream_body.as_ref())
                    .unwrap_or_else(|| upstream_body.to_vec())
            } else {
                upstream_body.to_vec()
            };
            let upstream_error_hint = (status.0 >= 400)
                .then(|| {
                    with_upstream_debug_suffix(
                        extract_error_hint_from_body(status.0, upstream_body.as_ref()),
                        None,
                        upstream_request_id.as_deref(),
                        upstream_cf_ray.as_deref(),
                        upstream_auth_error.as_deref(),
                        upstream_identity_error_code.as_deref(),
                    )
                })
                .flatten();
            let delivery_error = respond_json_bytes(request, status, headers, response_body);
            Ok(terminal_bridge_result_with_debug_meta(
                usage,
                delivery_error,
                upstream_error_hint,
                &upstream_request_id,
                &upstream_cf_ray,
                &upstream_auth_error,
                &upstream_identity_error_code,
                &upstream_content_type,
            ))
        }
        ResponseAdapter::AnthropicMessagesFromResponses
        | ResponseAdapter::ResponsesFromAnthropicMessages
        | ResponseAdapter::ChatCompletionsFromResponses
        | ResponseAdapter::ImagesB64JsonFromResponses
        | ResponseAdapter::ImagesUrlFromResponses
        | ResponseAdapter::GeminiJson
        | ResponseAdapter::GeminiCliJson
        | ResponseAdapter::GeminiSse
        | ResponseAdapter::GeminiCliSse => unreachable!(),
    }
}

/// 函数 `resolve_stream_keepalive_frame`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - response_adapter: 参数 response_adapter
/// - request_path: 参数 request_path
///
/// # 返回
/// 返回函数执行结果
fn resolve_stream_keepalive_frame(
    response_adapter: ResponseAdapter,
    request_path: &str,
) -> SseKeepAliveFrame {
    match response_adapter {
        ResponseAdapter::Passthrough => {
            if request_path.starts_with("/v1/responses") {
                SseKeepAliveFrame::OpenAIResponses
            } else {
                SseKeepAliveFrame::Comment
            }
        }
        ResponseAdapter::AnthropicMessagesFromResponses
        | ResponseAdapter::ResponsesFromAnthropicMessages
        | ResponseAdapter::ChatCompletionsFromResponses
        | ResponseAdapter::CompactFromChatCompletions
        | ResponseAdapter::ImagesB64JsonFromResponses
        | ResponseAdapter::ImagesUrlFromResponses
        | ResponseAdapter::GeminiJson
        | ResponseAdapter::GeminiCliJson
        | ResponseAdapter::GeminiSse
        | ResponseAdapter::GeminiCliSse => SseKeepAliveFrame::Comment,
    }
}

#[cfg(test)]
#[path = "delivery_tests.rs"]
mod tests;
