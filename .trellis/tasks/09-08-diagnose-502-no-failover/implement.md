# 实现计划：Aggregate API SSE 终态错误分类与候选轮转

## 注意
**本任务为诊断与规划任务，不包含产品代码实现。**

本文档描述假设的实现切片，供后续实现任务参考。实际实现需要独立的 `in_progress` 任务和完整的测试验证。

---

## Slice 1: 添加 SSE 终态错误码提取辅助函数

### AC-001: 从 SSE 终态错误消息中提取 error.code

**行为**：新增 `extract_error_code_from_terminal()` 函数，从 SSE `stream_terminal_error` 字符串中提取结构化 `error.code`。

**代码边界**：
- 文件：`crates/service/src/gateway/upstream/support/upstream_failure.rs`
- 位置：`error_code_from_response_body()` 函数后（约 line 100）

**测试边界**：
- 文件：`crates/service/src/gateway/upstream/support/upstream_failure.rs` 内联测试
- 公共接口：`pub(in crate::gateway) fn extract_error_code_from_terminal(message: Option<&str>) -> Option<String>`

**RED**：
```rust
#[test]
fn extract_error_code_from_sse_terminal_parses_json() {
    let json_error = r#"{"error":{"code":"rate_limit_exceeded","message":"Too many requests"}}"#;
    assert_eq!(
        extract_error_code_from_terminal(Some(json_error)),
        Some("rate_limit_exceeded".to_string())
    );
}

#[test]
fn extract_error_code_from_sse_terminal_parses_prefixed_message() {
    // extract_message_from_error_map 生成的格式
    let prefixed = "code=upstream_error type=server_error upstream exploded";
    assert_eq!(
        extract_error_code_from_terminal(Some(prefixed)),
        Some("upstream_error".to_string())
    );
}

#[test]
fn extract_error_code_from_sse_terminal_returns_none_for_plain_text() {
    assert_eq!(
        extract_error_code_from_terminal(Some("connection refused")),
        None
    );
}

#[test]
fn extract_error_code_from_sse_terminal_handles_empty() {
    assert_eq!(extract_error_code_from_terminal(None), None);
    assert_eq!(extract_error_code_from_terminal(Some("")), None);
}
```

**实现**：
```rust
/// 从 SSE 终态错误消息中提取 error.code（若有）。
///
/// # 参数
/// - `message`: SSE `stream_terminal_error` 字符串
///
/// # 返回
/// 若消息包含结构化 `error.code`，返回 Some(code)；否则 None
pub(in crate::gateway) fn extract_error_code_from_terminal(message: Option<&str>) -> Option<String> {
    let message = message?.trim();
    if message.is_empty() {
        return None;
    }

    // 1. 尝试 JSON 解析：复用 error_code_from_response_body
    if let Ok(value) = serde_json::from_str::<Value>(message) {
        return error_code_from_response_body(message.as_bytes());
    }

    // 2. 前缀匹配：`code=rate_limit_exceeded ...`
    //    这是 extract_message_from_error_map 的输出格式（output_text.rs:768）
    if let Some(rest) = message.strip_prefix("code=") {
        if let Some(code) = rest.split_whitespace().next() {
            return Some(code.to_string());
        }
    }

    None
}
```

**GREEN**：
```bash
cargo test --package codexmanager-service --lib gateway::upstream::support::upstream_failure::tests::extract_error_code_from_sse_terminal
```

**验证**：
- 4 个单元测试全部通过
- 函数复用现有 `error_code_from_response_body()` 逻辑，不重复实现 JSON 解析

---

## Slice 2: SSE 终态分类核心逻辑 - CandidateFailover 路径

### AC-002: HTTP 200 + SSE rate_limit_exceeded 切换到下一候选

**行为**：当 Aggregate API 返回 `HTTP 200 + {"type":"response.failed","error":{"code":"rate_limit_exceeded"}}`，且零交付时，应调用 `classify_upstream_failure()` 并执行候选 failover。

**代码边界**：
- 文件：`crates/service/src/gateway/upstream/protocol/aggregate_api.rs`
- 插入位置：line 3208（capacity recovery 逻辑之后、`succeeded=true` 赋值之前）

**测试边界**：
- 文件：`crates/service/src/gateway/upstream/protocol/aggregate_api_tests.rs`
- 新增测试：`sse_rate_limit_fails_over_to_next_candidate()`

