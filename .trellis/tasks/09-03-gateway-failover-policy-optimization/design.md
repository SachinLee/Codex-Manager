# Design: 网关上游错误分类与故障切换策略优化

## Overview

本设计统一 Aggregate API 与账号池上游失败的决策边界，建立类型化的错误分类，避免不可恢复的请求级 4xx 消耗同候选重试预算，同时保持真正的候选级故障可以 failover。核心改动是引入共享的 `UpstreamFailureDecision` 枚举和错误语义识别函数，替代散布在各处的 `!status.is_success()` 分支和字符串子串匹配。

## Architecture

### Current state

**Aggregate API 路径** (`aggregate_api.rs`):
- 所有非成功响应先尝试消耗 `transport_retry_budget_remaining`（初始为 `AGGREGATE_API_RETRY_ATTEMPTS_PER_CHANNEL = 3`）。
- 容量错误已有独立的 `capacity_retry_budget_remaining`（初始为 2），但仍在 WIP 分支中最终状态码可能不符合规范要求的 503。
- 400/401/403/404/429/5xx 是否会被同候选重放，或是否会推进到下一个 Aggregate API 候选，当前依赖泛化的失败分支和 `last_failure_status` 赋值时机；需要回归测试锁定基线。

**账号池路径** (`candidate_executor.rs` / `response_finalize.rs` / `account_status.rs`):
- `analyze_gateway_error` 将错误归为 `Deactivation`、`ReasoningGuard`、`Timeout`、`UsageLimit`、`Other`。
- `GatewayErrorFollowUp` 包含 `should_failover`、`should_mark_account_unavailable` 等布尔标志。
- 但当前 `rate_limit_exceeded` 错误码（常见于 OpenAI API 响应体）不被识别，被归为 `Other`；`Other` 的 `should_failover = false`，导致截图中的问题。

**共同问题**:
- 两条路径各自维护错误分类逻辑，存在语义不一致风险。
- 容量错误、能力错误、挑战重试的特殊预算和终态与普通 failover 混在一起。
- 缺少类型化的决策边界，难以在测试中断言"此错误是请求级终止"或"此错误是候选级 failover"。

### Target state

**新增共享决策类型** (`crates/service/src/gateway/upstream/support/upstream_failure.rs`):

```rust
/// 上游失败后的决策边界。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::gateway) enum UpstreamFailureDecision {
    /// 请求级终止：400/422 等明确的请求体或协议错误，不应在任何候选上重试。
    RequestTerminal,
    /// 候选级 failover：401/403/404/405/429/501、`rate_limit_exceeded` 等候选特有错误；
    /// 若有后续候选且未交付内容，应推进；否则终止。
    CandidateFailover,
    /// 同候选内部重试：5xx、连接错误、idle timeout 等可能瞬时恢复的传输故障。
    RetrySameCandidate,
    /// 独立容量恢复：精确匹配的 `selected model is at capacity` 错误，使用独立预算和延迟重放。
    CapacityRecovery,
    /// 独立能力降级：ChatGPT legacy 兼容重试，使用独立预算。
    CapabilityRetry,
    /// 推理守卫内部重试：`ReasoningGuardBridgeAction::InternalRetry`，使用独立预算。
    ReasoningGuardRetry,
}

/// 从账号池上游响应或 Aggregate API 响应分类决策。
pub(in crate::gateway) fn classify_upstream_failure(
    status_code: u16,
    error_message: &str,
    error_code_from_body: Option<&str>,
    has_delivered_content: bool,
    bridge_action: Option<ReasoningGuardBridgeAction>,
) -> UpstreamFailureDecision {
    // 优先级顺序：
    // 1. 推理守卫内部重试
    // 2. 容量错误
    // 3. 能力重试（若启用）
    // 4. 请求级终止 4xx
    // 5. 候选级 failover
    // 6. 同候选传输重试
    // 已交付内容后，除推理守卫外其他路径都不应重试或 failover。
}
```

