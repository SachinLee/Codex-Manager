# CodexManager 网关失败处理逻辑优化分析

## 执行摘要

当前网关实现在"聚合 API 候选轮转"与"账号池轮转"两条路径上存在**错误分类不一致、重试预算过度消耗、客户端可见状态码映射不合理**三大问题。这些问题导致：
1. **相同错误在不同路由策略下表现不同**（400/401/429 在聚合路径被当作可重试 502，在账号池路径按语义终态返回）
2. **无效重试浪费延迟和配额**（客户端 4xx 错误吃满 3 次传输重试预算）
3. **客户端难以诊断真实失败原因**（404/503 统一收敛为 502，丢失上游语义）

## 问题分类与优化建议

### 1. 【高优先级】错误分类不一致导致路由策略行为差异

**问题现象**：
- **聚合 API 路径**（`aggregate_api.rs:2446`）：`!upstream.status().is_success()` 统一当作"可重试失败"，先吃 3 次传输重试预算（`AGGREGATE_API_RETRY_ATTEMPTS_PER_CHANNEL=3`），再 failover 下一个候选。**即使是 400/401/403 等客户端错误也会重试 3 次**。
- **账号池路径**（`candidate_executor.rs` + `account_status.rs:274 analyze_gateway_error`）：先按错误语义分类（Deactivation/UsageLimit/Timeout/Other），**可操作类**（账号停用、用量耗尽）才 failover，**Other 类（无法识别）直接终态返回**。

**根本原因**：
聚合 API 路径缺少"客户端错误提前终止"分支。当前逻辑只检查 `aggregate_api_terminal_failure_status`（仅识别 413 请求体过大）和容量错误，**其他所有非 2xx 都走传输重试 → failover 流程**。

**优化方案**：
```rust
// aggregate_api.rs:2446 之后立即插入客户端错误分类
if !upstream.status().is_success() {
    cooldown_eligible_failure = true;
    let status_code = upstream.status().as_u16();
    
    // 新增：客户端错误提前终止（与账号池路径对齐）
    if matches!(status_code, 400 | 401 | 402 | 403 | 404 | 405..=499) {
        // 客户端错误：记录但不重试、不换候选（除非是 429 且有 Retry-After）
        let message = aggregate_api_failure_message(...);
        
        // 429 单独判断是否容量类（需 Retry-After 或特定 message）
        if status_code == 429 {
            let is_capacity = is_selected_model_capacity_error(&message)
                || upstream_retry_after.is_some();
            if is_capacity {
                // 走现有容量重放逻辑
                // ...
            } else {
                // 普通 429 限流：终态返回，不重试
                last_failure_status = 429;
                terminal_failure = true;
                break;
            }
        } else {
            // 其他 4xx：终态返回
            last_failure_status = status_code;
            terminal_failure = true;
            break;
        }
    }
    
    // 现有 5xx/transport 重试逻辑继续
    // ...
}
```

**收益**：
- 400/401/403 等客户端错误立即终止，不浪费 3×N 候选的无效重试
- 聚合路径与账号池路径行为对齐，用户在不同路由策略下看到一致的失败模式
- 429 非容量类限流（如 API key quota）也不再吃满重试预算

---

### 2. 【高优先级】传输重试预算过度消耗在客户端错误上

**问题现象**：
当前 `AGGREGATE_API_RETRY_ATTEMPTS_PER_CHANNEL = 3`（每候选 3 次重试）、`MAX_UPSTREAM_CAPACITY_RETRIES = 2`（容量错误 2 次重放）是**独立预算**，但都消耗在同一 deadline 下。若有 5 个聚合候选：
- 最坏情况：**5 候选 × 3 重试 = 15 次上游请求**（全是传输错误）
- 容量错误单候选：1+2=3 次请求后终态（符合预期）
- **但若是 400 错误，也会吃满 3×5=15 次无效重试**

**优化方案**：
1. **客户端错误零重试**（见上一节）
2. **5xx 区分"可重试类"和"终态类"**：
   ```rust
   fn is_retriable_5xx(status_code: u16) -> bool {
       // 可重试：502/503/504 网关/临时不可用/超时
       matches!(status_code, 502 | 503 | 504)
       // 500/501/505+ 通常是上游逻辑错误，重试无效
   }
   ```
3. **传输错误退避**：当前 3 次重试是**立即连续**的（无 sleep），建议加入短退避（50ms/100ms/200ms）避免同一瞬间打爆同一候选。

**收益**：
- 减少无效上游请求 50%+（客户端错误场景）
- 降低端到端延迟（不吃满 3 次重试的 connect timeout）
- 减少 quota 消耗（部分供应商按请求数计费）

---

### 3. 【中优先级】客户端可见状态码映射丢失上游语义