**RED**：
```rust
#[test]
fn sse_rate_limit_fails_over_to_next_candidate() {
    // 模拟 Aggregate Responses SSE 终态错误：
    // HTTP 200 + text/event-stream，但立即发送 response.failed 终态事件
    const RATE_LIMIT_SSE: &str = concat!(
        "data: {\"type\":\"response.failed\",\"error\":{\"code\":\"rate_limit_exceeded\",\"message\":\"Upstream rate limit\"}}\n",
        "\n"
    );
    
    let (storage, outcome, request_count) = run_aggregate_sse_scenario(
        "sse-rate-limit-failover",
        vec![
            (200, "text/event-stream", RATE_LIMIT_SSE),
            (200, "application/json", CAPACITY_OK_JSON),
        ],
        vec![
            aggregate_capacity_test_candidate("agg-sse-first", ""),
            aggregate_capacity_test_candidate("agg-sse-second", ""),
        ],
        true,  // is_stream=true
        None,
    );
    
    assert_eq!(
        request_count, 2,
        "SSE rate_limit_exceeded with zero delivery must fail over to next candidate"
    );
    assert!(matches!(outcome, AggregateAttemptOutcome::Responded { .. }));
    assert_eq!(
        terminal_request_log_status(&storage, "trc-sse-rate-limit-failover"),
        Some(200)
    );
}
```

**实现**：
```rust
// aggregate_api.rs:3208 插入

// SSE 终态错误的候选决策（零交付场景）
if !bridge_ok && bridge.pending_failover_request.is_some() {
    // 请求可归还：bridge delivery 层确认零交付，无语义内容已发送给客户端
    let error_message = final_error
        .as_deref()
        .unwrap_or("aggregate api upstream response incomplete");
    
    // 从 SSE 终态错误中提取 error.code（若有）
    let error_code = super::super::support::upstream_failure::extract_error_code_from_terminal(
        bridge.stream_terminal_error.as_deref()
    );
    
    // 统一分类决策
    let decision = super::super::support::upstream_failure::classify_upstream_failure(
        502,  // SSE 终态统一视为 502 客户端错误
        error_message,
        error_code.as_deref(),
        false,  // pending_failover_request 保证未交付
        false,  // 非推理守卫路径
    );
    
    match decision {
        super::super::support::upstream_failure::UpstreamFailureDecision::CandidateFailover => {
            // rate_limit_exceeded 等候选级错误：切到下一候选
            request = bridge.pending_failover_request.take();
            last_attempt_error = Some(error_message.to_string());
            last_failure_status = 502;
            cooldown_eligible_failure = true;
            // 零交付失败：释放预留的 spend attempt
            release_daily_spend_attempt(storage, &mut current_attempt_id, trace_id);
            break;  // 退出内层候选尝试循环，外层继续下一候选
        }
        _ => {
            // 其他决策分支稍后实现
        }
    }
}
```

**GREEN**：
```bash
cargo test --package codexmanager-service --lib gateway::upstream::protocol::aggregate_api_tests::sse_rate_limit_fails_over_to_next_candidate
```

**验证**：
- 新测试通过
- 现有 HTTP 502 JSON failover 测试（`aggregate_502_rate_limit_exceeded_code_fails_over_to_next_candidate`）保持通过
- `request_count=2` 证明两个候选都被尝试
- 终态 status=200 证明第二候选成功

---

## Slice 3: 同候选重试路径

### AC-003: HTTP 200 + SSE server_error 使用同候选重试预算

**行为**：当 SSE 终态错误不是 `rate_limit_exceeded`，应归类为 `RetrySameCandidate`，消耗同候选传输重试预算（最多 3 次）。

**代码边界**：
- 文件：`crates/service/src/gateway/upstream/protocol/aggregate_api.rs`
- 位置：Slice 2 的 `match decision` 添加 `RetrySameCandidate` 分支

**测试边界**：
- 文件：`crates/service/src/gateway/upstream/protocol/aggregate_api_tests.rs`
- 新增测试：`sse_server_error_keeps_same_candidate_transport_budget()`

