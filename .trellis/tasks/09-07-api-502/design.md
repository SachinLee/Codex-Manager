# 技术设计：优化聚合 API 故障切换策略

## 设计概览

本设计针对 4 个结构性缺陷提供最小化改动的修复方案，优先保证请求成功而非早终止。

**设计原则**：
1. **向后兼容**：新行为通过环境变量开关控制，默认开启但可回退
2. **最小侵入**：复用现有机制（`pending_failover_request`、`cooldown_eligible_failure`）
3. **可观测**：每个关键决策点增加 trace log

## 核心修改点

### 修改 1: FR1 — 零交付流中断 failover【P0】

**文件**：`crates/service/src/gateway/observability/http_bridge/delivery.rs`

**当前逻辑**（简化）：
```rust
// deliver.rs ~ L429, L471
HttpBridgeDeliveryOutcome {
    ...
    pending_failover_request: None, // <-- 硬编码 None
}
```

**修改方案**：

```rust
// delivery.rs 新增函数
fn should_allow_zero_delivery_failover(
    delivered_status_code: Option<u16>,
    stream_incomplete: bool,
    event_count: usize,
) -> bool {
    // 环境变量开关
    if !crate::runtime::process_env::aggregate_api_zero_delivery_failover_enabled() {
        return false;
    }
    
    // 判断条件：2xx + 零语义内容 + 流不完整
    delivered_status_code == Some(200)
        && event_count == 0
        && stream_incomplete
}

// 在 respond_with_upstream 的 stream 处理分支修改
let pending_failover_request = if should_allow_zero_delivery_failover(
    delivered_status_code,
    stream_incomplete,
    event_count, // 从 stream_reader 获取
) {
    log::info!(
        "event=aggregate_api_zero_delivery_failover trace_id={} status={} events={}",
        trace_id, delivered_status_code.unwrap_or(0), event_count
    );
    Some(request) // <-- 释放请求给外层
} else {
    None
};

HttpBridgeDeliveryOutcome {
    ...
    pending_failover_request,
}
```

**外层处理**（已存在，无需修改）：
`aggregate_api.rs:3062-3133` 已有 `pending_failover_request` 的容量重放逻辑，直接复用。

**环境变量**：
```rust
// runtime/process_env.rs
pub fn aggregate_api_zero_delivery_failover_enabled() -> bool {
    env_flag("CODEXMANAGER_AGGREGATE_ZERO_DELIVERY_FAILOVER").unwrap_or(true)
}
```

**测试**：
- 单元测试 `delivery.rs`: mock stream_reader 返回 event_count=0 + stream_incomplete → pending_failover_request = Some
- 集成测试 `aggregate_api_tests.rs`: 候选 A 返回 200 后立即断流 → failover 到 B

---

### 修改 2: FR2 — 每候选首次尝试保底【P0】

**文件**：`crates/service/src/gateway/upstream/protocol/aggregate_api.rs`

**当前逻辑**（L~2311）：
```rust
for attempt_idx in 0..=max_attempts_per_channel {
    if is_expired(request_deadline) { // <-- 无条件检查
        // respond 502 timeout, return (不再尝试后续候选)
    }
    ...
}
```

**修改方案 A（推荐）**：

```rust
for attempt_idx in 0..=max_attempts_per_channel {
    // 首次 attempt 保底，不受 deadline 限制
    let should_check_deadline = if attempt_idx == 0 {
        !crate::runtime::process_env::aggregate_api_guarantee_first_attempt()
    } else {
        true
    };
    
    if should_check_deadline && is_expired(request_deadline) {
        log::info!(
            "event=aggregate_api_deadline_exceeded trace_id={} candidate_idx={} attempt_idx={}",
            trace_id, candidate_idx, attempt_idx
        );
        // 对于首次 attempt，即使 deadline 过期也继续（保底）
        // 对于重试 attempt，break 到下一候选而非直接 respond
        if attempt_idx > 0 {
            cooldown_eligible_failure = true;
            break; // 换下一候选
        }
        // attempt_idx == 0 且保底开启 → 继续执行
    }
    ...
}
```

