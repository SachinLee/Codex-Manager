# 设计：Aggregate API SSE 终态错误分类与候选轮转

## 背景与当前行为

### 问题根因
生产证据显示三个 `gpt-5.6-terra/xhigh` 请求各只尝试了 `esfaery` 一次，尽管还有其他 active 候选（`wegoo-codex`、`timcc-0.2`、`wegoo-codex-0.2`）。

**根本原因**：`aggregate_api.rs:3357-3422` 在收到 SSE `response.failed`/`error` 终态时，无条件设置 `succeeded=true` 并立即返回 `AggregateAttemptOutcome::Responded`，跳过候选循环，**从未调用** `classify_upstream_failure()`。

现有分类器 `upstream_failure.rs:34-76` 只被 HTTP 非成功响应分支（`aggregate_api.rs:2828-2882`）调用，该分支发生在 bridge 创建之前。HTTP 200 + SSE 终态错误路径完全绕过了它。

### 现有成功路径对比
1. **HTTP 非 2xx JSON 错误**（`aggregate_api.rs:2657-2882`）：
   - 提取 `error_code_from_response_body()`（`upstream_failure.rs:89-100`）
   - 调用 `classify_upstream_failure(status_code, message, error_code, has_delivered=false, ...)`
   - `rate_limit_exceeded` → `CandidateFailover`，切到下一候选
   - 已验证：`aggregate_api_tests.rs:880-900`

2. **Chat 前置检查**（`aggregate_api.rs:887-989`）：
   - `preflight_chat_stream()` 在交付前检查 SSE 前缀
   - 遇到 `{"error": ...}` 或 `content_filter` → 返回 `ChatPreflightOutcome::Failover(message)`
   - 外层 `2946-2960` 收到 failover 后设置 `cooldown_eligible_failure=true; break`，由候选循环处理
   - 限制：只覆盖流式 Chat 协议（`UpstreamProtocol::ChatCompletions`），不覆盖 Responses

3. **账号池前置检查**（`stream_preflight.rs:1-460`）：
   - 对账号池路径，在交付前有完整的 SSE 错误/usage-limit 检测
   - Aggregate API 路径未复用

### 现有零交付保护
`delivery.rs:409-448` 和 `469-515` 已实现有限的零交付 failover guard：
- 条件：`terminal_error.is_some() && status==200 && !saw_terminal && output_tokens==0`
- 若满足：返回 `pending_failover_request = Some(request)`
- 由 `CODEXMANAGER_AGGREGATE_ZERO_DELIVERY_FAILOVER` 环境变量控制（默认 disabled）

**问题**：此保护仅对传输级错误（连接断开、EOF）有效，不处理语义级 SSE `response.failed`/`error` 终态。后者设置 `saw_terminal=true`，不满足 `!saw_terminal` 条件。

## 解决方案设计

### 策略表：上游错误决策矩阵

| 错误来源 | HTTP 状态 | SSE 事件 | `error.code` | 零交付？ | 决策 | 健康记录 |
|---------|----------|----------|-------------|---------|------|---------|
| **HTTP 非 2xx JSON** | 502 | — | `rate_limit_exceeded` | ✓ | `CandidateFailover` | cooldown |
| **HTTP 非 2xx JSON** | 502 | — | 其他/null | ✓ | `RetrySameCandidate` | cooldown (预算耗尽后) |
| **HTTP 非 2xx JSON** | 401/403/404/405/429/501 | — | 任意 | ✓ | `CandidateFailover` | cooldown |
| **HTTP 非 2xx JSON** | 400/422/413 | — | 任意 | ✓ | `RequestTerminal` | 无 |
| **SSE 终态** | 200 | `response.failed`/`error` | `rate_limit_exceeded` | ✓ | `CandidateFailover` | cooldown |
| **SSE 终态** | 200 | `response.failed`/`error` | 其他/null | ✓ | `RetrySameCandidate` | cooldown (预算耗尽后) |
| **SSE 终态** | 200 | `response.failed`/`error` | 任意 | ✗（已交付） | `RequestTerminal` | 无 |
| **Chat 前置检查** | 200 | `{"error": ...}` 在首帧 | 任意 | ✓ | `CandidateFailover` | cooldown |
| **传输断流** | 200 | EOF/idle timeout | — | ✓（零 token） | failover (当前可选) | cooldown |
| **容量错误** | 任意 | 任意 | — | ✓ | `CapacityRecovery` | 无 (health-neutral) |