**Aggregate API 集成**:
- 在 `proxy_aggregate_request` 的失败分支，调用 `classify_upstream_failure` 决定是消耗传输预算、切换候选、还是立即终止。
- 容量错误路径保持独立：`schedule_aggregate_api_capacity_retry` 不变，但确保耗尽后返回 503。
- 移除泛化的 `!upstream.status().is_success()` 分支对传输预算的无差别消耗。

**账号池集成**:
- 在 `finalize_upstream_response` 和 `finalize_terminal_candidate`，复用 `classify_upstream_failure`。
- `analyze_gateway_error` 继续用于账号可用性和冷却标记，但 failover 决策改由 `UpstreamFailureDecision::CandidateFailover` 判定。
- 补充 `rate_limit_exceeded` 识别，优先于外层 502 状态码。

## Data structures

### New module: `upstream_failure.rs`

```rust
// crates/service/src/gateway/upstream/support/upstream_failure.rs

use crate::gateway::upstream::protocol::aggregate_api::ReasoningGuardBridgeAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::gateway) enum UpstreamFailureDecision {
    RequestTerminal,
    CandidateFailover,
    RetrySameCandidate,
    CapacityRecovery,
    CapabilityRetry,
    ReasoningGuardRetry,
}

pub(in crate::gateway) fn classify_upstream_failure(
    status_code: u16,
    error_message: &str,
    error_code_from_body: Option<&str>,
    has_delivered_content: bool,
    bridge_action: Option<ReasoningGuardBridgeAction>,
) -> UpstreamFailureDecision {
    // 已交付内容后，除推理守卫外不重试/failover
    if has_delivered_content && bridge_action.is_none() {
        return UpstreamFailureDecision::RequestTerminal;
    }

    // 推理守卫内部重试
    if matches!(
        bridge_action,
        Some(ReasoningGuardBridgeAction::InternalRetry)
    ) {
        return UpstreamFailureDecision::ReasoningGuardRetry;
    }

    // 容量错误
    if crate::gateway::is_selected_model_capacity_error(error_message) {
        return UpstreamFailureDecision::CapacityRecovery;
    }

    // 能力重试（ChatGPT legacy）
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
    // 现有 ChatGPT capability retry 逻辑
    message.contains("does not support") || message.contains("capability not available")
}
```

### Integration points

**Aggregate API** (`aggregate_api.rs`):

```rust
// 在失败分支，替代泛化的 transport_retry_budget_remaining 消耗：
let decision = classify_upstream_failure(
    upstream.status().as_u16(),
    last_attempt_error.as_deref().unwrap_or_default(),
    error_code_from_response_body(&upstream),
    bridge.delivered_content,
    bridge.reasoning_guard_action,
);

match decision {
    UpstreamFailureDecision::RequestTerminal => {
        // 立即终止，不消耗预算
        break;
    }
    UpstreamFailureDecision::CandidateFailover => {
        // 若有后续候选，推进；否则终止
        if has_more_aggregate_apis {
            break;  // 退出当前候选循环
        } else {
            // 使用 route_exhaustion_terminal_status
            break;
        }
    }
    UpstreamFailureDecision::RetrySameCandidate => {
        // 消耗传输预算，同候选重试
        if transport_retry_budget_remaining > 0 {
            transport_retry_budget_remaining -= 1;
            continue;
        } else {
            break;
        }
    }
    UpstreamFailureDecision::CapacityRecovery => {
        // 使用独立容量预算
        match schedule_aggregate_api_capacity_retry(...) {
            Retry => continue,
            Exhausted => return_capacity_503(),
            DeadlineExceeded => return_timeout_504(),
        }
    }
    UpstreamFailureDecision::CapabilityRetry => {
        // 使用独立能力预算（若启用）
    }
    UpstreamFailureDecision::ReasoningGuardRetry => {
        // 使用推理守卫预算
        continue;
    }
}
```

**账号池** (`response_finalize.rs`):

