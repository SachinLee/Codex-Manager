# PRD: 优化聚合 API 故障切换策略以降低 502 失败率

## 问题陈述

**当前状况**：生产环境出现大量 502 错误（成功率 ~11%），客户端请求失败率远高于上游实际故障率。

**根本原因**：CodexManager 网关的故障切换逻辑存在 4 个关键缺陷，导致上游 10-20% 的偶发故障被放大为客户端 80-90% 的失败率：

1. **P0-1: bridge 后失败一刀切终态** — 上游返回 2xx 后任何失败（流中断、连接断开）都直接返回 502，即使客户端零字节未收到也不尝试下一个候选
2. **P0-2: 流式 deadline 全局共享** — 300 秒 deadline 从请求开始跨所有候选和重试共享，慢候选/多次重试可耗尽整个预算，导致后续候选零机会
3. **P1-1: 连续失败冻结雪崩** — 候选连续失败 5 次→冻结 5 分钟，多个候选同时冻结导致后续请求"所有候选都在冷却"→零上游流量直接 502
4. **P1-2: 会话亲和性放大单点故障** — 失败不更新绑定，同一会话反复打到坏候选

**业务影响**：
- 客户端体验极差（大量 502）
- 上游资源浪费（同候选无退避重试 3 次）
- 运营成本增加（guard retry 重复计费）

## 目标与非目标

### 核心目标
**将上游 10-20% 故障率转化为客户端 < 5% 失败率** — 通过快速故障切换和智能重试，把偶发上游故障对客户端透明化。

### 具体目标
1. **零交付流中断可以 failover** — `/v1/responses` 流式请求在"已 2xx 但未交付语义字节"时断开，应释放请求给下一个候选
2. **每个候选至少一次完整尝试机会** — deadline 不因前一个候选的拖延而剥夺后续候选的机会
3. **冷却全命中返回可重试状态码** — 503 + Retry-After 而非 502，避免客户端立即重试撞墙
4. **失败清除会话亲和绑定** — 避免会话粘在坏候选上
5. **传输重试加短退避** — 避免无效重试浪费 deadline（50/100/200ms 指数退避）

### 非目标
- 不改变 bridge 后**已交付语义内容**的终态行为（无法安全重放）
- 不改变容量错误的故意不 failover 策略（避免扩散）
- 不增加总体重试次数上限（控制延迟）

## 功能需求

### FR1: `/v1/responses` 零交付流中断 failover【P0】

**当前行为**：上游 2xx → SSE 读取中断 → `succeeded = true; break;` → 直接 502 给客户端。

**目标行为**：
- 判断条件：`bridge.delivered_status_code == Some(200)` && `!has_delivered_semantic_content` && stream incomplete
- 动作：设置 `pending_failover_request = Some(request)`，返回给外层候选循环
- 外层：`if let Some(req) = bridge.pending_failover_request` → 继续下一个候选

**验证标准**：
- `/v1/responses` 流式请求，候选 A 返回 200 后立即断流（0 events）→ 自动切候选 B → B 成功 → 客户端收到 200
- 已交付 1+ events 的流中断 → 仍然终态（不可重放）

### FR2: 每候选至少一次完整尝试机会【P0】

**当前行为**：`request_deadline` 全局共享，候选 A 的守卫/传输重试耗尽 300s → 候选 B 循环开始时 `is_expired` → 直接 502。

**目标行为**（三选一，推荐方案 A）：

**方案 A（推荐）**：每候选首次尝试保底不受 deadline 限制
```rust
// 每个候选首次 attempt (attempt_idx == 0) 跳过 deadline 检查
if attempt_idx > 0 && is_expired(request_deadline) {
    // timeout
}
```

**方案 B**：deadline 检查仅在重试前
```rust
// 只在 continue 前检查，首次 attempt 不检查
if is_expired(request_deadline) {
    // 但不立即 respond，而是 break 到下一候选
    cooldown_eligible_failure = true;
    break;
}
```

**方案 C**：per-candidate deadline budget
```rust
let candidate_deadline = request_deadline.map(|d| {
    let remaining = d.saturating_duration_since(Instant::now());
    let per_candidate = remaining / remaining_candidates.max(1);
    Instant::now() + per_candidate.max(Duration::from_secs(30)) // 至少 30s
});
```

**验证标准**：
- 候选 A 守卫重试 + 传输重试耗时 280s → 候选 B 仍可尝试（至少一次）
- 候选 A + B 总耗时 > 300s 是允许的（保证尝试优先于严格 deadline）

