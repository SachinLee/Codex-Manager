use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResponseAdapter {
    Passthrough,
    AnthropicMessagesFromResponses,
    ResponsesFromAnthropicMessages,
    ResponsesFromChatCompletions,
    ChatCompletionsFromResponses,
    #[allow(dead_code)]
    CompactFromChatCompletions,
    ImagesB64JsonFromResponses,
    ImagesUrlFromResponses,
    GeminiJson,
    GeminiSse,
    GeminiCliJson,
    GeminiCliSse,
}

/// 上游（candidate）实际使用的 wire 协议，与客户端协议（`ResponseAdapter`）正交。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpstreamProtocol {
    Responses,
    ChatCompletions,
    /// 遗留 Claude 桥接：Responses 请求转换为 Anthropic Messages 上游。
    AnthropicMessages,
}

impl UpstreamProtocol {
    pub(crate) fn label(self) -> &'static str {
        match self {
            UpstreamProtocol::Responses => "responses",
            UpstreamProtocol::ChatCompletions => "chat_completions",
            UpstreamProtocol::AnthropicMessages => "anthropic_messages",
        }
    }
}

/// 两轴响应计划：上游解码协议 + 客户端编码协议 + 工具名恢复映射。
/// 在 Aggregate 执行中取代单一 `bridge_responses_to_anthropic` 布尔轴。
#[derive(Debug, Clone)]
pub(crate) struct ResponsePlan {
    pub(crate) upstream_protocol: UpstreamProtocol,
    pub(crate) client_encoder: ResponseAdapter,
    pub(crate) tool_name_restore_map: ToolNameRestoreMap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum GeminiStreamOutputMode {
    Sse,
    Raw,
}

pub(crate) type ToolNameRestoreMap = BTreeMap<String, String>;

#[derive(Debug)]
pub(crate) struct AdaptedGatewayRequest {
    pub(crate) path: String,
    pub(crate) body: Vec<u8>,
    pub(crate) response_adapter: ResponseAdapter,
    pub(crate) gemini_stream_output_mode: Option<GeminiStreamOutputMode>,
    pub(crate) tool_name_restore_map: ToolNameRestoreMap,
}