**问题现象**：
- `route_exhaustion_terminal_status`（`proxy.rs:38-50`）把**404/503 统一收敛为 502**（除非 message 包含 "model_not_found"）
- 账号池耗尽 → 503 "no_available_account" → 502
- 聚合候选全冷却/零余额 → 502 "all aggregate apis are cooling down"

**客户端视角**：
- 502 Bad Gateway 语义模糊：是网关自身故障？上游不可达？还是账号/配额耗尽？
- **真实 404（模型不存在）和 503（临时不可用）被抹平**，客户端无法区分"换个模型"还是"稍后重试"

**优化方案**：
```rust
// 保留更多上游语义状态码
match status_code {
    404 if message.contains("model_not_found") => 404, // 保持
    404 => 404, // 新增：模型路由无候选也返回 404，客户端明确知道资源不存在
    503 if message.contains("no_available_account") 
        || message.contains("cooling down") 
        || message.contains("zero balance") => 503, // 保持 503，客户端可重试
    503 => 503, // 保持其他 503
    401 | 402 | 403 | 429 => status_code, // 保持客户端错误原状（已有）
    _ => 502, // 其他才收敛为 502
}
```

**权衡**：
- **优点**：客户端可根据 404/503 做针对性处理（404 → 换模型，503 → exponential backoff 重试）
- **风险**：若客户端依赖"所有路由失败 = 502"做判断，改动后需兼容性测试
- **建议**：通过配置开关控制（`CODEXMANAGER_PRESERVE_UPSTREAM_STATUS=true`），默认保守（当前 502 收敛），允许新客户端 opt-in

---

### 4. 【中优先级】容量错误处理不对称

**问题现象**：
- **聚合 API**：容量错误（"selected model is at capacity"）→ 同候选重放 2 次（500ms~1s 抖动退避 + 尊重 Retry-After ≤2s）→ 耗尽后 **terminal_failure = true，不换候选、不进模型降级**
- **账号池**：同样容量错误 → 同账号重放 2 次 → 耗尽后换下一个账号（通过 `capacity_retry_budget_remaining` 在 `response_finalize.rs` 实现）

**不对称原因**：
聚合 API 假设"容量打满的模型在所有供应商都打满"，故意不换候选避免扩散。但这在**多供应商、不同区域、不同账号池**场景下不成立。

**优化方案**：
```rust
// aggregate_api.rs 容量错误耗尽后，允许 failover 到下一个候选
if capacity_budget_exhausted {
    // 旧逻辑：terminal_failure = true; break;
    // 新逻辑：记录容量失败，继续下一个候选（但不再给该候选容量预算）
    super::super::super::record_gateway_upstream_capacity_exhausted();
    last_failure_status = 502;
    cooldown_eligible_failure = true; // 冷却该候选
    break; // 跳出当前候选的重试循环，进入下一个候选
}
```

**配置化**：
```rust
const AGGREGATE_API_CAPACITY_FAILOVER_ENABLED: bool = env_flag("CODEXMANAGER_CAPACITY_FAILOVER");
```

**收益**：
- 多区域部署场景：us-east 打满后自动 failover 到 eu-west
- 账号池混合场景：供应商 A 打满后尝试供应商 B（当前会直接 502）

---

### 5. 【低优先级】冷却与零余额候选提前过滤效率

**问题现象**：
当前冷却名单在**每个候选执行前** check（`should_skip_candidate`），跳过的候选会累加 `skipped_cooldown` 计数。若所有候选都冷却：
- 聚合路径：遍历完所有候选后才判定"全冷却" → 502
- 账号池路径：同样遍历完才 Exhausted

**优化方案**：
在候选循环**开始前**统一过滤：
```rust
// aggregate_api.rs:1924 之后
let active_candidates = aggregate_api_candidates
    .into_iter()
    .filter(|candidate| {
        !is_cooling_down(&candidate.id)
            && !zero_balance_blocked_ids.contains(&candidate.id)
    })
    .collect::<Vec<_>>();

if active_candidates.is_empty() {
    // 提前 502，不进入候选循环
    let message = if aggregate_api_candidates.iter().all(|c| is_cooling_down(&c.id)) {
        "all aggregate apis are cooling down"
    } else {
        "all aggregate apis are blocked by zero balance"
    };
    respond_error(request, 502, message, Some(trace_id));
    return Ok(AggregateAttemptOutcome::Responded);
}
```

**收益**：
- 降低 trace log 噪音（不再为跳过的候选记录 `candidate_skipped_cooldown`）
- 微小性能提升（避免循环和 lock 争用）

---

### 6. 【观测性】错误分类可见性不足

**问题现象**：
客户端收到 502 时，无法从 HTTP 响应判断：
- 是传输错误？上游 5xx？还是账号/配额耗尽？
- 重试了几次？在哪些候选上失败？