### FR3: 传输重试加短退避【P1】

**当前行为**：同候选传输重试 3 次，无退避（立即），快速耗尽 deadline。

**目标行为**：
- 第 1 次重试：等待 50ms
- 第 2 次重试：等待 100ms
- 第 3 次重试：等待 200ms

**实现**：在 `continue` 前 `tokio::time::sleep`。

**验证标准**：trace log 显示重试间隔时间戳符合退避。

### FR4: 冷却全命中返回 503【P1】

**当前行为**：所有候选都在冷却 → 502 "all aggregate apis are cooling down"。

**目标行为**：503 + `Retry-After: 300` header，客户端可延迟重试。

**实现**：修改 `aggregate_api.rs:1990` 附近的 respond_error 调用。

**验证标准**：所有候选冷却时 → 客户端收到 503 + Retry-After header。

### FR5: 失败清除会话亲和绑定【P1】

**当前行为**：`update_aggregate_api_affinity_binding` 仅在成功时调用，失败保持旧绑定。

**目标行为**：
- 候选失败（尤其 post-bridge 终态）→ 清除该会话的亲和绑定
- 下一个请求重新公平选择

**实现**：在 `cooldown_eligible_failure = true` 且未成功时调用 `clear_aggregate_api_affinity_binding`。

**验证标准**：
- 请求 1 绑定 A 成功 → 请求 2 优先 A → A 失败 → 请求 3 不再绑定 A（公平轮转）

### FR6: 降低 5xx 重试预算【P2，可选】

**当前**：同候选 5xx 重试 3 次。
**建议**：降为 1 次（首次 + 1 次重试），更快切换到下一个候选。

**配置化**：环境变量 `CODEXMANAGER_AGGREGATE_TRANSPORT_RETRY_ATTEMPTS` 默认 1。

## 验收标准

### 功能验收
- [ ] FR1: 零交付流中断 → failover 到下一候选（集成测试 + 手工验证）
- [ ] FR2: 每候选至少一次完整尝试机会（单元测试 mock deadline）
- [ ] FR3: 传输重试有退避（trace log 验证）
- [ ] FR4: 全冷却返回 503（单元测试）
- [ ] FR5: 失败清除亲和（单元测试 + 集成测试）

### 业务指标验收（生产环境观察 24h）
- [ ] **客户端 502 率从 ~80% 降至 < 10%**（同等上游故障率下）
- [ ] **端到端 P95 延迟增幅 < 10%**（退避和保底尝试的代价）
- [ ] **上游无效重试次数减少 > 50%**（退避生效）
- [ ] **冷却雪崩期间零流量窗口从分钟级降至 0**（503 + Retry-After）

## 实施风险与缓解

### 风险 1: FR1 误判"零交付"导致重复内容
**缓解**：严格定义 `has_delivered_semantic_content`，使用 stream_readers 现有逻辑（event count > 0）；灰度测试。

### 风险 2: FR2 方案 A 导致总延迟增加
**缓解**：保底仅对首次 attempt，重试仍受 deadline 限制；配置化开关 `CODEXMANAGER_AGGREGATE_GUARANTEE_FIRST_ATTEMPT`。

### 风险 3: 现有测试假设会被打破
**缓解**：先运行全量测试套件，修复受影响的测试（尤其 timeout 相关）。

## 实施计划

### Phase 1: 设计与验证（本任务）
1. 详细设计文档（design.md）
2. 修改点清单（implement.md）
3. 测试策略

### Phase 2: 实现核心修复
1. FR1: 零交付 failover（最高优先）
2. FR2: 保底尝试（最高优先）
3. FR4: 503 返回
4. FR5: 清除亲和

### Phase 3: 性能优化
1. FR3: 退避
2. FR6: 降低重试预算（可选）

### Phase 4: 测试与发布
1. 单元测试 + 集成测试
2. Staging 验证
3. 生产灰度（10% → 50% → 100%）
4. 监控指标 24h

## 参考

- 根因分析：前述对话上下文
- 现有分析文档：`gateway-failover-optimization-analysis.md`
- 现有 planning 任务：`09-03-gateway-failover-policy-optimization`（聚焦 4xx 分类，与本任务互补）
- 关键代码：`crates/service/src/gateway/upstream/protocol/aggregate_api.rs` L2311（deadline check）、L3175-3193（post-bridge 终态）
