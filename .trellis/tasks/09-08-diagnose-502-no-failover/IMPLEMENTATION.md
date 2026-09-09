# 实施记录：Aggregate API SSE 终态候选轮转

**日期**: 2026-09-08  
**状态**: 核心实现完成，测试验证受阻  
**实施者**: Main session (子代理卡住后接管)

## 完成的工作

### 1. 核心实现 ✅

**文件**: `crates/service/src/gateway/upstream/protocol/aggregate_api.rs`

**修改**: 在 `proxy_aggregate_request()` 的上游响应处理路径中，为流式 Responses 协议添加pre-delivery preflight检测（2963-2990行）

```rust
} else if upstream_protocol == crate::gateway::UpstreamProtocol::Responses
    && is_stream
{
    // 流式 Responses 上游：delivery 前做有界 preflight。
    // 尚未产出任何语义事件前的 SSE 终态错误回到候选 failover；
    // 一旦首个语义事件已交付则不再重放。
    use super::super::proxy_pipeline::stream_preflight::{
        preflight_stream_response, StreamPreflightOutcome,
    };
    match preflight_stream_response(
        GatewayUpstreamResponse::Blocking(upstream),
        path,
        true, // upstream_is_stream
        candidate_idx + 1 < total_candidates, // has_more_candidates
    ) {
        StreamPreflightOutcome::Ready(upstream) => upstream,
        StreamPreflightOutcome::Failover(message)
        | StreamPreflightOutcome::StatusFailover { message, .. }
        | StreamPreflightOutcome::RetryUsageNotice(message)
        | StreamPreflightOutcome::TransportFailover(message) => {
            last_attempt_url = Some(base_upstream_url.to_string());
            last_attempt_supplier_name = candidate_supplier_name.clone();
            last_attempt_error = Some(message);
            last_failure_status = 502;
            cooldown_eligible_failure = true;
            break; // 退出当前候选内层循环，外层继续下一候选
        }
    }
}
```

**关键设计修正**:
- ❌ **原计划**: 在 `respond_with_upstream()` 后依赖 `pending_failover_request` 分类SSE terminal
- ✅ **实际实施**: 在bridge交付前调用 `preflight_stream_response()` 预检SSE前缀

**原因**: `pending_failover_request` 在SSE `response.failed`/`error` terminal事件中不可用，因为delivery已消费请求且设置了`saw_terminal=true`。安全的seam是pre-delivery preflight。

### 2. 测试添加 ⚠️

**文件**: `crates/service/src/gateway/upstream/protocol/aggregate_api_tests.rs`

**新增测试** (942-1026行):
- `aggregate_sse_rate_limit_fails_over_to_next_candidate()` - SSE rate_limit_exceeded应切换候选
- `aggregate_sse_all_candidates_fail_returns_terminal_error()` - 所有候选SSE失败应返回一次终态
- `test_candidate_with_url()` helper函数

**测试状态**: 添加但未能成功运行（所有cargo test命令超时）

### 3. 编译验证 ✅

```bash
$ cargo build -p codexmanager-service --lib
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 19.84s
```

**编译成功**，库代码无语法/类型错误。

## 未完成/受阻的工作

### 测试验证 ❌

**问题**: 所有测试命令（包括现有回归测试）均超时（60-120秒），无法验证：
- 新SSE failover行为
- 现有HTTP 502 failover保持不变
- 现有401 failover保持不变

**尝试的测试**:
```bash
# 全部超时
cargo test -p codexmanager-service --lib "aggregate_sse_rate_limit_fails_over_to_next_candidate"
cargo test -p codexmanager-service --lib "aggregate_502_rate_limit_exceeded_code_fails_over_to_next_candidate"  
cargo test -p codexmanager-service --lib "aggregate_401_fails_over_to_next_candidate"
```

