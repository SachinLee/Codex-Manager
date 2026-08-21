//! Phase 4 tests: bounded Chat SSE -> Responses SSE reader.
//!
//! Covers text/reasoning/tool lifecycle, sequence numbers, finish mapping,
//! usage-only frames, [DONE] handling, bounds, malformed frames, and the
//! single-terminal guarantee.

use super::*;
use serde_json::json;
use std::io::{Cursor, Read};
use std::sync::Mutex as StdMutex;

fn run_reader(frames: &[&str]) -> (String, UpstreamResponseUsage) {
    let usage_collector = Arc::new(StdMutex::new(UpstreamResponseUsage::default()));
    let mut input = String::new();
    for frame in frames {
        input.push_str("data: ");
        input.push_str(frame);
        input.push_str("\n\n");
    }
    let mut reader = ResponsesFromChatCompletionsSseReader::from_reader(
        Cursor::new(input.into_bytes()),
        Arc::clone(&usage_collector),
        Some("fallback-model"),
        Instant::now(),
        None,
    );
    let mut output = Vec::new();
    reader.read_to_end(&mut output).expect("read reader output");
    let usage = usage_collector.lock().expect("usage lock").clone();
    (String::from_utf8_lossy(&output).to_string(), usage)
}

fn chunk(id: &str, model: &str, content: &str, finish: Option<&str>) -> String {
    json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": 1710000000,
        "model": model,
        "choices": [{
            "index": 0,
            "delta": { "content": content },
            "finish_reason": finish,
        }]
    })
    .to_string()
}

fn parse_events(output: &str) -> Vec<(String, Value)> {
    output
        .split("\n\n")
        .filter(|block| !block.trim().is_empty())
        .map(|block| {
            let mut event = String::new();
            let mut data = String::new();
            for line in block.lines() {
                if let Some(value) = line.strip_prefix("event:") {
                    event = value.trim().to_string();
                } else if let Some(value) = line.strip_prefix("data:") {
                    data.push_str(value.trim_start());
                }
            }
            let parsed = serde_json::from_str::<Value>(&data).unwrap_or(Value::Null);
            (event, parsed)
        })
        .collect()
}

#[test]
fn emits_text_lifecycle_with_sequence_numbers_and_terminal() {
    let (output, usage) = run_reader(&[
        &chunk("chatcmpl-1", "gpt-4o", "Hel", None),
        &chunk("chatcmpl-1", "gpt-4o", "lo", Some("stop")),
        "[DONE]",
    ]);
    let events = parse_events(&output);
    let names: Vec<&str> = events.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        [
            "response.created",
            "response.in_progress",
            "response.output_item.added",
            "response.content_part.added",
            "response.output_text.delta",
            "response.output_text.delta",
            "response.output_text.done",
            "response.content_part.done",
            "response.output_item.done",
            "response.completed",
        ]
    );
    // 单调递增 sequence_number
    let seqs: Vec<u64> = events
        .iter()
        .map(|(_, value)| value["sequence_number"].as_u64().expect("sequence_number"))
        .collect();
    assert_eq!(seqs, (1..=10).collect::<Vec<_>>());
    // delta 内容逐段累积
    let deltas: Vec<&str> = events
        .iter()
        .filter(|(name, _)| name == "response.output_text.delta")
        .map(|(_, value)| value["delta"].as_str().expect("delta"))
        .collect();
    assert_eq!(deltas, ["Hel", "lo"]);
    // 终态 completed + 完整输出
    let completed = events
        .iter()
        .find(|(name, _)| name == "response.completed")
        .expect("completed event");
    assert_eq!(completed.1["response"]["status"], "completed");
    assert_eq!(
        completed.1["response"]["output"][0]["content"][0]["text"],
        "Hel\nlo"
    );
    assert_eq!(completed.1["response"]["model"], "gpt-4o");
    assert_eq!(usage.output_text.as_deref(), Some("Hel\nlo"));
}

