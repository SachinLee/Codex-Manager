# Implementation Plan: 网关上游错误分类与故障切换策略优化

## Overview

分阶段实施类型化的上游失败决策边界，先建立共享分类函数和单元测试，再分别集成到 Aggregate API 和账号池路径，最后补齐回归测试并修正容量错误终态。每个阶段独立验证后再推进，避免一次性改动过多公共流程。

## Phase 1: Shared classification foundation

### 1.1 Create `upstream_failure.rs` module

**File**: `crates/service/src/gateway/upstream/support/upstream_failure.rs`

```rust
//! 上游失败决策边界分类。

use crate::gateway::is_selected_model_capacity_error;

/// 上游失败后的决策类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::gateway) enum UpstreamFailureDecision {
    /// 请求级终止：明确的请求体或协议错误，不应在任何候选上重试。
    RequestTerminal,
    /// 候选级 failover：候选特有错误；若有后续候选且未交付内容，应推进。
    CandidateFailover,
    /// 同候选内部重试：可能瞬时恢复的传输故障。
    RetrySameCandidate,
    /// 独立容量恢复：精确匹配的容量错误，使用独立预算。
    CapacityRecovery,
    /// 独立能力降级：ChatGPT legacy 兼容重试。
    CapabilityRetry,
    /// 推理守卫内部重试。
    ReasoningGuardRetry,
}

/// 从上游响应分类失败决策。
///
/// # 参数
/// - `status_code`: HTTP 状态码
/// - `error_message`: 错误消息文本
/// - `error_code_from_body`: 若响应体为 JSON，提取的 `error.code` 字段
/// - `has_delivered_content`: 是否已向客户端交付语义内容
/// - `is_reasoning_guard_internal_retry`: 是否为推理守卫内部重试
///
/// # 返回
/// 决策类型
pub(in crate::gateway) fn classify_upstream_failure(
    status_code: u16,
    error_message: &str,
    error_code_from_body: Option<&str>,
    has_delivered_content: bool,
    is_reasoning_guard_internal_retry: bool,
) -> UpstreamFailureDecision {
    // 已交付内容后，除推理守卫外不重试/failover
    if has_delivered_content && !is_reasoning_guard_internal_retry {
        return UpstreamFailureDecision::RequestTerminal;
    }

    // 推理守卫内部重试
    if is_reasoning_guard_internal_retry {
        return UpstreamFailureDecision::ReasoningGuardRetry;
    }

    // 容量错误
    if is_selected_model_capacity_error(error_message) {
        return UpstreamFailureDecision::CapacityRecovery;
    }

    // 能力重试
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
    let normalized = message.trim().to_ascii_lowercase();
    normalized.contains("does not support")
        || normalized.contains("capability not available")
        || normalized.contains("unsupported")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_request_terminal_4xx() {
        assert_eq!(
            classify_upstream_failure(400, "Bad Request", None, false, false),
            UpstreamFailureDecision::RequestTerminal
        );
        assert_eq!(
            classify_upstream_failure(422, "Unprocessable Entity", None, false, false),
            UpstreamFailureDecision::RequestTerminal
        );
        assert_eq!(
            classify_upstream_failure(413, "Request Too Large", None, false, false),
            UpstreamFailureDecision::RequestTerminal
        );
    }

    #[test]
    fn classify_candidate_failover_status_codes() {
        for code in [401, 403, 404, 405, 429, 501] {
            assert_eq!(
                classify_upstream_failure(code, "", None, false, false),
                UpstreamFailureDecision::CandidateFailover,
                "status {} should trigger CandidateFailover",
                code
            );
        }
    }

    #[test]
    fn classify_rate_limit_exceeded_from_body() {
        // 502 但错误码明确为 rate_limit_exceeded
        assert_eq!(
            classify_upstream_failure(502, "error", Some("rate_limit_exceeded"), false, false),
            UpstreamFailureDecision::CandidateFailover
        );

        // 502 且错误码为其他，应归为 RetrySameCandidate
        assert_eq!(
            classify_upstream_failure(502, "error", Some("internal_error"), false, false),
            UpstreamFailureDecision::RetrySameCandidate
        );
    }

    #[test]
    fn classify_capacity_error() {
        assert_eq!(
            classify_upstream_failure(
                502,
                "selected model is at capacity. please try a different model",
                None,
                false,
                false
            ),
            UpstreamFailureDecision::CapacityRecovery
        );
    }

    #[test]
    fn classify_retry_same_candidate_5xx() {
        assert_eq!(
            classify_upstream_failure(500, "Internal Server Error", None, false, false),
            UpstreamFailureDecision::RetrySameCandidate
        );
        assert_eq!(
            classify_upstream_failure(503, "Service Unavailable", None, false, false),
            UpstreamFailureDecision::RetrySameCandidate
        );
    }

    #[test]
    fn classify_terminal_after_delivery() {
        assert_eq!(
            classify_upstream_failure(500, "", None, true, false),
            UpstreamFailureDecision::RequestTerminal
        );
    }

    #[test]
    fn classify_reasoning_guard_retry() {
        assert_eq!(
            classify_upstream_failure(500, "", None, true, true),
            UpstreamFailureDecision::ReasoningGuardRetry
        );
    }
}
```

