# 设计：Aggregate SSE 502 零交付候选切换与逐候选观测

## 状态与质量等级

状态：planning；质量等级：critical。设计覆盖 PRD 的 R-001 至 R-010 与 AC-001 至 AC-009。本文只冻结实现边界，不启动任务、不修改生产代码。

## 问题与已确认边界

`/v1/responses` 的 Aggregate 路径会先执行 SSE preflight。preflight 在 wall-clock timeout 或首个非 metadata 事件出现时返回 `Ready`，随后 `respond_passthrough_collector_stream_strict_guard` 将整个 body 缓冲后才写客户端。当前 bridge 门槛同时要求 `read_error.is_some()` 与 `!collector.saw_terminal`，因此干净的 `response.failed` 终态会落到客户端 502，无法把 request 归还 Aggregate 候选循环。

目标错误：`server_error`、`service_unavailable_error`、`gateway_concurrency_limit`、`rate_limit_exceeded`。其中 `rate_limit_exceeded` 的分类意图已存在，但在上述后置 SSE 形态下不可达；`server_error` 当前仍是同候选重试。

不改实时直吐路径，不改账号池候选策略，不改 OMP 配置，不新增冷却参数。R-007 已确认沿用现有 `(api_id, upstream_model)` 连续 5 次失败、冷却 5 分钟。

## 设计决策

### D1：仅在 Aggregate opt-in 的 strict-guard body seam 允许零交付回退

不修改 `stream_preflight.rs` 的提交时机。10 秒 wall-clock 与首个非 metadata 事件都是既有延迟/语义边界；在 bridge 之后，strict-guard 已完成全量读取且尚未调用 `respond_streaming_chunked`，因此在门槛判断点客户端尚未收到任何字节。

新增独立的 `allow_zero_delivery_failover: bool` 参数，沿 `http_bridge/mod.rs` dispatcher、`delivery.rs` blocking/stream bridge 传递到 strict-guard：

- Aggregate `aggregate_api.rs` 传 `true`；
- account pool `response_finalize.rs` 传 `false`；
- 不复用已有 `allow_failover_for_deactivation`，避免两个不同语义共用一个布尔值。

strict-guard 新门槛分两部分：

1. 保留旧的 read-error 零交付分支语义；
2. 增加 `read_error.is_none()` 且 `collector.terminal_error.is_some()` 的完整 SSE 终态错误分支。

两部分均要求：Aggregate opt-in、运行时 `CODEXMANAGER_AGGREGATE_ZERO_DELIVERY_FAILOVER` 开启、HTTP status 为 200、没有已观察的语义输出证据。语义输出证据复用 collector 已有 usage：非空 `usage.output_text` 或 `output_tokens > 0` 时不回退；这保持 AC-003 的 output-text-then-failed no-replay。若实现需要补齐 tool-call 证据，必须在 collector 中增加单一 `saw_semantic_content` 标志，由 Responses reader 的事件模型设置，不能在 bridge 里再维护第二套 SSE 解析规则。

允许回退时返回 `pending_failover_request = Some(request)`，不调用 `respond_streaming_chunked`；`delivery_error` 保持现有 read-error 值。干净终态错误时即使 `delivery_error` 为 `None`，`stream_terminal_error` 仍使 `bridge.is_ok(true)` 为 false，Aggregate 可进入既有 SSE 终态决策块。

非 strict-guard 的 `respond_passthrough_collector_stream` 不修改；该路径实时直吐，无法证明客户端零交付，符合 Out of Scope。

### D2：集中扩展失败分类，不复制候选策略

在 `upstream/support/upstream_failure.rs` 的 `classify_upstream_failure` 中，将以下 code 归入 `CandidateFailover`：

- `server_error`；
- `service_unavailable_error`；
- `gateway_concurrency_limit`；
- 已有的 `rate_limit_exceeded` 保持不变。

容量匹配必须继续先于 code 分类，能力重试继续先于 code 分类；因此 selected-model capacity 不会被误分类为普通 failover。`invalid_request_error`、`invalid_request` 仍为 `RequestTerminal`，400/422/413 仍为请求级终态。账号池不调用该 classifier，保持 R-010。