```rust
// 在 finalize_upstream_response，替代 analyze_gateway_error 的 should_failover：
let decision = classify_upstream_failure(
    status_code,
    error_message,
    error_code_from_body,
    has_delivered_content,
    bridge_action,
);

let follow_up = analyze_gateway_error(error_message, has_more_candidates);
// follow_up 仅用于账号可用性和冷却标记

match decision {
    UpstreamFailureDecision::CandidateFailover if has_more_candidates => {
        apply_account_marks(&follow_up);
        return FinalizeUpstreamResponseOutcome::Failover { ... };
    }
    _ => {
        apply_account_marks(&follow_up);
        return FinalizeUpstreamResponseOutcome::Terminal { ... };
    }
}
```

## Capacity error 503 fix

当前 WIP 分支在容量预算耗尽后可能赋值 `last_failure_status = 502`；规范要求返回 503。修正点：

```rust
// aggregate_api.rs，容量耗尽分支：
match schedule_aggregate_api_capacity_retry(...) {
    AggregateApiCapacityRetryAction::Retry => continue,
    AggregateApiCapacityRetryAction::Exhausted => {
        last_failure_status = 503;  // 确保是 503
        last_attempt_error = Some("selected model capacity exhausted after retries".to_string());
        break;
    }
    AggregateApiCapacityRetryAction::DeadlineExceeded => {
        // 超时路径保持现有终态
        break;
    }
}
```

## Error code extraction

需要从响应体提取 `error.code` 字段（OpenAI/Anthropic 风格）：

```rust
fn error_code_from_response_body(upstream: &GatewayUpstreamResponse) -> Option<String> {
    // 若响应体已缓冲且为 JSON，尝试解析 error.code
    // 若为流式或已消费，返回 None
    // 避免日志或持久化完整错误体
}
```

仅在决策时使用；不持久化到请求日志或 trace，避免敏感信息泄漏。

## Observability

**现有可用字段**:
- `request_logs.attempted_aggregate_api_ids_json`（若 `09-03` 落地结构化摘要，可复用其字段）
- `gateway-trace.log` 中已有候选 ID、位置和错误消息
- Prometheus 容量指标：`gateway_upstream_capacity_error_total`、`gateway_upstream_capacity_exhausted_total`、`gateway_upstream_capacity_internal_retry_total`

**新增可选 trace 字段**（不强制落盘，仅供测试和调试）:
- `decision_kind`: `request_terminal` / `candidate_failover` / `retry_same` / `capacity_recovery` / `capability_retry` / `reasoning_guard_retry`
- `error_code`: 若提取成功，记录 `rate_limit_exceeded` 等；不记录完整错误体

不新增公开 RPC、前端设置或 Prometheus 维度；验收依赖现有测试断言和 trace 查询。

## Migration and rollback

**Migration**: 无数据库或配置变更；纯逻辑修改。

**Rollback**: Git revert。若发现 failover 行为变化导致客户端兼容问题，可临时关闭本功能（通过 feature flag 或回滚 commit）。

**Compatibility**: 不改变 `route_exhaustion_terminal_status` 的 404/503 → 502 映射；不改变全局路由排序；不改变客户端错误码契约（除容量 503 修正）。

## Testing strategy

### Unit tests

**`upstream_failure.rs`**:
- `classify_upstream_failure` 覆盖所有决策分支：
  - 400/422 → `RequestTerminal`
  - 401/403/404/405/429/501 → `CandidateFailover`
  - 502 + `rate_limit_exceeded` → `CandidateFailover`
  - 502 + `Other` → `RetrySameCandidate`
  - 5xx → `RetrySameCandidate`
  - 容量错误 → `CapacityRecovery`
  - 已交付内容 + 非推理守卫 → `RequestTerminal`