**Verification**:
```bash
cargo test -p codexmanager-service upstream_failure::tests
```

### 1.2 Export module

**File**: `crates/service/src/gateway/upstream/support/mod.rs`

```rust
pub(in crate::gateway) mod upstream_failure;
```

---

## Phase 2: Error code extraction helper

### 2.1 Add `error_code_from_response_body`

**File**: `crates/service/src/gateway/upstream/support/upstream_failure.rs`

```rust
use serde_json::Value;

/// 从响应体提取 error.code 字段（OpenAI/Anthropic 风格）。
///
/// 仅在响应体已完全缓冲且为 JSON 时尝试解析；否则返回 None。
/// 不持久化完整错误体，避免敏感信息泄漏。
pub(in crate::gateway) fn error_code_from_response_body(body: &[u8]) -> Option<String> {
    if body.is_empty() {
        return None;
    }

    let parsed: Value = serde_json::from_slice(body).ok()?;
    parsed
        .get("error")?
        .get("code")?
        .as_str()
        .map(|s| s.to_string())
}

#[cfg(test)]
mod body_tests {
    use super::*;

    #[test]
    fn extract_rate_limit_exceeded() {
        let body = r#"{"error":{"code":"rate_limit_exceeded","message":"Rate limit reached"}}"#;
        assert_eq!(
            error_code_from_response_body(body.as_bytes()),
            Some("rate_limit_exceeded".to_string())
        );
    }

    #[test]
    fn extract_none_for_non_json() {
        assert_eq!(error_code_from_response_body(b"plain text"), None);
    }

    #[test]
    fn extract_none_for_missing_code() {
        let body = r#"{"error":{"message":"something"}}"#;
        assert_eq!(error_code_from_response_body(body.as_bytes()), None);
    }
}
```

**Verification**:
```bash
cargo test -p codexmanager-service upstream_failure::body_tests
```

---

## Phase 3: Aggregate API integration

### 3.1 Import classification function

**File**: `crates/service/src/gateway/upstream/protocol/aggregate_api.rs`

```rust
use super::super::support::upstream_failure::{
    classify_upstream_failure, error_code_from_response_body, UpstreamFailureDecision,
};
```

### 3.2 Replace generic failure branch

**Location**: `aggregate_api.rs:2400-2700`（失败响应处理循环）

**Before**:
```rust
last_attempt_error = Some(message);
last_failure_status = 502;
// 泛化的传输重试
if transport_retry_budget_remaining > 0 {
    transport_retry_budget_remaining -= 1;
    continue;
}
```