**关键决策**：
1. **零交付判定**：现有 `pending_failover_request.is_some()` 已是最小准确信号
2. **错误码提取**：复用 `error_code_from_response_body()` 的 JSON 解析逻辑，从 SSE `stream_terminal_error` 字符串中提取
3. **分类器统一**：所有上游错误都经过 `classify_upstream_failure()`，消除路径分歧

### 最小修改边界

**核心变更点**：`aggregate_api.rs:3138-3422` 段落

#### 当前流程（有缺陷）
```rust
3138: let bridge_ok = bridge.is_ok(is_stream);
3139: let mut final_error = bridge.upstream_error_hint.clone();
3140: if final_error.is_none() && !bridge_ok {
3141:     final_error = Some(bridge.error_message(is_stream).unwrap_or(...));
3145: }
// ... capacity recovery logic 3146-3207 ...
3357: succeeded = true;
3358: break;  // 无条件跳出，返回 Responded
```

#### 修复后流程
```rust
3138: let bridge_ok = bridge.is_ok(is_stream);
3139: let mut final_error = bridge.upstream_error_hint.clone();
3140: if final_error.is_none() && !bridge_ok {
3141:     final_error = Some(bridge.error_message(is_stream).unwrap_or(...));
3145: }

// NEW: SSE 终态错误的候选决策
3208: if !bridge_ok && bridge.pending_failover_request.is_some() {
3209:     // 零交付：请求可归还，分类后决策候选/同候选重试/终止
3210:     let error_message = final_error.as_deref().unwrap_or("upstream response incomplete");
3211:     let error_code = extract_error_code_from_terminal(final_error.as_deref());
3212:     let decision = classify_upstream_failure(
3213:         502,  // SSE 终态统一视为 502 客户端错误
3214:         error_message,
3215:         error_code.as_deref(),
3216:         false,  // pending_failover_request 保证未交付
3217:         false,  // 非推理守卫路径
3218:     );
3219:     match decision {
3220:         UpstreamFailureDecision::CandidateFailover => {
3221:             // rate_limit_exceeded 等候选级错误：切到下一候选
3222:             request = bridge.pending_failover_request.take();
3223:             last_attempt_error = Some(error_message.to_string());
3224:             last_failure_status = 502;
3225:             cooldown_eligible_failure = true;
3226:             break;  // 外层候选循环继续
3227:         }
3228:         UpstreamFailureDecision::RetrySameCandidate => {
3229:             // 普通 5xx 类错误：同候选重试
3230:             if transport_retry_budget_remaining > 0 {
3231:                 request = bridge.pending_failover_request.take();
3232:                 transport_retry_budget_remaining -= 1;
3233:                 next_attempt_kind = SPEND_ATTEMPT_KIND_TRANSPORT_RETRY;
3234:                 continue;  // 内层重试循环
3235:             }
3236:             // 预算耗尽：候选失败，外层继续
3237:             request = bridge.pending_failover_request.take();
3238:             last_attempt_error = Some(error_message.to_string());
3239:             last_failure_status = 502;
3240:             cooldown_eligible_failure = true;
3241:             break;
3242:         }
3243:         UpstreamFailureDecision::RequestTerminal => {
3244:             // 400/422 等请求级错误：直接终止
3245:             terminal_failure = true;
3246:             last_attempt_error = Some(error_message.to_string());
3247:             last_failure_status = 502;
3248:             break;
3249:         }
3250:         _ => {
3251:             // CapacityRecovery/CapabilityRetry 已在前方处理
3252:             // 不应在 SSE 终态路径出现；保守 fallthrough
3253:             request = bridge.pending_failover_request.take();
3254:             last_attempt_error = Some(error_message.to_string());
3255:             last_failure_status = 502;
3256:             cooldown_eligible_failure = true;
3257:             break;
3258:         }
3259:     }
3260: }
3261:
3262: // 已交付或无请求可归还：原路径
3357: succeeded = true;
3358: break;
```

