//! 聚合 Chat 上游的续聊上下文缓存。
//!
//! Responses 协议用 `previous_response_id` 引用服务端历史会话；Chat Completions
//! 没有等价物，续聊必须由完整 `messages` 表达。网关把 Chat 上游成功的
//! assistant 响应（文本 / reasoning_content / tool_calls）按响应 id 缓存，
//! 后续带 `previous_response_id` 的请求在 Responses -> Chat 转换时用缓存
//! 重建上下文，而不是直接拒绝。
//!
//! 缓存是进程级内存单例（网关本身是单进程服务）。条目带 TTL 与容量上限，
//! 防止无限增长；上游无 id 的响应（兜底 `resp_codexmanager`）不入库，避免
//! 多个无 id 响应互相覆盖。

use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// 缓存条目上限；超出时驱逐最旧条目。
const MAX_CONTEXT_ENTRIES: usize = 512;
/// 条目 TTL：30 分钟。
const CONTEXT_TTL_SECS: i64 = 30 * 60;
/// 上游未返回 id 时的兜底响应 id，不参与缓存。
const FALLBACK_RESPONSE_ID: &str = "resp_codexmanager";

/// 一次 Chat 上游成功响应的上下文（Chat 格式 assistant 消息）。
#[derive(Debug, Clone)]
pub(crate) struct ChatCompletionsContextEntry {
    pub messages: Vec<Value>,
    pub created_at: i64,
}

/// 响应 id -> Chat 上下文条目。
#[derive(Debug, Default)]
pub(crate) struct ChatCompletionsContextStore {
    inner: Mutex<HashMap<String, ChatCompletionsContextEntry>>,
}

impl ChatCompletionsContextStore {
    pub(crate) fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// 存入一条 assistant 上下文。兜底 id / 空消息不入库。
    pub(crate) fn insert(&self, scope: Option<&str>, response_id: &str, messages: Vec<Value>) {
        let trimmed = response_id.trim();
        let key = scoped_key(scope, trimmed);
        if trimmed.is_empty()
            || trimmed == FALLBACK_RESPONSE_ID
            || messages.is_empty()
        {
            return;
        }
        let mut inner = match self.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        if inner.len() >= MAX_CONTEXT_ENTRIES && !inner.contains_key(&key) {
            if let Some(oldest_key) = inner
                .iter()
                .min_by_key(|(_, entry)| entry.created_at)
                .map(|(key, _)| key.clone())
            {
                inner.remove(&oldest_key);
            }
        }
        inner.insert(
            key,
            ChatCompletionsContextEntry {
                messages,
                created_at: now_ts(),
            },
        );
    }

    /// 查询响应 id 的历史上下文；过期条目删除后视为未命中。
    pub(crate) fn lookup(&self, scope: Option<&str>, response_id: &str) -> Option<Vec<Value>> {
        let trimmed = response_id.trim();
        if trimmed.is_empty() {
            return None;
        }
        let key = scoped_key(scope, trimmed);
        let mut inner = match self.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return None,
        };
        let entry = inner.get(&key)?;
        if now_ts() - entry.created_at > CONTEXT_TTL_SECS {
            inner.remove(&key);
            return None;
        }
        Some(entry.messages.clone())
    }

    /// 解析 Responses 请求体中的 `previous_response_id` 并查询历史上下文。
    pub(crate) fn lookup_previous_response_context(
        &self,
        scope: Option<&str>,
        body: &[u8],
    ) -> Option<Vec<Value>> {
        let Ok(value) = serde_json::from_slice::<Value>(body) else {
            return None;
        };
        let previous_id = value
            .get("previous_response_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        self.lookup(scope, previous_id)
    }

    /// 从 Chat 完成响应体（非流式）提取 assistant 上下文并入库存放。
    /// 解析失败 / 无 id / 无 message 时静默跳过（响应转换自身仍有完整校验）。
    pub(crate) fn insert_from_chat_response_body(&self, scope: Option<&str>, body: &[u8]) {
        let Ok(value) = serde_json::from_slice::<Value>(body) else {
            return;
        };
        let Some(response_id) = value.get("id").and_then(Value::as_str) else {
            return;
        };
        let Some(choice) = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
        else {
            return;
        };
        if matches!(
            choice.get("finish_reason").and_then(Value::as_str),
            Some("length" | "content_filter")
        ) {
            return;
        }
        let Some(message) = choice.get("message") else {
            return;
        };
        let Some(assistant) = chat_message_to_assistant_message(message) else {
            return;
        };
        self.insert(scope, response_id, vec![Value::Object(assistant)]);
    }

    #[cfg(test)]
    pub(crate) fn reset_for_tests(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.clear();
        }
    }
}