**After**:
```rust
last_attempt_error = Some(message.clone());
last_failure_status = upstream.status().as_u16();

// 提取错误码
let error_code = if let Some(body) = upstream.body_bytes() {
    error_code_from_response_body(body)
} else {
    None
};

// 分类决策
let is_reasoning_guard_internal_retry = matches!(
    bridge.reasoning_guard_action,
    Some(ReasoningGuardBridgeAction::InternalRetry)
);

let decision = classify_upstream_failure(
    last_failure_status,
    &message,
    error_code.as_deref(),
    bridge.delivered_content,
    is_reasoning_guard_internal_retry,
);

match decision {
    UpstreamFailureDecision::RequestTerminal => {
        // 立即终止，不消耗预算
        break;
    }
    UpstreamFailureDecision::CandidateFailover => {
        // 若有后续候选，推进；否则终止
        if candidate_index + 1 < candidates.len() {
            break; // 退出当前候选循环
        } else {
            break; // 无后续候选，使用 route_exhaustion_terminal_status
        }
    }
    UpstreamFailureDecision::RetrySameCandidate => {
        // 消耗传输预算
        if transport_retry_budget_remaining > 0 {
            transport_retry_budget_remaining -= 1;
            next_attempt_kind = SPEND_ATTEMPT_KIND_TRANSPORT_RETRY;
            continue;
        } else {
            break;
        }
    }
    UpstreamFailureDecision::CapacityRecovery => {
        // 使用独立容量预算（已有逻辑）
        match schedule_aggregate_api_capacity_retry(...) {
            // ... 现有实现
        }
    }
    UpstreamFailureDecision::CapabilityRetry => {
        // ChatGPT capability retry（若已启用）
        // 保持现有实现
    }
    UpstreamFailureDecision::ReasoningGuardRetry => {
        // 推理守卫内部重试
        continue;
    }
}
```

### 3.3 Fix capacity exhausted status code

**Location**: `aggregate_api.rs` 容量耗尽分支

**Before**:
```rust
last_failure_status = 502;
```

**After**:
```rust
last_failure_status = 503;
last_attempt_error = Some("selected model capacity exhausted after retries".to_string());
```

**Verification**:
```bash
cargo test -p codexmanager-service aggregate_api::tests::capacity_error_returns_503_after_exhaustion
```

---

## Phase 4: Account pool integration

### 4.1 Import classification function

**File**: `crates/service/src/gateway/upstream/proxy_pipeline/response_finalize.rs`

```rust
use super::super::support::upstream_failure::{
    classify_upstream_failure, error_code_from_response_body, UpstreamFailureDecision,
};
```

### 4.2 Update `finalize_upstream_response`

**Location**: `response_finalize.rs` 非成功响应分支

**Before**:
```rust
let follow_up = analyze_gateway_error(error_message, has_more_candidates);
if follow_up.should_failover {
    return FinalizeUpstreamResponseOutcome::Failover { ... };
}
```

**After**:
```rust
let error_code = error_code_from_response_body(response_body_if_buffered);

let is_reasoning_guard_internal_retry = matches!(
    bridge_action,
    Some(ReasoningGuardBridgeAction::InternalRetry)
);

let decision = classify_upstream_failure(
    status_code,
    error_message,
    error_code.as_deref(),
    has_delivered_content,
    is_reasoning_guard_internal_retry,
);

// follow_up 仅用于账号可用性和冷却标记
let follow_up = analyze_gateway_error(error_message, has_more_candidates);
apply_account_marks(&follow_up);

match decision {
    UpstreamFailureDecision::CandidateFailover if has_more_candidates => {
        return FinalizeUpstreamResponseOutcome::Failover { ... };
    }
    _ => {
        return FinalizeUpstreamResponseOutcome::Terminal { ... };
    }
}
```

**Verification**:
```bash
cargo test -p codexmanager-service response_finalize::tests::rate_limit_exceeded_triggers_failover
```