**可能原因**:
1. Mock服务器与stream preflight交互存在死锁
2. 测试helper `run_upstream_scenario_with_stream()` 的SSE响应格式不正确
3. Windows环境下网络/threading行为差异
4. 测试编译时间过长（后台job bg_4超时124秒仍在编译）

## 设计决策记录

### 修改边界确认

**原设计文档建议**: `aggregate_api.rs:3208` (post-bridge分类)  
**实际实施**: `aggregate_api.rs:2963-2990` (pre-delivery preflight)  

**决策**: Pre-delivery是唯一安全的seam，因为：
- SSE terminal事件后`pending_failover_request`不可用
- Bridge delivery已消费request
- Preflight可以在语义内容交付前检测并重放stream

### 复用现有Preflight

**选择**: 直接调用 `preflight_stream_response()` from `stream_preflight.rs`

**优势**:
- 已有完整的SSE prefix分类逻辑
- 支持 `response.failed`, `error`, usage notice检测
- 返回`StreamPreflightOutcome`枚举，语义清晰
- 传递`has_more_candidates`标志控制failover语义

**保持不变**:
- HTTP非2xx路径不变（2657-2883）
- Chat preflight独立保持（2940-2962）
- 容量/reasoning guard/spend核算不变
- 候选遍历/重试预算/deadline保持有界

## 变更统计

```
 crates/service/src/gateway/upstream/protocol/aggregate_api.rs       | 28 +++++++
 crates/service/src/gateway/upstream/protocol/aggregate_api_tests.rs | 89 ++++++++++++++++++++++
 2 files changed, 117 insertions(+)
```

## 验证缺口

由于测试超时无法验证：

1. ✅ **代码编译** - 通过
2. ❌ **SSE rate_limit候选切换** - 未运行
3. ❌ **所有候选失败有界终止** - 未运行
4. ❌ **已有输出不重试** - 未运行
5. ❌ **现有HTTP 502回归** - 未运行
6. ❌ **现有401回归** - 未运行
7. ❌ **项目级测试套件** - 跳过（按要求）

## 下一步建议

### 短期 - 测试修复

1. **简化SSE测试**: 不使用mock服务器，直接构造`GatewayUpstreamResponse`并验证preflight分类
2. **Unit test优先**: 测试`preflight_stream_response()`本身而非完整请求流程
3. **调试超时**: 增加日志，定位hang点（可能在stream读取或thread join）
4. **串行执行**: 使用`-- --test-threads=1`避免并发

### 中期 - 验证策略

1. **人工验证**: 部署到测试环境，用真实Aggregate API触发SSE `response.failed`
2. **生产监控**: 观察`gateway_upstream_attempt_events`表，确认多次attempt记录
3. **日志追踪**: 搜索`event=aggregate_api_candidate_failover`日志

### 长期 - 测试基础设施

1. **重构test helpers**: 分离stream构造与完整代理流程
2. **Mock抽象**: 使用in-memory channel而非真实TCP服务器
3. **集成测试分层**: Unit→Integration→E2E，不混在一起

## 关键不变式验证（理论）

基于代码审查，以下不变式应得到保证：

- ✅ **零交付安全**: preflight在bridge前执行，未交付任何客户端内容
- ✅ **有界遍历**: `candidate_idx + 1 < total_candidates`限制候选，break退出内层循环
- ✅ **cooldown触发**: `cooldown_eligible_failure = true`记录失败
- ✅ **spend核算**: 零交付时break前未调用hold/release，外层逻辑处理
- ✅ **请求终态**: preflight失败设置`last_attempt_error`和`last_failure_status = 502`

## 参考

- 原诊断任务: `.trellis/tasks/09-08-diagnose-502-no-failover/prd.md`
- 设计文档: `.trellis/tasks/09-08-diagnose-502-no-failover/design.md`
- 批准记录: `.trellis/tasks/09-08-diagnose-502-no-failover/APPROVAL.md`
- 相关需求: `.trellis/tasks/09-03-gateway-failover-policy-optimization/prd.md`
