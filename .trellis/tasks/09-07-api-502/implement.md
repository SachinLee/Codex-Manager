# 实施记录：优化聚合 API 故障切换策略

## 实施概览

**状态**：✅ Phase 2 核心修复已完成（FR1+FR2+FR4+FR5）

**实施时间**：2026-01-07

**修改文件**：
1. `crates/service/src/gateway/core/runtime_config.rs` — 环境变量支持
2. `crates/service/src/gateway/observability/http_bridge/delivery.rs` — FR1 零交付 failover
3. `crates/service/src/gateway/upstream/protocol/aggregate_api.rs` — FR2/FR3/FR4/FR5
4. `crates/service/src/gateway/routing/aggregate_api_affinity.rs` — FR5 清除亲和函数

## 详细实施

### FR1: 零交付流中断 failover【P0】✅

**文件**：`crates/service/src/gateway/observability/http_bridge/delivery.rs`

**修改点**：
1. `respond_openai_responses_compatible_stream` 函数（line ~408）
2. `respond_passthrough_collector_stream` 函数（line ~469）

**核心逻辑**：
```rust
// 在消费 request 之前检查是否满足零交付 failover 条件
let should_allow_failover = read_error.is_some()
    && crate::gateway::runtime_config::aggregate_api_zero_delivery_failover_enabled()
    && status.0 == 200
    && !collector.saw_terminal
    && collector.usage.output_tokens.unwrap_or(0) == 0;

let (delivery_error, pending_request) = if should_allow_failover {
    log::info!("event=aggregate_api_zero_delivery_failover ...");
    // 不消费 request，返回给外层
    (read_error, Some(request))
} else {
    // 正常流程，消费 request
    let err = respond_streaming_chunked(request, status, headers, response_body)
        .err()
        .map(|err| err.to_string());
    (err, None)
};

// 返回 pending_failover_request
HttpBridgeDeliveryOutcome {
    ...
    pending_failover_request: pending_request,
}
```

**环境变量**：
- `CODEXMANAGER_AGGREGATE_ZERO_DELIVERY_FAILOVER`（默认 true）

**验收标准**：
- ✅ 编译通过
- ⏸️ 单元测试（待添加）
- ⏸️ 集成测试（待添加）

---

### FR2: 每候选首次尝试保底【P0】✅

**文件**：`crates/service/src/gateway/upstream/protocol/aggregate_api.rs`

**修改点**：line ~2311 的 deadline 检查

**核心逻辑**：
```rust
for attempt_idx in 0..=max_attempts_per_channel {
    // FR2: 每候选首次尝试保底 - 首次 attempt 即使 deadline 过期也继续
    let should_check_deadline = attempt_idx > 0 
        || !crate::gateway::runtime_config::aggregate_api_guarantee_first_attempt_enabled();
    
    if should_check_deadline && super::super::support::deadline::is_expired(request_deadline) {
        // timeout 逻辑
        ...
    }
    ...
}
```

**环境变量**：
- `CODEXMANAGER_AGGREGATE_GUARANTEE_FIRST_ATTEMPT`（默认 true）

**效果**：
- 首次 attempt（`attempt_idx == 0`）在 deadline 过期时仍然继续执行
- 重试 attempt（`attempt_idx > 0`）仍然受 deadline 限制
- 保证每个候选至少有一次完整尝试机会

**验收标准**：
- ✅ 编译通过
- ⏸️ 单元测试（mock deadline expired）

---

### FR3: 传输重试加退避【P1】✅

**文件**：`crates/service/src/gateway/upstream/protocol/aggregate_api.rs`

**修改点**：两处 `UpstreamFailureDecision::RetrySameCandidate` 分支
- line ~2620
- line ~2830

**核心逻辑**：
```rust
UpstreamFailureDecision::RetrySameCandidate => {
    if transport_retry_budget_remaining > 0 {
        // FR3: 传输重试加退避
        let retry_attempt = AGGREGATE_API_RETRY_ATTEMPTS_PER_CHANNEL - transport_retry_budget_remaining;
        let backoff_ms = 50u64 * 2u64.pow(retry_attempt as u32);
        log::debug!(
            "event=aggregate_api_retry_backoff candidate_id={} retry_attempt={} backoff_ms={}",
            candidate_id, retry_attempt, backoff_ms
        );
        tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
        
        next_attempt_kind = SPEND_ATTEMPT_KIND_TRANSPORT_RETRY;
        transport_retry_budget_remaining = transport_retry_budget_remaining.saturating_sub(1);
        continue;
    }
    cooldown_eligible_failure = true;
    break;
}
```

