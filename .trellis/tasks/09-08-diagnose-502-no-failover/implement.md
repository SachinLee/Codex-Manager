# 实施计划：Aggregate Responses SSE 候选优先与模型降级

## 前提

- 适用质量等级：**critical**。
- 实现授权已因本方案修订而失效；完成本文件和用户的后续明确批准前，禁止编辑产品代码或本机 OMP 配置。
- 所有 Rust 行为改动遵循 RED → GREEN → REFACTOR。测试继续使用现有 service 内联测试结构，不新建未经证实的 `crates/service/tests/gateway/` 测试框架。

## Slice 1：AC-001 的 blocking SSE preflight seam

- **行为**：`GatewayUpstreamResponse::Blocking` 的 `HTTP 200 + text/event-stream + response.failed` 可在交付前进入 preflight，并得到安全的结构化终态事实。
- **代码边界**：`stream_preflight.rs` 仅根据 `response.headers()` 判定 SSE，新增 `TerminalFailure` outcome；`support/upstream_failure.rs` 提供 `error.code` 优先、`error.type` 回退的受限分类键投影，供 HTTP JSON 与 SSE 嵌套 error object 复用。
- **消费者同步**：更新 `aggregate_api.rs` 与 `proxy_pipeline/candidate_executor.rs` 的 exhaustive match；后者将新 outcome 交给现有账号池终态策略，不使用 Aggregate transport budget。
- **RED**：在 `stream_preflight_tests.rs` 增加 blocking Responses SSE `response.failed` fixture，断言 `rate_limit_exceeded`、`authentication_error`、`model_not_found` 与 `invalid_request_error` 的分类键均被保留；再加普通 `server_error` fixture。
- **GREEN**：实现 header-based SSE 判定和前缀终态投影；确认正常 SSE 前缀仍无字节丢失，且没有日志/返回值包含原始 SSE body。
- **验证**：`cargo test -p codexmanager-service stream_preflight --lib` 与 `cargo test -p codexmanager-service upstream_failure --lib`。
- **回滚**：回退本 slice 后只恢复旧 preflight 可达性；不会改变持久化数据。

## Slice 2：AC-001 的立即同模型候选切换

- **行为**：零输出 `rate_limit_exceeded`、`authentication_error` 或 `model_not_found` 从 Terra 候选 A 直接前进至候选 B；客户端只收到 B 的成功结果。`invalid_request_error` 仅返回一次终态。
- **代码边界**：`aggregate_api.rs` 将 preflight 与 bridge 的 `pending_failover_request` 错误归一为一个零交付 failure/action match；`upstream_failure.rs` 扩展并测试显式分类键 allow-list。不得保留两个独立的 SSE action match。
- **测试 seam**：现有 `aggregate_api_tests.rs` 的 `run_upstream_scenario_with_stream()`；该 helper 已复现 Blocking 上游响应，正是回归 seam。
- **RED**：修复已有 `aggregate_sse_rate_limit_fails_over_to_next_candidate` 与 prefix-format rate-limit 测试；添加 auth、model-not-found 和 invalid-request fixtures，分别断言候选数为 2、2、1。
- **GREEN**：`TerminalFailure` 仅填充统一的零交付 failure；共享 `CandidateFailover` action 释放 reservation、保留 request、跳出候选内层循环，且不得创建 bridge 或写客户端终态。
- **验证**：`cargo test -p codexmanager-service aggregate_sse --lib`。
- **回滚**：回退该 action match；HTTP 非 2xx 路径不受影响。

## Slice 3：AC-002 与 AC-005 的一轮同候选重试

- **行为**：普通零输出 SSE `server_error` 或 preflight transport failure 在 A 上总共两次，然后到 B；`invalid_request_error` 不重试也不轮转。
- **代码边界**：`aggregate_api.rs` 以 `aggregate_api_transport_retry_attempts()` 初始化 immutable budget 和 mutable remaining counter，更新最大内层尝试数和 backoff ordinal；共享零交付 action 处理 `RetrySameCandidate` 与 `TransportFailover`。`gateway/core/runtime_config.rs` 的 reader 保持默认值，但补测试其缺失、无效、0 和 1 的语义。
- **RED**：
  - 更新 `aggregate_sse_server_error_retries_same_candidate`：A 两次、B 一次成功，共 3 hits。
  - 添加 transport exhaustion 与 partial-output fixtures；partial output 始终 1 hit。
  - 为 runtime config reader 添加默认 1、0 禁用同候选重试、无效回退 1 的测试。
- **GREEN**：所有重试在 request deadline 和现有 capacity/capability/reasoning budgets 下运行；容量和 capability 分支不降级为普通 transport retry。
- **验证**：`cargo test -p codexmanager-service aggregate_sse --lib` 与 `cargo test -p codexmanager-service runtime_config --lib`。
- **回滚**：设环境变量为 `3` 可临时恢复旧重试次数；完全回退时恢复常量使用。

## Slice 4：AC-003 的语义交付安全