#[test]
fn streams_tool_calls_with_fragmented_arguments() {
    let tool_fragment = |index: usize, fragment: &str, finish: Option<&str>| {
        json!({
            "id": "chatcmpl-2",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "index": index,
                        "id": format!("call_{index}"),
                        "type": "function",
                        "function": { "name": "get_weather", "arguments": fragment }
                    }]
                },
                "finish_reason": finish,
            }]
        })
        .to_string()
    };
    let (output, _) = run_reader(&[
        &tool_fragment(0, "{\"city\":", None),
        &tool_fragment(0, "\"beijing\"}", Some("tool_calls")),
        "[DONE]",
    ]);
    let events = parse_events(&output);
    let names: Vec<&str> = events.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        [
            "response.created",
            "response.in_progress",
            "response.output_item.added",
            "response.function_call_arguments.delta",
            "response.function_call_arguments.delta",
            "response.function_call_arguments.done",
            "response.output_item.done",
            "response.completed",
        ]
    );
    let call = events
        .iter()
        .find(|(name, value)| {
            name == "response.output_item.done"
                && value["item"]["type"] == "function_call"
                && value["item"]["status"] == "completed"
        })
        .expect("completed function_call item");
    assert_eq!(call.1["item"]["name"], "get_weather");
    assert_eq!(call.1["item"]["call_id"], "fc_chatcmpl-2_0");
    // 参数分片合并并规范化为紧凑 JSON
    assert_eq!(call.1["item"]["arguments"], "{\"city\":\"beijing\"}");
    // 终态为 completed（tool_calls finish 映射）
    let completed = events
        .iter()
        .find(|(name, _)| name == "response.completed")
        .expect("completed");
    assert_eq!(completed.1["response"]["status"], "completed");
}

#[test]
fn maps_length_finish_to_incomplete() {
    let (output, _) = run_reader(&[
        &chunk("chatcmpl-3", "gpt-4o", "partial", Some("length")),
        "[DONE]",
    ]);
    let events = parse_events(&output);
    let incomplete = events
        .iter()
        .find(|(name, _)| name == "response.incomplete")
        .expect("incomplete event");
    assert_eq!(incomplete.1["response"]["status"], "incomplete");
    assert_eq!(incomplete.1["incomplete_details"]["reason"], "max_output_tokens");
    assert!(!events.iter().any(|(name, _)| name == "response.completed"));
}

#[test]
fn content_filter_is_terminal_error_event() {
    let (output, _) = run_reader(&[&chunk("chatcmpl-4", "gpt-4o", "", Some("content_filter")), "[DONE]"]);
    let events = parse_events(&output);
    let error_event = events
        .iter()
        .find(|(name, _)| name == "error")
        .expect("error event");
    assert_eq!(error_event.1["error"]["code"], "upstream_content_filter");
    assert!(!events.iter().any(|(name, _)| name == "response.completed"));
}

#[test]
fn absorbs_usage_only_frame_and_publishes_usage() {
    let usage_frame = json!({
        "id": "chatcmpl-5",
        "model": "gpt-4o",
        "choices": [],
        "usage": {
            "prompt_tokens": 12,
            "completion_tokens": 7,
            "total_tokens": 19,
            "completion_tokens_details": { "reasoning_tokens": 3 }
        }
    })
    .to_string();
    let (output, usage) = run_reader(&[
        &chunk("chatcmpl-5", "gpt-4o", "done", Some("stop")),
        &usage_frame,
        "[DONE]",
    ]);
    let events = parse_events(&output);
    assert!(!events.iter().any(|(name, value)| {
        name == "response.output_text.delta" && value["delta"].as_str() == Some(&usage_frame)
    }));
    assert_eq!(usage.input_tokens, Some(12));
    assert_eq!(usage.output_tokens, Some(7));
    assert_eq!(usage.total_tokens, Some(19));
    assert_eq!(usage.reasoning_output_tokens, Some(3));
    // 终态响应 usage 可见
    let completed = events
        .iter()
        .find(|(name, _)| name == "response.completed")
        .expect("completed");
    assert_eq!(completed.1["response"]["usage"]["input_tokens"], 12);
    assert_eq!(completed.1["response"]["usage"]["output_tokens"], 7);
}

#[test]
fn emits_reasoning_deltas_before_text() {
    let reasoning_chunk = |fragment: &str, finish: Option<&str>| {
        json!({
            "id": "chatcmpl-6",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "delta": { "reasoning_content": fragment },
                "finish_reason": finish,
            }]
        })
        .to_string()
    };
    let (output, _) = run_reader(&[
        &reasoning_chunk("think", None),
        &chunk("chatcmpl-6", "gpt-4o", "answer", Some("stop")),
        "[DONE]",
    ]);
    let events = parse_events(&output);
    let names: Vec<&str> = events.iter().map(|(name, _)| name.as_str()).collect();
    assert!(names.contains(&"response.reasoning_summary_text.added"));
    let reasoning_delta = events
        .iter()
        .find(|(name, _)| name == "response.reasoning_summary_text.delta")
        .expect("reasoning delta");
    assert_eq!(reasoning_delta.1["delta"], "think");
    let reasoning_done = events
        .iter()
        .find(|(name, _)| name == "response.reasoning_summary_text.done")
        .expect("reasoning done");
    assert_eq!(reasoning_done.1["text"], "think");
}