**RED**：
```rust
#[test]
fn sse_server_error_keeps_same_candidate_transport_budget() {
    const SERVER_ERROR_SSE: &str = concat!(
        "data: {\"type\":\"response.failed\",\"error\":{\"code\":\"server_error\",\"message\":\"Internal error\"}}\n",
        "\n"
    );
    
    let (storage, outcome, request_count) = run_aggregate_sse_scenario(
        "sse-server-error-budget",
        vec![
            (200, "text/event-stream", SERVER_ERROR_SSE),
            (200, "text/event-stream", SERVER_ERROR_SSE),
            (200, "text/event-stream", SERVER_ERROR_SSE),
            (200, "text/event-stream", SERVER_ERROR_SSE),
        ],
        vec![aggregate_capacity_test_candidate("agg-sse-retry", "")],
        true,
        None,
    );
    
    assert_eq!(
        request_count, 4,
        "SSE server_error keeps the initial request plus 3-attempt transport budget"
    );
    assert!(matches!(outcome, AggregateAttemptOutcome::AllFailed { .. }));
}
```

**实现**：
```rust
match decision {
    super::super::support::upstream_failure::UpstreamFailureDecision::CandidateFailover => {
        // ... (已实现)
    }
    super::super::support::upstream_failure::UpstreamFailureDecision::RetrySameCandidate => {
        // 同候选传输重试：5xx、连接错误等可能瞬时恢复的故障
        if transport_retry_budget_remaining > 0 {
            request = bridge.pending_failover_request.take();
            transport_retry_budget_remaining = transport_retry_budget_remaining.saturating_sub(1);
            next_attempt_kind = SPEND_ATTEMPT_KIND_TRANSPORT_RETRY;
            // 零交付失败：释放当前 spend attempt
            release_daily_spend_attempt(storage, &mut current_attempt_id, trace_id);
            // 添加退避（与 HTTP 非成功分支一致）
            let retry_attempt = AGGREGATE_API_RETRY_ATTEMPTS_PER_CHANNEL - transport_retry_budget_remaining;
            let backoff_ms = 50u64 * 2u64.pow(retry_attempt as u32);
            log::debug!(
                "event=aggregate_api_sse_retry_backoff candidate_id={} retry_attempt={} backoff_ms={}",
                candidate_id, retry_attempt, backoff_ms
            );
            std::thread::sleep(std::time::Duration::from_millis(backoff_ms));
            continue;  // 内层循环继续同候选重试
        }
        // 预算耗尽：候选失败，外层继续下一候选
        request = bridge.pending_failover_request.take();
        last_attempt_error = Some(error_message.to_string());
        last_failure_status = 502;
        cooldown_eligible_failure = true;
        release_daily_spend_attempt(storage, &mut current_attempt_id, trace_id);
        break;
    }
    _ => {
        // RequestTerminal 等其他分支稍后实现
    }
}
```

**GREEN**：
```bash
cargo test --package codexmanager-service --lib gateway::upstream::protocol::aggregate_api_tests::sse_server_error_keeps_same_candidate_transport_budget
```

**验证**：
- 测试通过，`request_count=4`（1 初始 + 3 重试）
- 现有 HTTP 500 传输重试测试（`aggregate_500_keeps_existing_same_candidate_transport_budget`）保持通过

---

## Slice 4: 请求级终止路径

### AC-004: HTTP 200 + SSE invalid_request 立即终止

**行为**：当 SSE 终态为请求级错误（400/422 类），应归类为 `RequestTerminal`，设置 `terminal_failure=true`，不尝试后续候选或重试。

**代码边界**：
- 文件：`crates/service/src/gateway/upstream/protocol/aggregate_api.rs`
- 位置：`match decision` 添加 `RequestTerminal` 和默认分支

**测试边界**：
- 新增测试：`sse_invalid_request_terminates_immediately()`

**RED**：
```rust
#[test]
fn sse_invalid_request_terminates_immediately() {
    const INVALID_REQUEST_SSE: &str = concat!(
        "data: {\"type\":\"response.failed\",\"error\":{\"code\":\"invalid_request\",\"message\":\"Bad prompt\"}}\n",
        "\n"
    );
    
    let (storage, outcome, request_count) = run_aggregate_sse_scenario(
        "sse-invalid-request-terminal",
        vec![(200, "text/event-stream", INVALID_REQUEST_SSE)],
        vec![
            aggregate_capacity_test_candidate("agg-sse-a", ""),
            aggregate_capacity_test_candidate("agg-sse-b", ""),
        ],
        true,
        None,
    );
    
    assert_eq!(
        request_count, 1,
        "SSE invalid_request must terminate without trying next candidate"
    );
    assert!(matches!(outcome, AggregateAttemptOutcome::AllFailed { .. }));
}
```