**但注意**：如果保底开启，总耗时可能 > 300s。需权衡：
- **方案 A1（更激进）**：首次 attempt 完全不检查 deadline
- **方案 A2（保守）**：首次 attempt 检查 deadline，但 break 而非 respond（给下一候选机会）

**推荐 A2**（平衡可用性和延迟）：

```rust
if is_expired(request_deadline) {
    if attempt_idx == 0 && crate::runtime::process_env::aggregate_api_guarantee_first_attempt() {
        // 首次 attempt deadline 过期 → break 到下一候选（而非 respond 终止）
        log::info!(
            "event=aggregate_api_first_attempt_deadline_break trace_id={} candidate_idx={}",
            trace_id, candidate_idx
        );
        cooldown_eligible_failure = true;
        break;
    } else if attempt_idx > 0 {
        // 重试 attempt deadline 过期 → 也 break
        cooldown_eligible_failure = true;
        break;
    } else {
        // 保底未开启，原逻辑：respond 502
        attempt_records.push(AggregateApiAttemptRecord {
            api_id: candidate_id.clone(),
            position: candidate_position,
            outcome: AttemptOutcome::Failure,
            failure_category: Some("timeout".to_string()),
        });
        // ... respond_error(request, 502, "aggregate api request timeout", ...)
        return Ok(AggregateAttemptOutcome::Responded { attempt_records });
    }
}
```

**环境变量**：
```rust
// runtime/process_env.rs
pub fn aggregate_api_guarantee_first_attempt() -> bool {
    env_flag("CODEXMANAGER_AGGREGATE_GUARANTEE_FIRST_ATTEMPT").unwrap_or(true)
}
```

**测试**：
- 单元测试：mock deadline 在候选 A 第一次 attempt 中过期 → break 而非 respond → 候选 B 得到尝试

---

### 修改 3: FR3 — 传输重试加退避【P1】

**文件**：`crates/service/src/gateway/upstream/protocol/aggregate_api.rs`

**当前逻辑**（L~2618-2626）：
```rust
UpstreamFailureDecision::RetrySameCandidate => {
    if transport_retry_budget_remaining > 0 {
        next_attempt_kind = SPEND_ATTEMPT_KIND_TRANSPORT_RETRY;
        transport_retry_budget_remaining = transport_retry_budget_remaining.saturating_sub(1);
        continue; // <-- 立即重试
    }
    cooldown_eligible_failure = true;
    break;
}
```

**修改方案**：

```rust
UpstreamFailureDecision::RetrySameCandidate => {
    if transport_retry_budget_remaining > 0 {
        next_attempt_kind = SPEND_ATTEMPT_KIND_TRANSPORT_RETRY;
        let retry_attempt = AGGREGATE_API_RETRY_ATTEMPTS_PER_CHANNEL - transport_retry_budget_remaining;
        transport_retry_budget_remaining = transport_retry_budget_remaining.saturating_sub(1);
        
        // 指数退避：50ms, 100ms, 200ms
        let backoff_ms = 50u64 * 2u64.pow(retry_attempt as u32);
        let backoff = std::time::Duration::from_millis(backoff_ms);
        
        log::debug!(
            "event=aggregate_api_retry_backoff trace_id={} candidate_id={} retry_attempt={} backoff_ms={}",
            trace_id, candidate_id, retry_attempt, backoff_ms
        );
        
        tokio::time::sleep(backoff).await;
        continue;
    }
    cooldown_eligible_failure = true;
    break;
}
```

**测试**：
- 单元测试：mock 重试 3 次，验证 sleep 调用参数为 50, 100, 200
- trace log 验证：生产环境 trace 日志时间戳间隔符合退避

---

### 修改 4: FR4 — 全冷却返回 503【P1】

**文件**：`crates/service/src/gateway/upstream/protocol/aggregate_api.rs`

**当前逻辑**（L~1990-1993）：
```rust
respond_error(request, 502, message.as_str(), Some(trace_id));
```

**修改方案**：