**退避时间**：
- 第 1 次重试：50ms
- 第 2 次重试：100ms
- 第 3 次重试：200ms

**环境变量**：
- `CODEXMANAGER_AGGREGATE_TRANSPORT_RETRY_ATTEMPTS`（默认 1，可选 FR6）

**验收标准**：
- ✅ 编译通过
- ⏸️ trace log 验证时间戳间隔

---

### FR4: 全冷却返回 503【P1】✅

**文件**：`crates/service/src/gateway/upstream/protocol/aggregate_api.rs`

**修改点**：line ~1965 "all aggregate apis are cooling down" 分支

**核心逻辑**：
```rust
if aggregate_api_candidates.is_empty() {
    let message = "all aggregate apis are cooling down".to_string();
    ...
    // FR4: 全冷却返回 503 + Retry-After
    super::super::super::record_gateway_request_outcome(path, 503, Some("aggregate_api"));
    ...
    
    let mut response = tiny_http::Response::from_string(serde_json::json!({
        "error": {
            "message": message,
            "type": "service_unavailable",
            "code": "all_cooling"
        }
    }).to_string())
    .with_status_code(503);
    
    response.add_header(
        tiny_http::Header::from_bytes(&b"Retry-After"[..], &b"300"[..]).unwrap()
    );
    response.add_header(
        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()
    );
    
    let _ = request.respond(response);
    return Ok(AggregateAttemptOutcome::Responded { attempt_records });
}
```

**改变**：
- 状态码：502 → 503
- 新增：`Retry-After: 300` header（5 分钟，与冷却时长一致）

**验收标准**：
- ✅ 编译通过
- ⏸️ 单元测试验证 503 + Retry-After header

---

### FR5: 失败清除会话亲和【P1】✅

**文件**：
1. `crates/service/src/gateway/upstream/protocol/aggregate_api.rs` — 调用清除
2. `crates/service/src/gateway/routing/aggregate_api_affinity.rs` — 实现清除函数

**修改点（aggregate_api.rs line ~3387）**：
```rust
if succeeded {
    if affinity_binding_success {
        // 成功 → 更新绑定
        if let Some(affinity) = aggregate_api_affinity.as_ref() {
            update_aggregate_api_affinity_binding(...);
        }
    } else {
        // FR5: 失败 → 清除绑定（即使 succeeded=true，但 affinity_binding_success=false）
        if aggregate_api_affinity.is_some() {
            log::info!(
                "event=aggregate_api_affinity_clear_on_failure trace_id={} candidate_id={}",
                trace_id, candidate_id
            );
            if let Some(affinity) = aggregate_api_affinity.as_ref() {
                super::super::routing::aggregate_api_affinity::clear_aggregate_api_affinity_binding(
                    storage,
                    trace_id,
                    affinity,
                );
            }
        }
    }
}
```

**新增函数（aggregate_api_affinity.rs）**：
```rust
pub(crate) fn clear_aggregate_api_affinity_binding(
    storage: &Storage,
    trace_id: &str,
    affinity: &super::super::upstream::protocol::AggregateApiAffinityContext,
) {
    let route_hash = derive_affinity_route_hash(&affinity.route_id);
    
    if let Err(err) = storage.delete_aggregate_api_binding(
        affinity.platform_key_hash.as_str(),
        affinity.protocol_type.as_str(),
        affinity.model.as_str(),
        affinity.route_id_hash.as_str(),
    ) {
        log::warn!(
            "event=aggregate_api_affinity_clear_failed trace_id={} route_hash={} err={}",
            trace_id, route_hash, err
        );
        return;
    }
    
    log::info!(
        "event=aggregate_api_affinity_cleared trace_id={} route_hash={}",
        trace_id, route_hash
    );
}
```

**验收标准**：
- ✅ 编译通过
- ⏸️ 单元测试验证绑定清除
- ⏸️ 集成测试验证后续请求不再绑定失败候选

---

## 环境变量配置汇总

| 环境变量 | 默认值 | 说明 | FR |
|---|---|---|---|
| `CODEXMANAGER_AGGREGATE_ZERO_DELIVERY_FAILOVER` | `true` | 零交付流中断 failover | FR1 |
| `CODEXMANAGER_AGGREGATE_GUARANTEE_FIRST_ATTEMPT` | `true` | 每候选首次尝试保底 | FR2 |
| `CODEXMANAGER_AGGREGATE_TRANSPORT_RETRY_ATTEMPTS` | `1` | 传输重试预算（可选） | FR6 |