**实现**：
```rust
match decision {
    super::super::support::upstream_failure::UpstreamFailureDecision::CandidateFailover => {
        // ... (已实现)
    }
    super::super::support::upstream_failure::UpstreamFailureDecision::RetrySameCandidate => {
        // ... (已实现)
    }
    super::super::support::upstream_failure::UpstreamFailureDecision::RequestTerminal => {
        // 请求级终止：400/422 等不可由重试或切换候选修复的错误
        last_attempt_error = Some(error_message.to_string());
        last_failure_status = 502;
        terminal_failure = true;
        release_daily_spend_attempt(storage, &mut current_attempt_id, trace_id);
        break;
    }
    super::super::support::upstream_failure::UpstreamFailureDecision::CapacityRecovery
    | super::super::support::upstream_failure::UpstreamFailureDecision::CapabilityRetry
    | super::super::support::upstream_failure::UpstreamFailureDecision::ReasoningGuardRetry => {
        // 这些决策已在前方独立处理（capacity: 3146-3207; reasoning: 3028-3136）
        // 不应在 SSE 终态路径出现；保守视为候选失败
        log::warn!(
            "event=aggregate_api_sse_unexpected_decision trace_id={} decision={:?}",
            trace_id, decision
        );
        request = bridge.pending_failover_request.take();
        last_attempt_error = Some(error_message.to_string());
        last_failure_status = 502;
        cooldown_eligible_failure = true;
        release_daily_spend_attempt(storage, &mut current_attempt_id, trace_id);
        break;
    }
}
```

**GREEN**：
```bash
cargo test --package codexmanager-service --lib gateway::upstream::protocol::aggregate_api_tests::sse_invalid_request_terminates_immediately
```

**验证**：
- 测试通过，`request_count=1`
- `terminal_failure=true` 阻止外层候选循环继续

---

## Slice 5: 已交付场景的不变性验证

### AC-005: 已交付的 SSE 终态错误不重放

**行为**：当客户端已收到部分 SSE 输出后出现 `response.failed`，`pending_failover_request=None`，应跳过分类逻辑，维持 `succeeded=true`。

**代码边界**：
- 验证：现有逻辑（`if !bridge_ok && bridge.pending_failover_request.is_some()`）已自然排除
- 无需新增代码，仅需回归测试

**测试边界**：
- 新增测试：`sse_error_after_delivery_does_not_retry()`

**RED**：
```rust
#[test]
fn sse_error_after_delivery_does_not_retry() {
    // 模拟已交付 output token 后的终态错误
    const PARTIAL_THEN_ERROR_SSE: &str = concat!(
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hello\"}\n",
        "\n",
        "data: {\"type\":\"response.failed\",\"error\":{\"code\":\"upstream_error\",\"message\":\"Lost connection\"}}\n",
        "\n"
    );
    
    let (storage, outcome, request_count) = run_aggregate_sse_scenario(
        "sse-error-after-delivery",
        vec![(200, "text/event-stream", PARTIAL_THEN_ERROR_SSE)],
        vec![
            aggregate_capacity_test_candidate("agg-partial-a", ""),
            aggregate_capacity_test_candidate("agg-partial-b", ""),
        ],
        true,
        None,
    );
    
    assert_eq!(
        request_count, 1,
        "SSE error after delivery must not retry or failover"
    );
    assert!(matches!(outcome, AggregateAttemptOutcome::Responded { .. }));
    
    // 验证记录为失败（affinity_binding_success=false）
    let log = terminal_request_log(&storage, "trc-sse-error-after-delivery");
    assert!(log.error_message.is_some());
    assert_eq!(log.output_tokens.unwrap_or(0) > 0, true, "output tokens delivered");
}
```

**实现**：
无需修改。现有条件 `bridge.pending_failover_request.is_some()` 已排除此场景。

**GREEN**：
```bash
cargo test --package codexmanager-service --lib gateway::upstream::protocol::aggregate_api_tests::sse_error_after_delivery_does_not_retry
```

**验证**：
- 测试通过
- `pending_failover_request=None` → 不进入分类逻辑
- `succeeded=true` → 记录 attempt 为 `Responded`，但 `affinity_binding_success=false`

---

## Slice 6: 回归与集成验证

### AC-006: 现有测试保持通过

**验证清单**：
1. **HTTP 非成功 JSON 路径**：
   - `aggregate_502_rate_limit_exceeded_code_fails_over_to_next_candidate` ✓
   - `aggregate_500_keeps_existing_same_candidate_transport_budget` ✓