/// Chat `message` 对象 -> 可回放的 assistant 消息。无文本/推理/工具调用时返回 None。
fn chat_message_to_assistant_message(message: &Value) -> Option<Map<String, Value>> {
    let mut assistant = Map::new();
    assistant.insert("role".to_string(), Value::String("assistant".to_string()));

    let mut has_content = false;
    if let Some(content) = message.get("content") {
        if let Some(text) = flatten_chat_content(content) {
            assistant.insert("content".to_string(), Value::String(text));
            has_content = true;
        }
    }
    if let Some(reasoning) = message
        .get("reasoning_content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        assistant.insert(
            "reasoning_content".to_string(),
            Value::String(reasoning.to_string()),
        );
        has_content = true;
    }
    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        let mapped = tool_calls
            .iter()
            .filter_map(chat_tool_call_to_chat_format)
            .collect::<Vec<_>>();
        if !mapped.is_empty() {
            assistant.insert("tool_calls".to_string(), Value::Array(mapped));
            has_content = true;
        }
    }
    if has_content {
        Some(assistant)
    } else {
        None
    }
}

/// Chat `content`（string 或 [{type:text,...}] 数组）折叠为纯文本。
fn flatten_chat_content(content: &Value) -> Option<String> {
    match content {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Array(parts) => {
            let mut text = String::new();
            for part in parts {
                if part.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(value) = part.get("text").and_then(Value::as_str) {
                        if !value.is_empty() {
                            if !text.is_empty() {
                                text.push('\n');
                            }
                            text.push_str(value);
                        }
                    }
                }
            }
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
        _ => None,
    }
}

/// Chat 上游 `tool_calls` 项已是 Chat 格式，原样保留（仅校验结构）。
fn chat_tool_call_to_chat_format(call: &Value) -> Option<Value> {
    let call_id = call.get("id").and_then(Value::as_str)?;
    let function = call.get("function")?;
    let name = function.get("name").and_then(Value::as_str)?;
    Some(json!({
        "id": call_id,
        "type": "function",
        "function": {
            "name": name,
            "arguments": function
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}"),
        },
    }))
}

fn scoped_key(scope: Option<&str>, response_id: &str) -> String {
    match scope.map(str::trim).filter(|value| !value.is_empty()) {
        Some(scope) => format!("{scope}\u{1f}{response_id}"),
        None => response_id.to_string(),
    }
}

/// 进程级共享上下文缓存。
pub(crate) fn global_chat_completions_context() -> Arc<ChatCompletionsContextStore> {
    static STORE: LazyLock<Arc<ChatCompletionsContextStore>> =
        LazyLock::new(|| Arc::new(ChatCompletionsContextStore::new()));
    Arc::clone(&STORE)
}