```rust
// 新增 respond_error_with_retry_after 函数
fn respond_error_with_retry_after(
    request: Request,
    status: u16,
    message: &str,
    retry_after_secs: Option<u64>,
    trace_id: Option<&str>,
) {
    let mut response = respond_error(request, status, message, trace_id);
    if let Some(secs) = retry_after_secs {
        response.headers_mut().insert(
            hyper::header::RETRY_AFTER,
            hyper::header::HeaderValue::from_str(&secs.to_string()).unwrap(),
        );
    }
    response
}

// 调用点修改
respond_error_with_retry_after(
    request,
    503, // <-- 改为 503
    message.as_str(),
    Some(300), // Retry-After: 300 (5 分钟，与冷却时长一致)
    Some(trace_id)
);
```

**测试**：
- 单元测试：所有候选冷却 → 响应 503 + `Retry-After: 300`

---

### 修改 5: FR5 — 失败清除会话亲和【P1】

**文件**：`crates/service/src/gateway/upstream/protocol/aggregate_api.rs`

**当前逻辑**（L~3344-3368）：
```rust
if succeeded {
    if affinity_binding_success {
        // 更新绑定
        update_aggregate_api_affinity_binding(...);
    }
}
```

**修改方案**：

```rust
if succeeded {
    if affinity_binding_success {
        // 成功 → 更新绑定
        update_aggregate_api_affinity_binding(...);
    } else if aggregate_api_affinity.is_some() {
        // 失败 → 清除绑定（尤其对于 post-bridge 终态失败）
        log::info!(
            "event=aggregate_api_affinity_clear_on_failure trace_id={} candidate_id={}",
            trace_id, candidate_id
        );
        super::super::super::clear_aggregate_api_affinity_binding(
            storage,
            trace_id,
            aggregate_api_affinity.as_ref().unwrap(),
        );
    }
}
```

需要在 `crates/service/src/gateway/routing/aggregate_api_affinity.rs` 新增：

```rust
pub(crate) fn clear_aggregate_api_affinity_binding(
    storage: &Storage,
    trace_id: &str,
    affinity: &AggregateApiAffinity,
) {
    let route_hash = derive_affinity_route_hash(&affinity.route_id);
    let mut state = crate::lock_utils::lock_recover(&AFFINITY_BINDINGS, "affinity_bindings");
    state.remove(&route_hash);
    log::info!(
        "event=aggregate_api_affinity_cleared trace_id={} route_id={} route_hash={}",
        trace_id, affinity.route_id, route_hash
    );
}
```

**测试**：
- 单元测试：候选 A 成功绑定 → 请求 2 → A 失败 → 验证绑定已清除
- 集成测试：请求 3 应公平选择候选（而非粘在 A）

---

### 修改 6（可选）: FR6 — 降低重试预算【P2】

**文件**：`crates/service/src/gateway/upstream/protocol/aggregate_api.rs`

**当前**：
```rust
const AGGREGATE_API_RETRY_ATTEMPTS_PER_CHANNEL: usize = 3;
```

**修改**：
```rust
fn aggregate_api_retry_attempts_per_channel() -> usize {
    std::env::var("CODEXMANAGER_AGGREGATE_TRANSPORT_RETRY_ATTEMPTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1) // <-- 默认降为 1（首次 + 1 次重试）
}
```

在循环初始化时使用：
```rust
let mut transport_retry_budget_remaining = aggregate_api_retry_attempts_per_channel();
```

---

## 数据流图

### 修改前（当前）：

```
客户端请求
  ↓
候选 A (attempt 1)
  ↓ 上游 200 → SSE 读取中断
  ↓ bridge: final_error = Some
  ↓ succeeded = true; break
  ↓ return Responded { 502 }  ← ❌ 直接终止
客户端收到 502
```

### 修改后（FR1）：

```
客户端请求
  ↓
候选 A (attempt 1)
  ↓ 上游 200 → SSE 读取中断
  ↓ bridge: event_count=0, stream_incomplete=true
  ↓ pending_failover_request = Some(request)  ← ✅ 释放请求
  ↓ 外层检测到 pending_failover_request
  ↓
候选 B (attempt 1)
  ↓ 上游 200 → 正常完成
  ↓ return Responded { 200 }
客户端收到 200
```

---

## 配置项汇总