**`aggregate_api_tests.rs`**:
- 400/401/403/404/429 不消耗 `AGGREGATE_API_RETRY_ATTEMPTS_PER_CHANNEL`，直接终止或推进候选
- 502 + `rate_limit_exceeded` 推进候选
- 容量错误：初始请求 + 2 次重放，耗尽后 503
- 5xx 消耗传输预算，最多 3 次同候选请求

**`candidate_executor` / `response_finalize` tests**:
- 账号池 401/403 + 有后续账号 → failover
- 账号池 502 + `rate_limit_exceeded` + 有后续账号 → failover
- 账号池 400/422 → 终止，不 failover

### Integration tests

- 空候选直接 502
- 单候选容量错误耗尽后 503
- 多候选链，首个 401，推进到第二个成功
- 多候选链，首个 502 + `rate_limit_exceeded`，推进到第二个成功

## Implementation deviations (as-built record)

实施期间与原设计的偏差，经基线验证后确认：

1. **对外状态码保持既有映射（重要）**：初版实现曾将 `last_failure_status` 设为真实上游状态码（会把 401/429 直接透传客户端），与 FR-5“保持现状”冲突。最终实现：`last_failure_status = 502` 保持既有对外收敛映射，`classify_upstream_failure` 使用真实 `status_code` 仅做内部决策。容量耗尽 503 是唯一的状态码变化（服务规范明确要求）。
2. **账号池未完整复用 `classify_upstream_failure`**：账号池已有状态驱动的 failover 结构（preflight `StatusFailover`、`failover_policy.rs`、`mark_account_cooldown_for_status`），全量替换风险高于收益。实际采用最小集成：`account_status.rs` 新增 `GatewayErrorKind::RateLimited` 与 `rate_limit_reason_from_message`（识别 `rate_limit_exceeded`/`rate_limit_error`/`too many requests`），`analyze_gateway_error` 中 failover=true 且不标记账号不可用；`stream_preflight.is_actionable_gateway_error` 接入同一识别函数。共享函数位于 `account_status.rs`，两条路径复用同一识别逻辑，未复制字符串判断。
3. **失败分支（bridge 之前）无 delivered-content / reasoning-guard 输入**：Aggregate API 非成功响应分支发生在 bridge 创建之前，`has_delivered_content` 恒为 false、推理守卫动作不存在，分类调用中这两参数传 `false`。已交付后不重放的约束由既有 bridge 路径（`pending_failover_request`）保障。
4. **`attempt_records` 未在决策分支推送（遗留）**：协作线程完成了 `AggregateAttemptOutcome::Responded { attempt_records }` 结构化摘要，但我的决策分支（RequestTerminal/CandidateFailover/RetrySameCandidate）尚未推送记录，失败尝试在最终日志仍只有最后一条。建议作为后续任务：在每个决策分支推送 `AggregateApiAttemptRecord`（含决策类别），并在 `stream_preflight` 状态码 failover 路径同步补充。
5. **测试依赖协作线程的 helper**：新增聚合测试使用 `run_aggregate_capacity_scenario`，其 `AggregateProxyRequest` 构造包含协作线程添加的 `aggregate_api_affinity: None` 字段——本任务的测试与亲和性改动存在编译级耦合，无法独立于亲和性改动单独提交。

## Deferred items

- 决策分支的 trace 日志与 `attempt_records` 推送（见偏差 4）。
- Phase 6 手动烟雾测试（多候选 401 / rate_limit_exceeded 场景已在自动化回归中覆盖，手动脚本待部署前验证）。
- 全量 `cargo clippy` 复核（确认无本任务引入的新警告；`account_status.rs:70/186` 的 2 个警告为协作线程既有 WIP 代码）。
- 9 个与本任务无关的既有测试失败（3 个 balance extractor、2 个 chat preflight/action-path、1 个 proxy 路由谓词、3 个 quota 定价），已在无本任务改动的基线上确认，需各自任务处理。

## Open questions

无。错误分类、预算边界、容量 503 修正、`rate_limit_exceeded` 识别均已明确。5xx/transport 重试次数保持基线行为，由回归测试锁定。