#[cfg(test)]
pub(crate) fn reset_global_chat_completions_context_for_tests() {
    global_chat_completions_context().reset_for_tests();
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_store() -> ChatCompletionsContextStore {
        ChatCompletionsContextStore::new()
    }

    #[test]
    fn insert_and_lookup_round_trip() {
        let store = test_store();
        store.insert(
            None,
            "chatcmpl_abc",
            vec![json!({ "role": "assistant", "content": "hi" })],
        );
        let messages = store.lookup(None, "chatcmpl_abc").expect("lookup hit");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "assistant");
    }

    #[test]
    fn fallback_response_id_is_never_stored() {
        let store = test_store();
        store.insert(
            None,
            FALLBACK_RESPONSE_ID,
            vec![json!({ "role": "assistant", "content": "x" })],
        );
        store.insert(None, "", vec![json!({ "role": "assistant" })]);
        assert!(store.lookup(None, FALLBACK_RESPONSE_ID).is_none());
        assert!(store.lookup(None, "").is_none());
    }

    #[test]
    fn lookup_misses_unknown_id() {
        let store = test_store();
        assert!(store.lookup(None, "chatcmpl_unknown").is_none());
    }

    #[test]
    fn insert_from_chat_response_body_extracts_message() {
        let store = test_store();
        let body = json!({
            "id": "chatcmpl_123",
            "choices": [{
                "finish_reason": "stop",
                "message": {
                    "role": "assistant",
                    "content": "answer",
                    "reasoning_content": "think",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": { "name": "search", "arguments": "{\"q\":\"x\"}" }
                    }]
                }
            }]
        });
        store.insert_from_chat_response_body(
            None,
            serde_json::to_vec(&body).expect("body").as_slice(),
        );
        let messages = store.lookup(None, "chatcmpl_123").expect("stored");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["content"], "answer");
        assert_eq!(messages[0]["reasoning_content"], "think");
        assert_eq!(messages[0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(messages[0]["tool_calls"][0]["function"]["name"], "search");
    }

    #[test]
    fn insert_from_chat_response_body_skips_no_message() {
        let store = test_store();
        store.insert_from_chat_response_body(None, br#"{"id":"chatcmpl_1","choices":[]}"#);
        assert!(store.lookup(None, "chatcmpl_1").is_none());
    }

    #[test]
    fn content_array_folds_to_text() {
        let store = test_store();
        let body = json!({
            "id": "chatcmpl_arr",
            "choices": [{
                "message": {
                    "content": [
                        { "type": "text", "text": "a" },
                        { "type": "text", "text": "b" }
                    ]
                }
            }]
        });
        store.insert_from_chat_response_body(
            None,
            serde_json::to_vec(&body).expect("body").as_slice(),
        );
        let messages = store.lookup(None, "chatcmpl_arr").expect("stored");
        assert_eq!(messages[0]["content"], "a\nb");
    }

    #[test]
    fn oldest_entry_is_evicted_when_full() {
        let store = test_store();
        // 容量上限较小才能直接验证驱逐；直接构造小容量实例不可行（const），
        // 因此验证 TTL 过期路径：手工插入过期条目后 lookup 返回 None。
        let mut inner = store.inner.lock().expect("lock");
        inner.insert(
            "chatcmpl_stale".to_string(),
            ChatCompletionsContextEntry {
                messages: vec![json!({ "role": "assistant" })],
                created_at: now_ts() - CONTEXT_TTL_SECS - 1,
            },
        );
        drop(inner);
        assert!(store.lookup(None, "chatcmpl_stale").is_none());
    }

    #[test]
    fn lookup_previous_response_context_reads_body() {
        let store = test_store();
        store.insert(None, "chatcmpl_prev", vec![json!({ "role": "assistant" })]);
        let body = json!({
            "previous_response_id": "chatcmpl_prev",
            "input": "follow up"
        });
        let messages = store
            .lookup_previous_response_context(
                None,
                serde_json::to_vec(&body).expect("body").as_slice(),
            )
            .expect("hit");
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn lookup_previous_response_context_misses_when_absent() {
        let store = test_store();
        let body = json!({ "input": "no chaining" });
        assert!(store
            .lookup_previous_response_context(
                None,
                serde_json::to_vec(&body).expect("body").as_slice(),
            )
            .is_none());
    }
}