- **行为**：一旦 prefix 已遇到文本、工具调用或其他非元数据语义事件，preflight 返回 `Ready`；后续终态错误只走现有 bridge 终态，不会发送第二个上游请求。
- **代码边界**：`stream_preflight.rs` 与 `stream_preflight_tests.rs`；不放宽 `delivery.rs` 的 `pending_failover_request` 条件。
- **RED**：保留并强化 `aggregate_sse_with_delivered_content_does_not_replay`，添加工具语义前缀（非文本）后 `response.failed` 的单命中断言。
- **GREEN**：终态投影仅在 `PrefixDecision::Deliver` 之前发生；不读取或记录原始 SSE payload。
- **验证**：`cargo test -p codexmanager-service aggregate_sse_with_delivered_content_does_not_replay --lib` 与 `cargo test -p codexmanager-service stream_preflight --lib`。
- **回滚**：不需要数据回滚；此 slice 只保护既有 no-replay invariant。

## Slice 5：AC-004 的网关模型降级

- **行为**：当 Terra 的 Aggregate 候选和账号路径全部耗尽且该模型有 fallback，outer proxy 在同一网关请求中改写为 Sol，并为 Sol 重新解析候选；没有 Sol 路由时返回一次有界终态。
- **代码边界**：保留 `proxy.rs` 的 `fallback_model_slugs` / `RequestReleased` model-hop loop。先在 `aggregate_api_tests.rs` 证明 `fallback_available=true` 时全部 Terra 候选耗尽会归还 request；在 `proxy_tests.rs` 以真实 `proxy_validated_request()` 请求/响应 harness 验证归还 request 后选择 Sol、重建 Sol 候选集并写回一次成功响应。只有该 RED 测试揭示缺口时才改 `proxy.rs`。
- **RED**：构造 Terra fallback list `["gpt-5.6-sol"]`、Terra 的 scripted SSE failures、Sol 的 scripted success；断言上游记录先后只见 Terra、Sol，响应一次成功，且候选不会跨模型复用。
- **GREEN**：若现有 `RequestReleased` 已完成链路，只保留测试；不得新增模型 fallback shim 或第二套候选选择器。
- **验证**：`cargo test -p codexmanager-service proxy --lib` 与相关 Aggregate 测试。
- **回滚**：从模型目录移除 Terra fallback list；源代码不需要回滚，除非测试揭示了 proxy 缺口并实际修改。

## Slice 6：AC-005 与 AC-006 的文档和部署配置

- **行为**：环境变量和 OMP 外层策略可由操作员准确配置，且不触及 secrets。
- **代码边界**：`docs/en/report/environment-and-runtime-config.md` 补充 `CODEXMANAGER_AGGREGATE_TRANSPORT_RETRY_ATTEMPTS`：默认 1、0、无效值、仅 Aggregate 零交付 transport/SSE server-error 路径和 restart/reload 语义。
- **非仓库操作（在所有代码验证通过后）**：
  1. 用 CodexManager 模型目录 UI 设置 Terra fallback model list 为 Sol；不直接编辑 SQLite。
  2. 将 `C:/Users/shuan/.omp/agent/config.yml` 的 `retry.maxRetries` 改为 `1`。
  3. 不修改 `models.yml` 或现有 `fallbackChains`；重启受影响的 CodexManager/OMP 进程。
- **验证**：用无敏感内容的测试请求观察一个 trace 的 candidate IDs、model fallback hop 和最终模型；确认 OMP 没有在单个网关请求尚未结束时切换模型。
- **回滚**：把 OMP 值还原为 3、从模型目录删除 Sol fallback，并重启对应进程。

## Slice 7：critical 质量门禁

1. 运行所有受影响的 focused RED/GREEN tests。
2. 运行 `cargo fmt --check`。
3. 运行 `cargo test -p codexmanager-service`；这是 service 范围内完整回归。
4. 运行 Trellis `trellis-check`；修复发现后重新运行相关 tests。
5. 对稳定快照运行一次独立 `workflow-reviewer`，重点检查：请求 ownership、零交付安全、reservation release/hold、跨模型候选隔离、日志脱敏和回滚。
6. 通过后记录实际执行命令和结果到 task outcome；不把计划中的命令说成已执行。

## 变更文件清单

| 文件 | 预期变更 |
| --- | --- |
| `crates/service/src/gateway/upstream/proxy_pipeline/stream_preflight.rs` | Blocking SSE 识别与结构化终态事实 outcome |
| `crates/service/src/gateway/upstream/support/upstream_failure.rs` | HTTP/SSE 共用分类键投影和候选/请求级 allow-list |
| `crates/service/src/gateway/upstream/protocol/aggregate_api.rs` | 统一的零交付失败 action、动态 transport budget、候选动作 |
| `crates/service/src/gateway/upstream/proxy_pipeline/candidate_executor.rs` | 新 outcome 的穷尽处理，保持账号池既有策略 |
| `crates/service/src/gateway/upstream/proxy_pipeline/stream_preflight_tests.rs` | blocking SSE 和结构化终态测试 |
| `crates/service/src/gateway/upstream/protocol/aggregate_api_tests.rs` | 候选顺序、retry budget、no-replay、RequestReleased 测试 |
| `crates/service/src/gateway/core/tests/runtime_config_tests.rs` | transport budget 配置语义测试 |
| `crates/service/src/gateway/upstream/proxy_tests.rs` | Terra 到 Sol 的真实 proxy model-hop 路由测试 |
| `docs/en/report/environment-and-runtime-config.md` | 已有环境变量文档 |

不新增 migration、RPC、前端 UI、模型 secret 或持久化结构。