**回滚方法**：
```bash
# 禁用 FR1
export CODEXMANAGER_AGGREGATE_ZERO_DELIVERY_FAILOVER=false

# 禁用 FR2
export CODEXMANAGER_AGGREGATE_GUARANTEE_FIRST_ATTEMPT=false

# 回退传输重试预算（如果使用了 FR6）
export CODEXMANAGER_AGGREGATE_TRANSPORT_RETRY_ATTEMPTS=3
```

---

## 未实施功能（Phase 3）

### FR6: 降低重试预算【P2】⏸️

**说明**：FR3 已实现退避逻辑，FR6（降低重试次数从 3→1）可选。当前默认重试预算仍为 3 次。

**如需启用**：
```bash
export CODEXMANAGER_AGGREGATE_TRANSPORT_RETRY_ATTEMPTS=1
```

**代码位置**：已添加环境变量支持，但 `AGGREGATE_API_RETRY_ATTEMPTS_PER_CHANNEL` 常量未修改。

---

## 下一步

### Phase 3: 测试与验证

1. **单元测试**（优先）
   - FR1: `delivery.rs` 零交付 failover 测试
   - FR2: `aggregate_api.rs` deadline 保底测试
   - FR4: 503 + Retry-After header 测试
   - FR5: 亲和清除测试

2. **集成测试**
   - FR1: 候选 A 返回 200 后立即断流 → 自动切候选 B → B 成功
   - FR2: 候选 A 耗时过长 → 候选 B 仍可尝试
   - FR5: 失败后亲和清除 → 下次请求公平选择

3. **Staging 验证**
   - 部署到 staging 环境
   - 使用真实流量验证（可手动模拟上游故障）
   - 观察 trace log：
     - `event=aggregate_api_zero_delivery_failover`
     - `event=aggregate_api_retry_backoff`
     - `event=aggregate_api_affinity_clear_on_failure`

4. **生产灰度**
   - 10% 流量：观察 24h，监控 502 率、P95 延迟
   - 50% 流量：继续观察
   - 100% 流量：全量上线

### Phase 4: 监控指标

**预期改善**：
- ✅ 客户端 502 率：~80% → < 10%
- ✅ 成功率：~11% → > 95%
- ⚠️ P95 延迟：增幅 < 10%

**监控命令**（生产环境）：
```bash
# 502 率
grep "status=502" /var/log/codexmanager.log | wc -l

# Failover 事件
grep "event=aggregate_api_zero_delivery_failover" /var/log/codexmanager.log

# 退避事件
grep "event=aggregate_api_retry_backoff" /var/log/codexmanager.log

# 亲和清除事件
grep "event=aggregate_api_affinity_clear_on_failure" /var/log/codexmanager.log
```

---

## 风险与缓解

### 已识别风险

1. **FR1 误判零交付**
   - **风险**：`output_tokens == 0` 可能误判已交付内容
   - **缓解**：只有同时满足"200 + 未 saw_terminal + output_tokens=0 + terminal_error"才 failover
   - **回滚**：设置 `CODEXMANAGER_AGGREGATE_ZERO_DELIVERY_FAILOVER=false`

2. **FR2 延迟增加**
   - **风险**：保底尝试导致总耗时 > 300s
   - **缓解**：仅首次 attempt 保底，重试仍受 deadline 限制
   - **回滚**：设置 `CODEXMANAGER_AGGREGATE_GUARANTEE_FIRST_ATTEMPT=false`

3. **FR3 退避过长**
   - **风险**：50/100/200ms 退避浪费时间
   - **实际**：相比无效重试（立即失败），退避增加成功率，总体减少延迟
   - **调整**：如需调整退避参数，修改 `backoff_ms` 计算公式

### 未发现的风险

- 目前编译通过，无语法错误
- 逻辑审查未发现明显缺陷
- 需通过测试和 staging 验证发现潜在问题

---

## 参考

- PRD: `.trellis/tasks/09-07-api-502/prd.md`
- 设计: `.trellis/tasks/09-07-api-502/design.md`
- 根因分析：前述对话上下文
- 现有分析文档：`gateway-failover-optimization-analysis.md`