**辅助函数**（新增）：
```rust
/// 从 SSE 终态错误消息中提取 error.code（若有）。
/// 复用 error_code_from_response_body 的解析逻辑。
fn extract_error_code_from_terminal(terminal_error: Option<&str>) -> Option<String> {
    let message = terminal_error?;
    // 1. 尝试 JSON 解析：{"error": {"code": "...", "message": "..."}}
    if let Ok(value) = serde_json::from_str::<Value>(message) {
        return error_code_from_response_body(message.as_bytes());
    }
    // 2. 前缀匹配：`code=rate_limit_exceeded ...`
    //    这是 extract_message_from_error_map 的输出格式
    let normalized = message.trim();
    if let Some(rest) = normalized.strip_prefix("code=") {
        if let Some(code) = rest.split_whitespace().next() {
            return Some(code.to_string());
        }
    }
    None
}
```

### 不变量与边界条件

#### 零交付保证
- **检查点**：`bridge.pending_failover_request.is_some()`
- **含义**：delivery 层确认请求未被消费、无 semantic 输出已写入客户端
- **来源**：`delivery.rs:410-414` / `470-476`
- **条件**：`terminal_error.is_some() && status==200 && !saw_terminal && output_tokens==0`
- **已交付情况**：`pending_failover_request=None`，跳过分类，维持 `succeeded=true` 原路径

#### 候选轮转边界
1. **单次遍历**：外层 `for (candidate_idx, ...) in planned_candidates` 只走一遍（`aggregate_api.rs:2173`）
2. **同候选预算**：每候选初始 1 次 + `transport_retry_budget=3`（`aggregate_api.rs:2317-2328`）
3. **候选耗尽终态**：外层循环结束后 `3448-3534` 向客户端写一次最终错误
4. **模型降级有界**：若走模型 fallback，`proxy.rs:22` 限制 `MAX_MODEL_FALLBACK_HOPS=3`

#### 健康与冷却
- **Cooldown 触发**：`cooldown_eligible_failure=true` 后 `3365-3370` 调用 `gateway_record_aggregate_api_failure()`
- **不触发冷却的情况**：
  - Reasoning Guard 内部重试（`3031: cooldown_eligible_failure=false`）
  - 已交付的容量错误（`3154: cooldown_eligible_failure=false`）
  - 请求级终态错误（400/422，不应冷却健康候选）
- **会话亲和性清除**：失败时清除绑定（`3390-3406`），成功时更新绑定（`3381-3388`）

#### Spend 账户
- **预留**：每次 attempt 前 `reserve_daily_spend_attempt()`
- **结算**：成功时 `settle_daily_spend_from_usage()`
- **持有**：失败/ambiguous 时 `hold_daily_spend_attempt()`
- **释放**：零交付的容量重试前 `release_daily_spend_attempt()`
- **零交付失败的 SSE 终态**：请求未消费 → 应 release，同账号池 `response_finalize.rs:489-490`

#### 与现有 Preflight 的关系
- **Chat preflight**（`aggregate_api.rs:2940-2962`）：在 bridge 前检查，只覆盖流式 Chat
- **SSE 终态分类**（本方案）：在 bridge 后检查，覆盖所有 Responses SSE
- **互补关系**：preflight 捕获 Chat 首帧错误，bridge 后捕获 Responses 和 Chat 非流式的终态
- **不冲突**：preflight 返回 failover 时不会创建 bridge；SSE 终态分类只在 bridge 已创建、请求可归还时生效

### 风险与权衡