**优化方案**：
1. **结构化错误响应**（需客户端配合）：
   ```json
   {
     "error": {
       "code": "upstream_unavailable",
       "message": "all aggregate apis are cooling down",
       "type": "gateway_error",
       "details": {
         "attempted_candidates": ["agg-a", "agg-b"],
         "failure_reasons": ["capacity_exhausted", "cooldown"],
         "retry_after_seconds": 300
       }
     }
   }
   ```

2. **HTTP 响应头**（向后兼容）：
   ```http
   X-CodexManager-Error-Class: capacity_exhausted
   X-CodexManager-Attempted-Candidates: 3
   X-CodexManager-Retry-After: 300
   ```

3. **增强 trace log**（已有 `attempted_aggregate_api_ids`，可补充失败原因分类）：
   ```rust
   super::super::super::write_request_log(
       // ...
       failure_class: Some("capacity_exhausted"), // 新增
       attempted_candidates_count: Some(attempted_account_ids.len()), // 新增
   );
   ```

---

## 实施优先级与风险评估

| 优化项 | 优先级 | 预期收益 | 破坏性风险 | 实施复杂度 |
|--------|--------|----------|------------|------------|
| 1. 客户端错误提前终止 | 🔴 高 | 减少 50%+ 无效重试 | 低（行为更合理） | 中 |
| 2. 5xx 分类 + 退避 | 🔴 高 | 降低延迟、减少 quota 消耗 | 低 | 低 |
| 3. 状态码映射保留语义 | 🟡 中 | 客户端可针对性重试 | **中**（需兼容性测试） | 中 |
| 4. 容量错误 failover | 🟡 中 | 多区域/多供应商可用性 | 中（需配置开关） | 中 |
| 5. 冷却名单提前过滤 | 🟢 低 | 微小性能提升 | 低 | 低 |
| 6. 错误分类可见性 | 🟢 低 | 诊断体验提升 | 低（新增字段） | 低 |

---

## 测试覆盖建议

当前测试覆盖（`aggregate_api_tests.rs`）：
- ✅ 容量错误 429 重放 → 502
- ✅ 503 纯文本容量错误
- ✅ 空候选列表 → 502
- ✅ Chat 流 preflight failover
- ❌ **缺失**：400/401/403 客户端错误场景（当前会吃 3 次重试）
- ❌ **缺失**：429 非容量类限流（如 API key quota）
- ❌ **缺失**：500/501 非可重试 5xx

**补充测试用例**：
```rust
#[test]
fn aggregate_api_client_error_400_terminates_immediately() {
    // 验证 400 不重试、不 failover
}

#[test]
fn aggregate_api_401_unauthorized_terminates_without_retry() {
    // 验证 401 立即终态
}

#[test]
fn aggregate_api_429_non_capacity_rate_limit_terminates() {
    // 429 但无 Retry-After 且 message 非 capacity → 终态
}

#[test]
fn aggregate_api_500_internal_error_does_not_retry() {
    // 500 非 502/503/504 → 不重试或只重试 1 次
}
```

---

## 配置化开关建议

为降低破坏性风险，建议通过环境变量控制新行为：

```rust
// runtime_config.rs
pub(crate) fn aggregate_api_client_error_zero_retry() -> bool {
    env_flag("CODEXMANAGER_AGGREGATE_CLIENT_ERROR_ZERO_RETRY")
        .unwrap_or(true) // 默认开启（新行为更合理）
}

pub(crate) fn preserve_upstream_status_codes() -> bool {
    env_flag("CODEXMANAGER_PRESERVE_UPSTREAM_STATUS")
        .unwrap_or(false) // 默认关闭（向后兼容）
}

pub(crate) fn capacity_error_failover_enabled() -> bool {
    env_flag("CODEXMANAGER_CAPACITY_FAILOVER")
        .unwrap_or(false) // 默认关闭（需观察多区域场景）
}
```

---

## 总结

CodexManager 网关的失败处理逻辑**在正确性上没有致命缺陷**（容量错误、冷却、超时处理都符合设计意图），但在**效率、一致性、可观测性**三方面有显著优化空间：

1. **效率**：客户端错误零重试 + 5xx 分类可减少 50% 无效上游请求
2. **一致性**：聚合路径与账号池路径错误分类对齐，消除路由策略行为差异
3. **可观测性**：保留上游语义状态码 + 结构化错误响应，客户端可针对性处理

**最小改动、最大收益的实施路径**：
- Phase 1（2 周）：优化 1+2（客户端错误 + 5xx 分类），补充测试用例
- Phase 2（1 周）：观测性增强（响应头 + trace log）
- Phase 3（按需）：状态码映射 + 容量 failover（需配置开关 + A/B 测试）

**关键指标监控**：
- `gateway_failover_attempt_total`（期望：客户端错误场景下降 50%）
- `request_duration_seconds`（期望：P95 下降 10-20%）
- `upstream_requests_total / client_requests_total`（期望：比值从 2-3 降至 1.5-2）
- `terminal_status_code` 分布（期望：404/503 占比上升，502 占比下降）