2. **Chat preflight 路径**：
   - 所有 `preflight_chat_stream` 相关测试 ✓

3. **容量恢复路径**：
   - `aggregate_capacity_error_retries_with_backoff` ✓
   - `aggregate_capacity_retry_respects_deadline` ✓

4. **推理守卫路径**：
   - `reasoning_guard_internal_retry_*` 系列测试 ✓

5. **零候选/全冷却路径**：
   - 验证 `upstream_url=NULL` 的 503 终态不受影响

**GREEN**：
```bash
cargo test --package codexmanager-service --lib gateway::upstream::protocol::aggregate_api_tests
```

**验证**：
- 所有现有测试通过
- 无意外行为回归

---

## Slice 7: 文档与日志

### AC-007: 添加关键路径日志

**行为**：在分类决策点添加 trace 日志，便于生产诊断。

**实现**：
```rust
if !bridge_ok && bridge.pending_failover_request.is_some() {
    let error_message = final_error.as_deref().unwrap_or("aggregate api upstream response incomplete");
    let error_code = super::super::support::upstream_failure::extract_error_code_from_terminal(
        bridge.stream_terminal_error.as_deref()
    );
    let decision = super::super::support::upstream_failure::classify_upstream_failure(
        502, error_message, error_code.as_deref(), false, false,
    );
    
    log::info!(
        "event=aggregate_api_sse_terminal_classification trace_id={} candidate_id={} error_code={} decision={:?}",
        trace_id,
        candidate_id,
        error_code.as_deref().unwrap_or("-"),
        decision
    );
    
    match decision {
        // ... (分支实现)
    }
}
```

**验证**：
- 手动运行测试，检查日志输出
- 确认 `error_code` 和 `decision` 正确记录

---

## 依赖关系

```
Slice 1 (辅助函数)
  ↓
Slice 2 (CandidateFailover)
  ↓
Slice 3 (RetrySameCandidate) + Slice 4 (RequestTerminal)
  ↓
Slice 5 (已交付不变性) + Slice 6 (回归)
  ↓
Slice 7 (日志)
```

每个 Slice 独立可验证，按顺序实施可保持代码始终可运行。

---

## 回滚计划

若发现未预期的回归或生产问题：

1. **环境变量控制**：
   - 添加 `CODEXMANAGER_AGGREGATE_SSE_FAILOVER_ENABLED` 开关
   - 默认 `true`；紧急情况设为 `false` 恢复旧行为

2. **代码回滚点**：
   - Slice 2-4 的 `if !bridge_ok && bridge.pending_failover_request.is_some()` 块
   - 整个块可安全删除，回到 `succeeded=true` 原路径

3. **监控指标**：
   - `GATEWAY_FAILOVER_ATTEMPTS` 增量（预期上升）
   - `GATEWAY_UPSTREAM_CAPACITY_ERRORS` 无变化
   - 请求日志中 `attempted_aggregate_api_ids_json` 长度分布

---

## 未涵盖场景（明确排除）

1. **非流式 Responses 路径**：
   - 行为：非流式已在 `aggregate_api.rs:2890-2939` 预转换为 Responses 形状
   - 影响：非流式错误在 HTTP 状态码层面处理，不走 SSE 终态路径
   - 结论：现有 HTTP 非成功分支已覆盖

2. **Anthropic Native SSE 协议**：
   - 行为：Anthropic `message_stop` 事件已在 `sse_frame.rs:109-110` 分类
   - 影响：`stream_terminal_error` 逻辑与 Anthropic 兼容
   - 结论：无需特殊处理

3. **Gemini SSE 协议**：
   - 行为：Gemini 使用 `candidates[].finishReason`，由 `responses_from_anthropic.rs` 转换
   - 影响：转换后统一为 Responses SSE 格式
   - 结论：无需特殊处理

---

## 测试辅助函数（假设）

```rust
/// 运行 Aggregate API SSE 场景测试
fn run_aggregate_sse_scenario(
    trace_id_suffix: &str,
    responses: Vec<(u16, &str, &str)>,  // (status, content-type, body)
    candidates: Vec<AggregateApiForTest>,
    is_stream: bool,
    affinity: Option<AggregateApiAffinity>,
) -> (Storage, AggregateAttemptOutcome, usize) {
    // 模拟 SSE 响应的测试工具
    // 返回：storage, outcome, 实际发出的 HTTP 请求数
}
```

此函数需在实际实现时根据现有测试框架（如 `run_aggregate_capacity_scenario`）编写。