#[test]
fn eof_without_done_is_incomplete_error() {
    let (output, _) = run_reader(&[&chunk("chatcmpl-7", "gpt-4o", "partial", None)]);
    let events = parse_events(&output);
    let error_event = events
        .iter()
        .find(|(name, _)| name == "error")
        .expect("error event");
    assert_eq!(error_event.1["error"]["code"], "upstream_chat_stream_incomplete");
    assert!(!events.iter().any(|(name, _)| name == "response.completed"));
}

#[test]
fn malformed_frame_is_skipped_not_fatal() {
    let (output, _) = run_reader(&[
        "not-json-{broken",
        &chunk("chatcmpl-8", "gpt-4o", "ok", Some("stop")),
        "[DONE]",
    ]);
    let events = parse_events(&output);
    let completed = events
        .iter()
        .find(|(name, _)| name == "response.completed")
        .expect("completed");
    assert_eq!(completed.1["response"]["output"][0]["content"][0]["text"], "ok");
}

#[test]
fn tool_index_beyond_limit_triggers_terminal_error() {
    let tool_chunk = json!({
        "id": "chatcmpl-9",
        "model": "gpt-4o",
        "choices": [{
            "index": 0,
            "delta": { "tool_calls": [{ "index": MAX_CHAT_TOOL_CALLS, "function": { "arguments": "{}" } }] },
            "finish_reason": null,
        }]
    })
    .to_string();
    let (output, _) = run_reader(&[&tool_chunk, "[DONE]"]);
    let events = parse_events(&output);
    let error_event = events
        .iter()
        .find(|(name, _)| name == "error")
        .expect("error event");
    assert_eq!(error_event.1["error"]["code"], "upstream_chat_stream_error");
    assert!(!events.iter().any(|(name, _)| name == "response.completed"));
}

#[test]
fn empty_done_emits_completed_with_empty_output() {
    let (output, _) = run_reader(&["[DONE]"]);
    let events = parse_events(&output);
    let completed = events
        .iter()
        .find(|(name, _)| name == "response.completed")
        .expect("completed");
    assert_eq!(completed.1["response"]["status"], "completed");
    assert_eq!(completed.1["response"]["output"].as_array().unwrap().len(), 0);
}

#[test]
fn completed_stream_caches_assistant_context() {
    let usage_collector = Arc::new(StdMutex::new(UpstreamResponseUsage::default()));
    let context_store = Arc::new(ChatCompletionsContextStore::new());
    let frames = [
        chunk("chatcmpl_context", "gpt-4o", "answer", Some("stop")),
        "[DONE]".to_string(),
    ];
    let input = frames
        .iter()
        .map(|frame| format!("data: {frame}\n\n"))
        .collect::<String>();
    let mut reader = ResponsesFromChatCompletionsSseReader::from_reader(
        Cursor::new(input.into_bytes()),
        usage_collector,
        Some("fallback-model"),
        Instant::now(),
        Some(Arc::clone(&context_store)),
    );
    let mut output = Vec::new();
    reader.read_to_end(&mut output).expect("read reader output");

    let messages = context_store
        .lookup(None, "chatcmpl_context")
        .expect("completed stream cached");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "assistant");
    assert_eq!(messages[0]["content"], "answer");
}

#[test]
fn incomplete_stream_does_not_cache_context() {
    let usage_collector = Arc::new(StdMutex::new(UpstreamResponseUsage::default()));
    let context_store = Arc::new(ChatCompletionsContextStore::new());
    let input = format!("data: {}\n\n", chunk("chatcmpl_incomplete", "gpt-4o", "partial", None));
    let mut reader = ResponsesFromChatCompletionsSseReader::from_reader(
        Cursor::new(input.into_bytes()),
        usage_collector,
        Some("fallback-model"),
        Instant::now(),
        Some(Arc::clone(&context_store)),
    );
    let mut output = Vec::new();
    reader.read_to_end(&mut output).expect("read reader output");

    assert!(context_store.lookup(None, "chatcmpl_incomplete").is_none());
}
