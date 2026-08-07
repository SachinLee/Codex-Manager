from pathlib import Path
import re
p = Path('crates/service/src/gateway/observability/http_bridge/delivery.rs')
t = p.read_text(encoding='utf-8')
new = '''fn resolve_stream_keepalive_frame(
    response_adapter: ResponseAdapter,
    _request_path: &str,
) -> SseKeepAliveFrame {
    match response_adapter {
        // Prefer protocol-aware keepalive when available; fall back to SSE comments
        // so image generation and long-lived streams stay alive.
        ResponseAdapter::ResponsesFromAnthropicMessages => SseKeepAliveFrame::Anthropic,
        ResponseAdapter::Passthrough
        | ResponseAdapter::AnthropicMessagesFromResponses
        | ResponseAdapter::ChatCompletionsFromResponses
        | ResponseAdapter::CompactFromChatCompletions
        | ResponseAdapter::ImagesB64JsonFromResponses
        | ResponseAdapter::ImagesUrlFromResponses
        | ResponseAdapter::GeminiJson
        | ResponseAdapter::GeminiCliJson
        | ResponseAdapter::GeminiSse
        | ResponseAdapter::GeminiCliSse => SseKeepAliveFrame::Comment,
    }
}'''
m = re.search(r'fn resolve_stream_keepalive_frame\([\s\S]*?\n\}', t)
if not m:
    raise SystemExit('helper not found')
t = t[:m.start()] + new + t[m.end():]
p.write_text(t, encoding='utf-8')
print('fixed keepalive helper')