#### 已排除的替代方案
1. **修改零交付条件 `!saw_terminal`**：
   - 错误：`saw_terminal=true` 是正确行为，表示收到了 SSE 终态事件
   - 风险：改为允许 `saw_terminal=true` 后重放，可能在已交付场景错误重试
   - 正确做法：保持 `saw_terminal` 语义，依赖 `pending_failover_request` 判定可归还性

2. **在 delivery 层直接分类**：
   - 缺点：delivery 层不应了解 Aggregate 候选决策、重试预算、cooldown 逻辑
   - 职责：delivery 负责 SSE 解析和零交付判定；候选决策属于 aggregate_api 外层循环

3. **泛化为"所有 502 都 failover"**：
   - 错误：HTTP 502 JSON 已有正确分类（同候选重试 vs 候选 failover）
   - 风险：忽略 `error.code`，将普通传输错误也强制切候选，破坏重试预算语义

#### 残留限制
1. **已交付后的终态错误**：
   - 场景：客户端已收到部分 SSE delta，后续出现 `response.failed`
   - 行为：不重试，维持 `succeeded=true`，记录 `attempt_records` 为失败
   - 理由：已交付内容不可撤回，重放会导致重复输出或工具调用
   - 观测：`affinity_binding_success=false`，亲和性清除

2. **Gateway 生成的 502 vs 上游原始 502**：
   - Gateway 归一化：SSE 终态转换为 502 客户端响应，`error_code=NULL`
   - 原始上游 502 JSON：HTTP 非成功分支，`error_code` 可提取
   - 区分：已在策略表中明确；本方案不改变 HTTP 非成功分支行为

3. **缓存率相关性非因果性**：
   - 观测：单候选路径 83.57% cache，多候选路径 68.26% cache
   - 解释：多候选路径通常是首候选失败后的 fallback，请求往往无缓存
   - 不作为设计约束：修复后仍允许候选轮转，缓存率下降是正确 failover 的副作用，而非目标

## 实现顺序

### 前置检查
- [x] 确认 `pending_failover_request` 语义正确
- [x] 确认 `classify_upstream_failure()` 已覆盖所有决策类型
- [x] 确认 SSE `stream_terminal_error` 包含完整错误消息（含 `error.code` 若有）

### Slice 划分
1. **添加 `extract_error_code_from_terminal()` 辅助函数**
   - 单元测试：JSON 解析、前缀匹配、无 code 场景
   
2. **在 bridge 后插入 SSE 终态分类逻辑**
   - 位置：`aggregate_api.rs:3208` (capacity recovery 后、succeeded 赋值前)
   - 条件：`!bridge_ok && bridge.pending_failover_request.is_some()`
   
3. **实现候选 failover 分支**
   - 测试：HTTP 200 + SSE `{"error": {"code": "rate_limit_exceeded"}}` → 两次 upstream attempt
   
4. **实现同候选重试分支**
   - 测试：HTTP 200 + SSE `{"error": {"code": "server_error"}}` → 最多 4 次同候选 attempt
   
5. **实现请求级终止分支**
   - 测试：HTTP 200 + SSE `{"error": {"code": "invalid_request"}}` → 单次 attempt，terminal_failure=true
   
6. **验证 spend 结算**
   - 零交付失败 → `release_daily_spend_attempt()`
   - 已交付失败 → `hold_daily_spend_attempt()`

### 回归验证
- 现有 `aggregate_api_tests.rs:880-900` (HTTP 502 JSON) 保持通过
- 现有 Chat preflight 行为不变
- 零候选/全冷却场景的 `upstream_url=NULL` 路径不受影响

## 开放问题
无。用户已确认：
- 要求区分真实上游错误与 gateway 生成的终态
- 要求保留有界候选轮转和重试预算
- 要求不在已交付后重放

## 参考文件
- `crates/service/src/gateway/upstream/protocol/aggregate_api.rs`
- `crates/service/src/gateway/upstream/support/upstream_failure.rs`
- `crates/service/src/gateway/observability/http_bridge/delivery.rs`
- `crates/service/src/gateway/observability/http_bridge/aggregate/output_text.rs`
- `crates/service/src/gateway/upstream/protocol/aggregate_api_tests.rs`
