//! Bounded Chat Completions SSE -> Responses SSE converter (上游 Chat 流式路径).
//!
//! 与 `ResponsesFromAnthropicSseReader` 同构：消费上游 Chat SSE chunk，
//! 产出 Responses SSE 语义事件（含单调 sequence_number），并在 `[DONE]`/
//! EOF/边界违规时发出唯一终态。所有分配前先做硬上限检查；违规只会产生
//! 稳定的本地错误事件，绝不把原始 Chat 帧透传给客户端。

use super::{
    append_output_text, json, mark_first_response_ms_on_usage,
    should_emit_keepalive_after_first_frame, stream_idle_timed_out, stream_wait_timeout, Arc,
    Cursor, Map, Mutex, Read, SseKeepAliveFrame, UpstreamResponseUsage, UpstreamSseFramePump,
    UpstreamSseFramePumpItem, Value,
};
use std::time::Instant;
use crate::gateway::ChatCompletionsContextStore;

const MAX_CHAT_SSE_CHUNKS: usize = 4096;
const MAX_CHAT_TOOL_CALLS: usize = 32;
const MAX_CHAT_ARGUMENT_BYTES: usize = 1024 * 1024;
const MAX_CHAT_TEXT_BYTES: usize = 8 * 1024 * 1024;

const CHAT_STREAM_ERROR_MESSAGE: &str = "upstream chat stream failed validation";
const CHAT_STREAM_INCOMPLETE_MESSAGE: &str = "upstream chat stream ended without [DONE]";
const CHAT_STREAM_BOUND_MESSAGE: &str = "upstream chat stream exceeded safety limits";

pub(crate) struct ResponsesFromChatCompletionsSseReader {
    upstream: UpstreamSseFramePump,
    out_cursor: Cursor<Vec<u8>>,
    state: ChatToResponsesStreamState,
    usage_collector: Arc<Mutex<UpstreamResponseUsage>>,
    request_started_at: Instant,
    last_upstream_activity: Instant,
    saw_upstream_frame: bool,
    context_store: Option<Arc<ChatCompletionsContextStore>>,
}

#[derive(Default)]
struct ChatToResponsesStreamState {
    response_id: Option<String>,
    model: Option<String>,
    created: i64,
    started: bool,
    finished: bool,
    saw_terminal_frame: bool,
    text_item_started: bool,
    text_part_started: bool,
    text_finished: bool,
    output_text: String,
    reasoning_text: String,
    reasoning_started: bool,
    tool_calls: Vec<PendingChatToolCall>,
    finish_reason: Option<String>,
    input_tokens: Option<i64>,
    cached_input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    reasoning_output_tokens: Option<i64>,
    total_tokens: Option<i64>,
    sequence_number: u64,
    chunk_count: usize,
    terminal_error: Option<String>,
}

#[derive(Default)]
struct PendingChatToolCall {
    index: usize,
    id: String,
    name: String,
    arguments: String,
    started: bool,
}

impl ResponsesFromChatCompletionsSseReader {
    pub(crate) fn from_reader<R>(
        upstream: R,
        usage_collector: Arc<Mutex<UpstreamResponseUsage>>,
        fallback_model: Option<&str>,
        request_started_at: Instant,
        context_store: Option<Arc<ChatCompletionsContextStore>>,
    ) -> Self
    where
        R: Read + Send + 'static,
    {
        let mut state = ChatToResponsesStreamState::default();
        state.model = fallback_model
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        Self {
            upstream: UpstreamSseFramePump::from_reader(upstream),
            out_cursor: Cursor::new(Vec::new()),
            state,
            usage_collector,
            request_started_at,
            last_upstream_activity: Instant::now(),
            saw_upstream_frame: false,
            context_store,
        }
    }

    pub(crate) fn new(
        upstream: reqwest::blocking::Response,
        usage_collector: Arc<Mutex<UpstreamResponseUsage>>,
        fallback_model: Option<&str>,
        request_started_at: Instant,
        context_store: Option<Arc<ChatCompletionsContextStore>>,
    ) -> Self {
        Self::from_reader(
            upstream,
            usage_collector,
            fallback_model,
            request_started_at,
            context_store,
        )
    }