---

## Phase 5: Regression tests

### 5.1 Aggregate API tests

**File**: `crates/service/src/gateway/upstream/protocol/aggregate_api_tests.rs`

```rust
#[test]
fn aggregate_400_does_not_consume_transport_budget() {
    // 模拟 400 响应
    // 断言：只有 1 次请求，未消耗 AGGREGATE_API_RETRY_ATTEMPTS_PER_CHANNEL
}

#[test]
fn aggregate_401_failover_to_next_candidate() {
    // 模拟两个候选：第一个返回 401，第二个成功
    // 断言：第一个候选只请求 1 次，第二个候选成功
}

#[test]
fn aggregate_502_rate_limit_exceeded_failover() {
    // 模拟 502 + rate_limit_exceeded 错误码
    // 断言：推进到下一个候选
}

#[test]
fn aggregate_5xx_consumes_transport_budget() {
    // 模拟 500 响应
    // 断言：初始请求 + 最多 3 次同候选传输重试
}

#[test]
fn capacity_error_returns_503_after_exhaustion() {
    // 模拟容量错误
    // 断言：初始请求 + 2 次重放，最终状态码 503
}
```

### 5.2 Account pool tests

**File**: `crates/service/src/gateway/upstream/proxy_pipeline/tests/response_finalize_tests.rs`

```rust
#[test]
fn account_401_failover_when_has_more_candidates() {
    // 模拟账号 401
    // 断言：返回 Failover
}

#[test]
fn account_502_rate_limit_exceeded_failover() {
    // 模拟 502 + rate_limit_exceeded
    // 断言：返回 Failover
}

#[test]
fn account_400_terminal_no_failover() {
    // 模拟账号 400
    // 断言：返回 Terminal，不 failover
}
```

**Verification**:
```bash
cargo test -p codexmanager-service
```

---

## Phase 6: Integration and smoke tests

### 6.1 Multi-candidate scenarios

- 空候选直接 502
- 单候选容量错误耗尽后 503
- 多候选链，首个 401，推进到第二个成功
- 多候选链，首个 502 + rate_limit_exceeded，推进到第二个成功

### 6.2 Smoke test script

```bash
#!/bin/bash
# 本地烟雾测试：模拟多候选 failover

# 1. 启动本地服务
cargo build --release
./target/release/codexmanager-service &
SERVICE_PID=$!

# 2. 配置两个测试候选：第一个返回 401，第二个成功

# 3. 发送测试请求
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer test-key" \
  -d '{"model":"gpt-4","messages":[{"role":"user","content":"test"}]}'

# 4. 检查日志：确认第一个候选 401，第二个候选成功

kill $SERVICE_PID
```

---

## Verification commands

### During development

```bash
# 单元测试
cargo test -p codexmanager-service upstream_failure

# Aggregate API 测试
cargo test -p codexmanager-service aggregate_api

# 账号池测试
cargo test -p codexmanager-service response_finalize

# 全量服务测试
cargo test -p codexmanager-service

# 格式检查
cargo fmt --check
cargo clippy -- -D warnings
```

### Post-implementation

```bash
# 完整构建
cargo build --release

# 回归测试覆盖
cargo test --workspace

# 性能基准（若需要）
cargo bench --workspace
```

---

## Rollback plan

- **Git revert**: 所有改动在单个 PR 中，revert commit 即可完全回滚。
- **Feature flag**: 若需要逐步上线，可在 `app_settings` 中增加 `gateway.upstream_failure_classification_enabled`，默认 `false`，逐步灰度。

---

## Dependencies

- 本任务不依赖 `09-03-request-log-cache-failover-optimization` 的 `aggregate_api_bindings` 表或亲和性逻辑。
- 可复用已稳定的 `request_logs.attempted_aggregate_api_ids_json` 字段记录候选尝试链；该字段在 migration 039 中已添加，早于亲和性表结构。
- 不读写 `aggregate_api_bindings` 表，避免与 09-03 的会话级亲和性逻辑冲突。
---