`extract_error_code_from_terminal` 与 JSON 路径保持同一优先级：先识别 `code=`，缺失时识别首个 `type=`。只提取 bounded code，不记录完整 body。未知 code 继续使用既有同候选 transport retry 语义。

不把 `upstream_error` 或其他未由本任务证据确认的 code 无条件改为 failover；除非实现阶段发现现有测试/契约要求同步变更并记录于审查结果。

### D3：复用 Aggregate 既有决策块与 reservation 生命周期

`aggregate_api.rs` 的 preflight `TerminalFailure` 与 bridge `pending_failover_request` 继续汇入现有 `classify_upstream_failure` 决策，不增加第二个候选循环。

- `CandidateFailover`：取回 request，释放本次未交付 spend reservation，标记 `cooldown_eligible_failure = true`，结束当前候选内层循环，外层尝试下一候选；
- `RetrySameCandidate`：只消耗既有 transport retry budget，保持既有退避与 reservation 生命周期；
- `RequestTerminal`：取回 pending request，释放 reservation，设置 terminal 状态，最终通过既有 `respond_error` 返回一次 502，不能让 request 因 ownership 丢失而触发 `aggregate api request already consumed...`；
- Capacity/Capability/Reasoning Guard：保持现有优先级、独立预算与 health-neutral 语义。

SSE 终态 `RequestTerminal` arm 必须显式 `take()` pending request；这是放宽 bridge 门槛后会被激活的 ownership 修复。`cooldown_eligible_failure` 继续调用 `gateway_record_aggregate_api_failure`，不新增冷却配置。单次请求对每个候选最多计一次失败，现有五次阈值避免首击直接冷却全部候选。

### D4：逐候选 trace 观测使用现有 trace-log convention

在 `observability/trace_log.rs` 增加 Aggregate 专用结构化事件 helper，格式至少包含：

`trace_id`、`candidate_id`、`position`、sanitized `upstream_url`、HTTP `status`、`error_code`、`decision`、`zero_delivery`、`retry_ordinal`。

事件写入现有 trace buffer，使用与 `CANDIDATE_START`/`ATTEMPT_RESULT` 相同的 `sanitize_text` 和 URL 脱敏；不写 prompt、request body、SSE 原文、Authorization、tool arguments、secret。决策点必须覆盖 Aggregate 的 preflight、非 2xx 和 post-bridge SSE 终态；同候选 retry 的每次决策都记录 retry ordinal。事件名固定为 `AGGREGATE_CANDIDATE_DECISION`，decision 使用 `candidate_failover`、`retry_same_candidate`、`terminal`，其他内部恢复动作使用明确的既有 action 名。

为 AC-007 提供纯格式化函数测试；若 trace buffer 没有可观察测试接口，增加仅 `#[cfg(test)]` 的 pending-line 读取接口，不改变生产 API。恢复成功的请求也必须保留候选失败事件，不能只依赖最终 502 才 flush。

### D5：用既有 migration 140 补齐 attempt records 落库

`crates/core/migrations/140_request_logs_aggregate_api_attempts.sql` 已存在但未注册、未接入 `RequestLog`。沿既有 request-log column migration convention：

- 在 `Storage::init` 注册 `140_request_logs_aggregate_api_attempts`，并提供 `ensure_request_log_aggregate_api_attempts_column` 兼容路径；
- `RequestLog` 增加 `aggregate_api_attempts: Option<String>`；
- request-log list select、row mapper、普通 INSERT、带 token stat INSERT、legacy compaction schema/INSERT 全部同步列；
- `RequestLogTraceContext` 增加预序列化 attempts JSON 字段；Aggregate 使用 serde 将 `AggregateApiAttemptRecord` 序列化为数组，`AttemptOutcome` 使用 snake_case；
- 成功桥接路径在写 request log 前先将当前候选的最终 `Success`/`Failure` record 纳入数组；失败终态路径直接使用已完成的数组；
- `RequestLog` API/查询保留 additive 字段，前端日志页面不新增展示逻辑。