    fn next_chunk(&mut self) -> std::io::Result<Vec<u8>> {
        loop {
            match self
                .upstream
                .recv_timeout(stream_wait_timeout(self.last_upstream_activity))
            {
                Ok(UpstreamSseFramePumpItem::Frame(frame)) => {
                    self.last_upstream_activity = Instant::now();
                    self.saw_upstream_frame = true;
                    mark_first_response_ms_on_usage(&self.usage_collector, self.request_started_at);
                    let mapped = self.process_sse_frame(&frame);
                    if !mapped.is_empty() {
                        mark_first_response_ms_on_usage(
                            &self.usage_collector,
                            self.request_started_at,
                        );
                        return Ok(mapped);
                    }
                }
                Ok(UpstreamSseFramePumpItem::Eof)
                | Ok(UpstreamSseFramePumpItem::Error(_))
                | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    let finished = self.finish_stream();
                    if !finished.is_empty() {
                        mark_first_response_ms_on_usage(
                            &self.usage_collector,
                            self.request_started_at,
                        );
                    }
                    return Ok(finished);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if stream_idle_timed_out(self.last_upstream_activity) {
                        let finished = self.finish_stream();
                        if !finished.is_empty() {
                            mark_first_response_ms_on_usage(
                                &self.usage_collector,
                                self.request_started_at,
                            );
                        }
                        return Ok(finished);
                    }
                    if should_emit_keepalive_after_first_frame(self.saw_upstream_frame) {
                        return Ok(SseKeepAliveFrame::Comment.bytes().to_vec());
                    }
                }
            }
        }
    }

    fn process_sse_frame(&mut self, lines: &[String]) -> Vec<u8> {
        let mut data_lines = Vec::new();
        for line in lines {
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if let Some(rest) = trimmed.strip_prefix("data:") {
                data_lines.push(rest.trim_start().to_string());
            }
        }
        if data_lines.is_empty() {
            return Vec::new();
        }
        let data = data_lines.join("\n");
        if data.trim() == "[DONE]" {
            self.state.saw_terminal_frame = true;
            return self.finish_stream();
        }
        let value = match serde_json::from_str::<Value>(&data) {
            Ok(value) => value,
            Err(_) => return Vec::new(),
        };
        self.consume_chat_chunk(&value)
    }

    fn consume_chat_chunk(&mut self, value: &Value) -> Vec<u8> {
        if self.state.finished {
            return Vec::new();
        }
        self.state.chunk_count += 1;
        if self.state.chunk_count > MAX_CHAT_SSE_CHUNKS {
            self.fail(CHAT_STREAM_BOUND_MESSAGE);
            return self.finish_stream();
        }
        if let Some(id) = value.get("id").and_then(Value::as_str) {
            self.state.response_id = Some(id.to_string());
        }
        if let Some(model) = value.get("model").and_then(Value::as_str) {
            self.state.model = Some(model.to_string());
        }
        if let Some(created) = value.get("created").and_then(Value::as_i64) {
            self.state.created = created;
        }
        if let Some(usage) = value.get("usage").and_then(Value::as_object) {
            self.capture_usage(usage);
        }
        let mut out = String::new();
        if let Some(choices) = value.get("choices").and_then(Value::as_array) {
            if let Some(choice) = choices.first() {
                if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                    self.state.finish_reason = Some(reason.to_string());
                }
                if let Some(delta) = choice.get("delta").and_then(Value::as_object) {
                    self.consume_delta(delta, &mut out);
                }
            }
        }
        out.into_bytes()
    }

    fn consume_delta(&mut self, delta: &Map<String, Value>, out: &mut String) {
        if let Some(text) = delta.get("content").and_then(Value::as_str) {
            if !text.is_empty() {
                if self.state.output_text.len() + text.len() > MAX_CHAT_TEXT_BYTES {
                    self.fail(CHAT_STREAM_BOUND_MESSAGE);
                    return;
                }
                append_output_text(&mut self.state.output_text, text);
                self.ensure_text_part_started(out);
                self.emit(
                    out,
                    "response.output_text.delta",
                    &json!({
                        "type": "response.output_text.delta",
                        "delta": text,
                        "item_id": self.text_item_id(),
                        "output_index": 0,
                        "content_index": 0,
                    }),
                );
            }
        }
        if let Some(reasoning) = delta
            .get("reasoning_content")
            .or_else(|| delta.get("reasoning"))
            .and_then(Value::as_str)
        {
            if !reasoning.is_empty() {
                if self.state.reasoning_text.len() + reasoning.len() > MAX_CHAT_TEXT_BYTES {
                    self.fail(CHAT_STREAM_BOUND_MESSAGE);
                    return;
                }
                append_output_text(&mut self.state.reasoning_text, reasoning);
                self.ensure_response_started(out);
                if !self.state.reasoning_started {
                    self.state.reasoning_started = true;
                    self.emit(
                        out,
                        "response.reasoning_summary_text.added",
                        &json!({
                            "type": "response.reasoning_summary_text.added",
                            "item_id": self.text_item_id(),
                            "output_index": 0,
                            "content_index": 0,
                            "part": { "type": "reasoning_summary_text", "text": "" },
                        }),
                    );
                }
                self.emit(
                    out,
                    "response.reasoning_summary_text.delta",
                    &json!({
                        "type": "response.reasoning_summary_text.delta",
                        "delta": reasoning,
                        "item_id": self.text_item_id(),
                        "output_index": 0,
                        "content_index": 0,
                    }),
                );
            }
        }
        if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for call in tool_calls {
                self.consume_tool_call_delta(call, out);
            }
        }
    }

    fn consume_tool_call_delta(&mut self, call: &Value, out: &mut String) {
        let Some(call_obj) = call.as_object() else {
            return;
        };
        let index = call_obj
            .get("index")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or_default();
        if index >= MAX_CHAT_TOOL_CALLS {
            self.fail(CHAT_STREAM_BOUND_MESSAGE);
            return;
        }
        while self.state.tool_calls.len() <= index {
            self.state.tool_calls.push(PendingChatToolCall {
                index: self.state.tool_calls.len(),
                ..Default::default()
            });
        }
        let mut pending_id = self.state.tool_calls[index].id.clone();
        let mut pending_name = self.state.tool_calls[index].name.clone();
        let pending_arguments = self.state.tool_calls[index].arguments.clone();
        let pending_started = self.state.tool_calls[index].started;
        if pending_id.is_empty() {
            if let Some(id) = call_obj.get("id").and_then(Value::as_str) {
                pending_id = id.to_string();
            }
        }
        if pending_name.is_empty() {
            if let Some(name) = call_obj
                .get("function")
                .and_then(|function| function.get("name"))
                .and_then(Value::as_str)
            {
                pending_name = name.to_string();
            }
        }
        let fragment = call_obj
            .get("function")
            .and_then(|function| function.get("arguments"))
            .map(|value| match value {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default();
        if !fragment.is_empty() {
            if pending_arguments.len() + fragment.len() > MAX_CHAT_ARGUMENT_BYTES {
                self.fail(CHAT_STREAM_BOUND_MESSAGE);
                return;
            }
            self.ensure_response_started(out);
            if !pending_started {
                let item_id = self.tool_item_id(index);
                let name = if pending_name.is_empty() { "tool" } else { &pending_name };
                self.emit(
                    out,
                    "response.output_item.added",
                    &json!({
                        "type": "response.output_item.added",
                        "output_index": self.tool_output_index(index),
                        "item": {
                            "id": item_id,
                            "type": "function_call",
                            "status": "in_progress",
                            "call_id": item_id,
                            "name": name,
                            "arguments": "",
                        },
                    }),
                );
            }
            let item_id = self.tool_item_id(index);
            self.emit(
                out,
                "response.function_call_arguments.delta",
                &json!({
                    "type": "response.function_call_arguments.delta",
                    "delta": fragment,
                    "item_id": item_id,
                    "output_index": self.tool_output_index(index),
                }),
            );
            let pending = &mut self.state.tool_calls[index];
            pending.id = pending_id;
            pending.name = pending_name;
            pending.arguments.push_str(fragment.as_str());
            pending.started = true;
        } else if !pending_started {
            let pending = &mut self.state.tool_calls[index];
            if !pending_id.is_empty() || !pending_name.is_empty() {
                pending.id = pending_id;
                pending.name = pending_name;
            }
        }
    }

    fn capture_usage(&mut self, usage: &Map<String, Value>) {
        if let Some(value) = usage_i64(usage, &["prompt_tokens", "input_tokens"]) {
            self.state.input_tokens = Some(value);
        }
        if let Some(value) = usage_i64(
            usage,
            &[
                "prompt_tokens_details.cached_tokens",
                "prompt_tokens_details.cache_read_input_tokens",
                "input_tokens_details.cached_tokens",
            ],
        ) {
            self.state.cached_input_tokens = Some(value);
        }
        if let Some(value) = usage_i64(usage, &["completion_tokens", "output_tokens"]) {
            self.state.output_tokens = Some(value);
        }
        if let Some(value) = usage_i64(
            usage,
            &[
                "completion_tokens_details.reasoning_tokens",
                "output_tokens_details.reasoning_tokens",
            ],
        ) {
            self.state.reasoning_output_tokens = Some(value);
        }
        if let Some(value) = usage_i64(usage, &["total_tokens"]) {
            self.state.total_tokens = Some(value);
        }
    }

    fn fail(&mut self, reason: &str) {
        if self.state.terminal_error.is_none() {
            self.state.terminal_error = Some(reason.to_string());
        }
    }

    fn ensure_response_started(&mut self, out: &mut String) {
        if self.state.started {
            return;
        }
        self.state.started = true;
        let response = self.response_payload("in_progress");
        self.emit(
            out,
            "response.created",
            &json!({
                "type": "response.created",
                "response": response,
            }),
        );
        self.emit(
            out,
            "response.in_progress",
            &json!({
                "type": "response.in_progress",
                "response": self.response_payload("in_progress"),
            }),
        );
    }

    fn ensure_text_part_started(&mut self, out: &mut String) {
        self.ensure_response_started(out);
        if !self.state.text_item_started {
            self.state.text_item_started = true;
            self.emit(
                out,
                "response.output_item.added",
                &json!({
                    "type": "response.output_item.added",
                    "output_index": 0,
                    "item": {
                        "id": self.text_item_id(),
                        "type": "message",
                        "status": "in_progress",
                        "role": "assistant",
                        "content": [],
                    }
                }),
            );
        }
        if !self.state.text_part_started {
            self.state.text_part_started = true;
            self.emit(
                out,
                "response.content_part.added",
                &json!({
                    "type": "response.content_part.added",
                    "item_id": self.text_item_id(),
                    "output_index": 0,
                    "content_index": 0,
                    "part": { "type": "output_text", "text": "" },
                }),
            );
        }
    }

    fn finish_text_item(&mut self, out: &mut String) {
        if !self.state.text_part_started || self.state.text_finished {
            return;
        }
        self.state.text_finished = true;
        if self.state.reasoning_started {
            self.emit(
                out,
                "response.reasoning_summary_text.done",
                &json!({
                    "type": "response.reasoning_summary_text.done",
                    "text": self.state.reasoning_text,
                    "item_id": self.text_item_id(),
                    "output_index": 0,
                    "content_index": 0,
                }),
            );
        }
        self.emit(
            out,
            "response.output_text.done",
            &json!({
                "type": "response.output_text.done",
                "text": self.state.output_text,
                "item_id": self.text_item_id(),
                "output_index": 0,
                "content_index": 0,
            }),
        );
        self.emit(
            out,
            "response.content_part.done",
            &json!({
                "type": "response.content_part.done",
                "item_id": self.text_item_id(),
                "output_index": 0,
                "content_index": 0,
                "part": { "type": "output_text", "text": self.state.output_text },
            }),
        );
        self.emit(
            out,
            "response.output_item.done",
            &json!({
                "type": "response.output_item.done",
                "output_index": 0,
                "item": {
                    "id": self.text_item_id(),
                    "type": "message",
                    "status": "completed",
                    "role": "assistant",
                    "content": [{ "type": "output_text", "text": self.state.output_text }],
                }
            }),
        );
    }

    fn finish_tool_calls(&mut self, out: &mut String) {
        for index in 0..self.state.tool_calls.len() {
            let started = self.state.tool_calls[index].started;
            if !started {
                continue;
            }
            let item_id = self.tool_item_id(index);
            let arguments = normalize_json_fragment(self.state.tool_calls[index].arguments.as_str());
            let name = {
                let pending = &self.state.tool_calls[index];
                if pending.name.is_empty() {
                    "tool".to_string()
                } else {
                    pending.name.clone()
                }
            };
            self.state.tool_calls[index].arguments = arguments.clone();
            self.emit(
                out,
                "response.function_call_arguments.done",
                &json!({
                    "type": "response.function_call_arguments.done",
                    "arguments": arguments,
                    "item_id": item_id,
                    "output_index": self.tool_output_index(index),
                }),
            );
            self.emit(
                out,
                "response.output_item.done",
                &json!({
                    "type": "response.output_item.done",
                    "output_index": self.tool_output_index(index),
                    "item": {
                        "id": item_id,
                        "type": "function_call",
                        "status": "completed",
                        "call_id": item_id,
                        "name": name,
                        "arguments": arguments,
                    },
                }),
            );
        }
    }

    fn finish_stream(&mut self) -> Vec<u8> {
        if self.state.finished {
            return Vec::new();
        }
        self.state.finished = true;
        let mut out = String::new();
        self.ensure_response_started(&mut out);
        self.finish_text_item(&mut out);
        self.finish_tool_calls(&mut out);
        self.publish_usage();
        if let Some(message) = self.state.terminal_error.clone() {
            self.emit(
                &mut out,
                "error",
                &json!({
                    "type": "error",
                    "error": {
                        "code": "upstream_chat_stream_error",
                        "message": message,
                    },
                }),
            );
        } else {
            match self.state.finish_reason.as_deref() {
                Some("content_filter") => {
                    self.emit(
                        &mut out,
                        "error",
                        &json!({
                            "type": "error",
                            "error": {
                                "code": "upstream_content_filter",
                                "message": CHAT_STREAM_ERROR_MESSAGE,
                            },
                        }),
                    );
                }
                Some("length") => {
                    self.emit(
                        &mut out,
                        "response.incomplete",
                        &json!({
                            "type": "response.incomplete",
                            "response": self.response_payload("incomplete"),
                            "incomplete_details": { "reason": "max_output_tokens" },
                        }),
                    );
                }
                _ => {
                    if !self.state.saw_terminal_frame {
                        self.emit(
                            &mut out,
                            "error",
                            &json!({
                                "type": "error",
                                "error": {
                                    "code": "upstream_chat_stream_incomplete",
                                    "message": CHAT_STREAM_INCOMPLETE_MESSAGE,
                                },
                            }),
                        );
                    } else {
                        self.emit(
                            &mut out,
                            "response.completed",
                            &json!({
                                "type": "response.completed",
                                "response": self.response_payload("completed"),
                            }),
                        );
                        self.capture_completed_context();
                    }
                }
            }
        }
        out.into_bytes()
    }

    /// 在真正完成的 Chat SSE 流结束后，保存可供下一轮 previous_response_id
    /// 使用的 assistant 消息。上游无 id 时 response_id() 返回固定兜底值，
    /// store 会拒绝该值，避免不同会话互相覆盖。
    fn capture_completed_context(&self) {
        let Some(store) = self.context_store.as_ref() else {
            return;
        };
        let mut assistant = Map::new();
        assistant.insert("role".to_string(), Value::String("assistant".to_string()));
        let mut has_context = false;
        if !self.state.output_text.is_empty() {
            assistant.insert(
                "content".to_string(),
                Value::String(self.state.output_text.clone()),
            );
            has_context = true;
        }
        if !self.state.reasoning_text.is_empty() {
            assistant.insert(
                "reasoning_content".to_string(),
                Value::String(self.state.reasoning_text.clone()),
            );
            has_context = true;
        }
        let tool_calls = self
            .state
            .tool_calls
            .iter()
            .filter(|call| call.started)
            .map(|call| {
                json!({
                    "id": if call.id.is_empty() {
                        format!("fc_{}_{}", self.response_id(), call.index)
                    } else {
                        call.id.clone()
                    },
                    "type": "function",
                    "function": {
                        "name": if call.name.is_empty() { "tool" } else { call.name.as_str() },
                        "arguments": call.arguments,
                    },
                })
            })
            .collect::<Vec<_>>();
        if !tool_calls.is_empty() {
            assistant.insert("tool_calls".to_string(), Value::Array(tool_calls));
            has_context = true;
        }
        if has_context {
            store.insert(None, self.response_id().as_str(), vec![Value::Object(assistant)]);
        }
    }

    fn emit(&mut self, out: &mut String, event: &str, payload: &Value) {
        self.state.sequence_number += 1;
        let mut enriched = payload.clone();
        if let Some(object) = enriched.as_object_mut() {
            object.insert(
                "sequence_number".to_string(),
                Value::Number(self.state.sequence_number.into()),
            );
        }
        append_sse_event(out, event, &enriched);
    }

    fn publish_usage(&self) {
        if let Ok(mut usage) = self.usage_collector.lock() {
            usage.input_tokens = self.state.input_tokens;
            usage.cached_input_tokens = self.state.cached_input_tokens;
            usage.output_tokens = self.state.output_tokens;
            usage.total_tokens = self.state.total_tokens;
            usage.reasoning_output_tokens = self.state.reasoning_output_tokens;
            if !self.state.output_text.trim().is_empty() {
                usage.output_text = Some(self.state.output_text.clone());
            }
        }
    }

    fn response_payload(&self, status: &str) -> Value {
        json!({
            "id": self.response_id(),
            "object": "response",
            "created_at": self.state.created,
            "status": status,
            "model": self.model(),
            "output": if status == "completed" { self.completed_output() } else { Value::Array(Vec::new()) },
            "usage": self.usage_payload(),
        })
    }

    fn completed_output(&self) -> Value {
        let mut output = Vec::new();
        if !self.state.output_text.is_empty() {
            output.push(json!({
                "id": self.text_item_id(),
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": self.state.output_text }],
            }));
        }
        for (index, pending) in self.state.tool_calls.iter().enumerate() {
            if !pending.started {
                continue;
            }
            let item_id = self.tool_item_id(index);
            output.push(json!({
                "id": item_id,
                "type": "function_call",
                "status": "completed",
                "call_id": item_id,
                "name": if pending.name.is_empty() { "tool" } else { &pending.name },
                "arguments": pending.arguments,
            }));
        }
        Value::Array(output)
    }

    fn tool_output_index(&self, tool_index: usize) -> usize {
        usize::from(!self.state.output_text.is_empty()) + tool_index
    }

    fn usage_payload(&self) -> Value {
        let mut payload = json!({
            "input_tokens": self.state.input_tokens.unwrap_or(0),
            "output_tokens": self.state.output_tokens.unwrap_or(0),
        });
        let total = self.state.total_tokens.or_else(|| {
            Some(
                self.state.input_tokens.unwrap_or(0) + self.state.output_tokens.unwrap_or(0),
            )
        });
        if let Some(total) = total {
            payload["total_tokens"] = Value::Number(total.into());
        }
        if self.state.cached_input_tokens.is_some()
            || self.state.reasoning_output_tokens.is_some()
        {
            let mut input_details = serde_json::Map::new();
            if let Some(cached) = self.state.cached_input_tokens {
                input_details.insert("cached_tokens".to_string(), Value::Number(cached.into()));
            }
            if !input_details.is_empty() {
                payload["input_tokens_details"] = Value::Object(input_details);
            }
            let mut output_details = serde_json::Map::new();
            if let Some(reasoning) = self.state.reasoning_output_tokens {
                output_details.insert(
                    "reasoning_tokens".to_string(),
                    Value::Number(reasoning.into()),
                );
            }
            if !output_details.is_empty() {
                payload["output_tokens_details"] = Value::Object(output_details);
            }
        }
        payload
    }

    fn response_id(&self) -> String {
        self.state
            .response_id
            .clone()
            .unwrap_or_else(|| "resp_codexmanager".to_string())
    }

    fn text_item_id(&self) -> String {
        format!("msg_{}", self.response_id())
    }

    fn tool_item_id(&self, index: usize) -> String {
        format!("fc_{}_{}", self.response_id(), index)
    }

    fn model(&self) -> String {
        self.state.model.clone().unwrap_or_default()
    }
}

impl Read for ResponsesFromChatCompletionsSseReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            let read = self.out_cursor.read(buf)?;
            if read > 0 {
                return Ok(read);
            }
            let chunk = self.next_chunk()?;
            if chunk.is_empty() {
                return Ok(0);
            }
            self.out_cursor = Cursor::new(chunk);
        }
    }
}

fn normalize_json_fragment(value: &str) -> String {
    if value.trim().is_empty() {
        return "{}".to_string();
    }
    serde_json::from_str::<Value>(value)
        .map(|json| json.to_string())
        .unwrap_or_else(|_| value.to_string())
}

fn append_sse_event(buffer: &mut String, event: &str, payload: &Value) {
    buffer.push_str("event: ");
    buffer.push_str(event);
    buffer.push('\n');
    buffer.push_str("data: ");
    buffer.push_str(payload.to_string().as_str());
    buffer.push_str("\n\n");
}

fn usage_i64(usage: &Map<String, Value>, paths: &[&str]) -> Option<i64> {
    for path in paths {
        let mut current: Option<&Value> = None;
        let mut found = true;
        for (index, segment) in path.split('.').enumerate() {
            current = if index == 0 {
                usage.get(segment)
            } else {
                current
                    .and_then(Value::as_object)
                    .and_then(|object| object.get(segment))
            };
            if current.is_none() {
                found = false;
                break;
            }
        }
        if !found {
            continue;
        }
        if let Some(value) = current.and_then(Value::as_i64) {
            return Some(value);
        }
    }
    None
}

#[cfg(test)]
#[path = "responses_from_chat_completions_tests.rs"]
mod tests;