| 环境变量 | 默认值 | 说明 |
|---|---|---|
| `CODEXMANAGER_AGGREGATE_ZERO_DELIVERY_FAILOVER` | `true` | FR1: 零交付流中断 failover |
| `CODEXMANAGER_AGGREGATE_GUARANTEE_FIRST_ATTEMPT` | `true` | FR2: 每候选首次尝试保底 |
| `CODEXMANAGER_AGGREGATE_TRANSPORT_RETRY_ATTEMPTS` | `1` | FR6: 传输重试预算（可选） |

---

## 回滚策略

若生产环境出现意外行为：
1. **FR1/FR2 导致延迟增加**：设置对应环境变量为 `false` 立即回退
2. **FR3 退避过长**：调整 `backoff_ms` 计算公式（代码热修复）
3. **FR4 客户端不识别 503**：回退为 502（一行改动）
4. **FR5 清除过激**：条件改为仅 post-bridge 终态失败才清除

每个修改点都是独立的，可单独启用/禁用。

---

## 性能影响评估

| 修改 | 预期延迟影响 | 吞吐影响 | 缓解 |
|---|---|---|---|
| FR1 零交付 failover | +0-200ms（一次额外上游尝试） | 无 | 只对零交付流触发 |
| FR2 保底尝试 | +0-300ms（最坏情况多一个候选） | 无 | 仅首次 attempt |
| FR3 退避 | +350ms（50+100+200） | 无 | 避免无效重试，实际可能减少总延迟 |
| FR4 503 返回 | 0（仅改状态码） | 改善（客户端延迟重试） | - |
| FR5 清除亲和 | 0 | 无 | - |

**总体评估**：P95 延迟增幅预计 < 10%，但成功率提升 70%+，ROI 极高。

---

## 测试策略

### 单元测试（新增）
- `delivery.rs`: `test_zero_delivery_failover_enabled` / `test_zero_delivery_failover_disabled`
- `aggregate_api.rs`: `test_guarantee_first_attempt_deadline_break` / `test_transport_retry_backoff`
- `aggregate_api_cooldown.rs`: `test_all_cooling_returns_503`
- `aggregate_api_affinity.rs`: `test_clear_affinity_on_failure`

### 集成测试（修改现有）
- `aggregate_api_tests.rs`: `chat_nonstream_malformed_response_fails_over_to_later_candidate` — 验证 FR1 对 responses 协议生效
- 新增：`responses_zero_delivery_failover` — responses 流立即断开 → failover

### 手工验证（Staging）
1. 部署到 staging
2. 使用 14 万 token prompt 请求 `/v1/responses`
3. 手动断开第一个候选的连接（或让其返回 200 后立即断流）
4. 验证请求自动切换到第二个候选并成功

### 生产灰度
1. 10% 流量开启 FR1+FR2+FR4+FR5（保守配置：FR3 禁用）
2. 观察 24h：502 率、P95 延迟、上游重试次数
3. 若指标符合预期 → 50% → 100%
4. 100% 后再开启 FR3（退避）

---

## 关键代码位置速查

| 修改点 | 文件 | 行号（大约） | 函数 |
|---|---|---|---|
| FR1 零交付判断 | `gateway/observability/http_bridge/delivery.rs` | ~167, ~429 | `respond_with_upstream` |
| FR2 deadline 检查 | `gateway/upstream/protocol/aggregate_api.rs` | ~2311 | `proxy_aggregate_request` (attempt 循环顶部) |
| FR3 退避 | `gateway/upstream/protocol/aggregate_api.rs` | ~2620 | `proxy_aggregate_request` (RetrySameCandidate 分支) |
| FR4 503 返回 | `gateway/upstream/protocol/aggregate_api.rs` | ~1990 | `proxy_aggregate_request` (all cooling 分支) |
| FR5 清除亲和 | `gateway/upstream/protocol/aggregate_api.rs` | ~3350 | `proxy_aggregate_request` (succeeded 块) |

---

## 下一步

设计已完成，下一步：
1. 编写 `implement.md`（逐文件逐函数的修改清单）
2. 实现 Phase 2（FR1+FR2+FR4+FR5）
3. 单元测试 + 集成测试
4. Staging 验证
5. 生产灰度发布