## Acceptance checklist
## Acceptance checklist（实施进度）

- [x] `upstream_failure.rs` 模块与 11 个单元测试全部通过（分类 + 错误码提取）
- [x] Aggregate API 失败分支接入 `classify_upstream_failure`；对外状态保持既有 502 映射（决策用真实状态码）
- [x] 容量耗尽返回 503（服务规范修正），6 个容量测试全部通过
- [x] 新增 4 个聚合回归测试：400 不消耗预算 / 401 立即 failover / 502+rate_limit_exceeded failover（用户截图场景）/ 500 保留 3 次传输预算
- [x] 账号池 `analyze_gateway_error` 新增 `RateLimited` 类别：`rate_limit_exceeded`/`rate_limit_error`/`too many requests` 识别为可 failover，不标记账号不可用
- [x] `stream_preflight.is_actionable_gateway_error` 接入限流识别，SSE 错误事件限流语义可触发 failover
- [x] 账号池回归测试 `gateway_rate_limit_error_fails_over_without_account_unavailable` 通过
- [x] 我方触碰文件 `cargo fmt --check` 干净
- [ ] 全套件 `cargo test -p codexmanager-service`：1663 通过 / 9 失败——9 个失败均为与本任务无关的既有问题（3 个 balance extractor、2 个 chat 测试经基线确认、1 个 proxy 路由谓词、3 个 quota 定价）

## 协作约束记录
- 实施期间另一线程完成了 Aggregate API 亲和性（`aggregate_api_bindings`、`AggregateAttemptOutcome::Responded { attempt_records }`、`AggregateProxyRequest.aggregate_api_affinity`），本任务改动已在其最终基线之上重新应用并验证。
- 实施中发现初版实现曾把 `last_failure_status = status_code`（会把 401/429 等真实状态直接透传客户端），与 FR-5“保持现状”冲突，已修正为：对外统一 502，决策分类使用真实状态码。

## Deferred（遗留，见 design.md Implementation deviations）

- [ ] 决策分支 trace 日志与 `attempt_records` 推送：RequestTerminal/CandidateFailover/RetrySameCandidate 分支尚未推送结构化尝试记录，失败尝试在最终日志只有最后一条；建议后续任务在每个决策分支推送 `AggregateApiAttemptRecord`（含决策类别）。
- [ ] Phase 6 手动烟雾测试（自动化回归已覆盖核心场景，手动多候选脚本留待部署前验证）。
- [ ] 全量 `cargo clippy` 复核（已确认无本任务引入的新警告；既有 2 个警告属协作线程 WIP 代码）。
- [ ] 9 个既有测试失败需各自归属任务处理，不在本任务范围。

## 提交边界说明

- 提交结果：两任务已合并提交为 `dfc9bd95`（18 文件，+1630/-150），包含亲和性 + 本任务全部 Rust 代码、迁移与测试。
- 提交采用部分暂存：`account_status.rs` 中归属待确认的 `AccountStatusContext.updated_at` 相关改动已排除，保留在工作区。
- 编译验证发现：基线 merge commit（`22e64b72`）本身不可编译——其根 `aggregate_api.rs` 引用 `AggregateApiFetchModelsResult`/`ModelRouteV2` 等类型但 import 缺失（模型发现任务的配套改动在工作区 WIP 中）。`dfc9bd95` 继承该基线，未引入新破坏；分支在模型发现任务的 WIP（`lib.rs`、根 `aggregate_api.rs`、i18n 等）提交后恢复完整可编译。
- 工作区另甄别出：OMP 日志元数据任务（`requestlog_session_titles.rs`、`local_validation/request.rs` 含临时调试日志、`request-log.ts`）与模型发现任务（`lib.rs`、根 `aggregate_api.rs`、`zh-aggregate-api.ts`）的改动，均未纳入本次提交。