数组元素至少包含 `api_id`、1-based `position`、`outcome`、`failure_category`。不改已有 `attempted_aggregate_api_ids_json`，保持旧消费者兼容。

### D6：冷却策略保持既有实现

不新增环境变量、设置项或 RPC。新增/修正的候选失败只保证继续走既有 `cooldown_eligible_failure` → `gateway_record_aggregate_api_failure` 链路；阈值仍为 5，时长仍为 300 秒。新增边界测试验证第 1 次不进入冷却、第 5 次进入冷却，并验证单次 failover sweep 不会把 4 个 Terra 候选直接置为 cooling down。

## 数据流与状态转换

```text
A: HTTP 200 + SSE response.failed
  -> preflight Ready (wall-clock 或首个非 metadata)
  -> strict-guard 全量缓冲，尚未写客户端
  -> zero-delivery gate (Aggregate opt-in)
  -> pending_failover_request
  -> extract code/type
  -> classify CandidateFailover
  -> release spend + record health failure
  -> B

A: output_text.delta ... response.failed
  -> strict-guard observes output_text
  -> gate denied
  -> flush buffered SSE to client
  -> one upstream access, no replay

A: invalid_request_error
  -> RequestTerminal
  -> restore request ownership
  -> release reservation
  -> one final 502, no B
```

成功候选的最终 attempt record 必须在 request-log INSERT 前进入 JSON；所有失败候选记录按实际 candidate loop 顺序保留。`RequestReleased`（Aggregate 后回落 account pool）不提前写最终 request outcome，沿用现有防重复结算语义。

## 兼容性与风险

- account pool 的 bridge opt-in 固定为 false；其 `response_finalize.rs`、`candidate_executor.rs` 策略和 retry budget 不变。
- capacity 文案优先级高于新 code 分类；capacity retry 不会被新 failover 规则吞掉。
- `server_error` 从同候选 retry 改为直接 candidate failover，会减少一次同候选重试；这是 R-002 的明确行为变化，原 `aggregate_sse_server_error_retries_same_candidate_once` 必须改为新契约测试。
- 每次候选失败仍会计入现有冷却计数；保留五次/五分钟意味着全候选持续过载时仍可能出现 `all aggregate apis are cooling down`，这是已确认的健康策略而非本任务新增行为。
- 16 MiB strict-guard buffer 限制、deadline、daily spend、affinity clear 和 model fallback 不扩展。

## 验证与回滚

验证重点：

1. R1b 快速 fixture：先发 reasoning-only/non-output event，再发 `response.failed`，证明 preflight 已提交但 bridge 仍能 failover；R1a wall-clock 由同一 gate 逻辑覆盖，避免默认测试引入 10 秒等待。
2. output-text-then-failed fixture：证明 gate denied、body flush、无重复上游访问。
3. classifier 单测：新 code、type fallback、capacity/invalid request 优先级。
4. cooldown boundary 单测与 Aggregate integration assertion。
5. request log schema migration、读写和多候选 JSON 查询。
6. `cargo test -p codexmanager-service --lib aggregate_sse`、相关 core storage tests、账号池 targeted tests；最终按仓库规则执行 Rust workspace 相关验证。

回滚：先将 `CODEXMANAGER_AGGREGATE_ZERO_DELIVERY_FAILOVER=false` 作为运行时止损开关；代码回滚只涉及 bridge opt-in/gate、classifier、Aggregate observability/persistence 和 migration wiring。已应用的 additive column 保留，不执行破坏性降级迁移。

## 未决技术风险

- strict-guard 对完整 body 的内存上限仍为既有 16 MiB；超限按既有 read-error 处理，不扩大 buffer。
- trace buffer 对恢复成功请求的 flush 行为必须在实现阶段用测试 seam 证实；如果现有 flush 策略只保留 error trace，必须为 Aggregate candidate decision 增加最小的保留条件，而不能退回到仅 `log::info!`。
