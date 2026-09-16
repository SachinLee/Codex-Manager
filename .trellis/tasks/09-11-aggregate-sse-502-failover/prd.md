# Aggregate SSE 502 候选切换修复（P0-P3）

## Goal

用户（客户端）在 Terra 上拿到 502 `server_error` / `gateway_concurrency_limit` / `rate_limit_exceeded` 时，
说明网关把候选 A 的失败直接吐给了客户端，而没有切换到候选 B/C/D。

本任务让这三类**零交付**上游错误必须推进到下一候选；同时补齐逐候选失败观测，
使运维能从请求日志/追踪日志看出「每个候选实际返回了什么、为什么被跳过」。

用户价值：Terra 请求不再因为单一聚合商过载/限流而成片 502，改为在候选池内自愈；
失败仍有终态，但只在候选真正耗尽时出现。

## Background / Confirmed Facts

证据来源：本机 `C:\Users\shuan\AppData\Roaming\com.codexmanager.desktop\codexmanager.db`
（`request_logs`，7 天窗口）与同目录 `gateway-trace.log`。代码锚点为当前工作区。

### 事实 1：这三类错误几乎从不在候选间切换

| error code（`request_logs.error`） | 7 天次数 | 发生候选切换（chain>1） | chain=1 |
| --- | --- | --- | --- |
| `server_error`（含 `type=service_unavailable_error`） | 61 | 5 | 56 |
| `rate_limit_exceeded` | 13 | 0 | 13 |
| `gateway_concurrency_limit` | 7 | 0 | 7 |

### 事实 2：这些请求全部是「零交付」失败

trace 样本（id 80105 / 79208 / 78401 等 81 条）字段完全同构：

```
adapter=Passthrough stream=true upstream_content_type=text/event-stream
last_sse_event=response.failed output_text_len=0 output_tokens=- delivered_status=-
→ FAILED_REQUEST status=502  upstream_url=https://esfaery.com/v1/responses
```

即：客户端一个字节都没收到，也没有 billable output。failover 在物理上完全可行。

### 事实 3：候选池并不缺候选——同期同类错误确实切换成功过

同一分钟的相邻请求（id 76222–76290，全部命中同一个 `upstream_url=https://esfaery.com`）：

- `76230/76232/76236/76237/76240/76242/76253/76257/76269/76272–76276` → 502 `rate_limit_exceeded`，chain=1
- `76222–76228/76235/76251/76265/76270/76286` → **200，chain=3**
- `76241/76243/76245/76246/76260/76266` → **200，chain=2**

7 天聚合链路长度分布：成功 chain1=1610 / chain2=32 / chain3=123 / chain4=9；
失败 chain1=164 / chain2=12 / chain3=10 / chain4=2。
说明轮转机制本身可用，问题在特定失败形态上没有进入轮转。

### 事实 4：既有 SSE 候选切换用例全绿（决定性反证）

命令与结果（本工作区实跑，编译 3m45s + 测试 2.23s）：

```
cargo test -p codexmanager-service --lib aggregate_sse
running 7 tests
aggregate_sse_invalid_request_terminates_immediately ... ok
aggregate_sse_with_delivered_content_does_not_replay ... ok
aggregate_sse_rate_limit_fails_over_to_next_candidate ... ok
aggregate_sse_prefix_format_rate_limit_fails_over ... ok
aggregate_sse_server_error_retries_same_candidate_once ... ok
aggregate_sse_all_candidates_fail_returns_terminal_error ... ok
aggregate_sse_capacity_error_retries_same_candidate_independently ... ignored
test result: ok. 6 passed; 0 failed; 1 ignored
```

即：**当 SSE 终态错误是首个「可判定帧」时，preflight seam 完全可用**
（`rate_limit_exceeded` 命中 2 次上游、`server_error` 命中 3 次、已交付内容仅命中 1 次）。
因此 `aggregate_api.rs:3410-3471` 的候选决策块并非死代码。
问题在于**生产环境里 error 帧从来没能成为首个可判定帧**。

### 事实 5：根因 R1——preflight 在 error 帧到达前就已提交（决定性）

`preflight_stream_response_with_timeouts`（`stream_preflight.rs:365-483`）
只在本函数能把 prefix 判定为错误时才 failover；以下两条路径都会**提前提交**，
把响应交给 bridge，从此再无候选切换机会：

- **R1a｜wall-clock 静默超时**：上游 10s 内一字节不发 →
  `GatewayStreamPrefetchTerminal::WallClockTimeout` → `StreamPreflightOutcome::Ready(response)`（`:457`）。
  提交后 bridge 才开始读，因此 `first_response_ms` ≈ 上游真实首字节时间（可 ≥10s）。
- **R1b｜非元数据事件**：`classify_prefix` 对任何非 metadata 事件直接
  `Some(_) | None => PrefixDecision::Deliver`（`:331`）。metadata 白名单
  （`:182-193`）只含 `response.created/in_progress/queued/output_item.added/
  content_part.added/reasoning_summary_part.added/ping`；
  **`response.reasoning_summary_text.delta`、`response.reasoning_text.delta`、
  `response.content_part.delta`、`response.output_item.done` 等都不在名单内** → 立即提交。

两条路径的后果相同：`/v1/responses` 走
`respond_passthrough_collector_stream_strict_guard`（`delivery.rs:337-458`），
该函数把**整个 body 读进内存后才写**（`delivery.rs:348-370`），
但它的 failover 门槛是 `read_error.is_some() && … && !collector.saw_terminal`（`delivery.rs:409-414`）。
干净收尾的 SSE 终态错误没有 `read_error`，且 `response.failed` 必然把 `saw_terminal`
置 true（`stream_readers/openai_responses.rs:183-190`）→ 门槛恒 false →
`pending_failover_request = None` → 落到 `aggregate_api.rs:3797` 的 `respond_error(502)`。

证据吻合度（事实 1 的 81 条）：52 条 `first_response_ms >= 10s`（R1a），
24 条 `< 10s`（R1b，首字节很快但先到的帧不是 error）。

> 备注：`.trellis/tasks/09-08-diagnose-502-no-failover/design.md` 曾以
> 「`saw_terminal` 不是零交付的可靠证明」为由否决放宽该门槛。该论证对
> strict-guard 路径**不成立**：那条路径在任何字节写出之前就已完成全量缓冲
> （`delivery.rs:348-370`），缓冲区完整、`delivered_status_code = None`、
> 客户端零字节——这是比 `saw_terminal` 强得多的零交付证明。
> 该约束只对**实时直吐**的 `respond_passthrough_collector_stream`（`delivery.rs:460-525`）成立，
> 那条路径确实不可回退（且同样因 `terminal_error`/`saw_terminal` 同时置位而恒 false）。

### 事实 6：根因 R2——失败分类表缺项且取向相反

`upstream/proxy_pipeline/stream_preflight.rs:265-288` 与
`upstream/support/upstream_failure.rs:130-145`：

- `gateway_concurrency_limit` 不在任何 allow-list → 落到 status 分支，
  HTTP 502 → `RetrySameCandidate`，白白消耗 transport 预算（默认 1 次，`runtime_config.rs:3558`）后 502。
- `server_error` / `upstream_error` 被显式归入终态并在分类器里落到 `RetrySameCandidate`
  （09-08 的刻意决定，测试 `aggregate_sse_server_error_retries_same_candidate_once` 固定了它）。
- `rate_limit_exceeded` **已**在 allow-list 内（→ `CandidateFailover`），
  但由于事实 4 的 R1，这条规则在 SSE 终态路径上不可达。

### 事实 7：根因 R3——逐候选观测缺失

- 聚合路径**从不**写 `CANDIDATE_POOL` / `CANDIDATE_START` / `ATTEMPT_RESULT`
  （`gateway-trace.log` 中各 747 条全部带 `account_id`，属账号池路径）。
- `AggregateApiAttemptRecord.failure_category`（`aggregate_api.rs:1773-1779`，
  由 `classify_aggregate_api_failure` `:1056` 产出）在 `proxy.rs:1137/1262/1520`
  的 `Responded { .. }` 处被丢弃。
- 决策日志 `event=aggregate_responses_sse_preflight_action` 是 `log::info!`，
  而本机 `CodexManager.log` 仅收 WARN 级。
- `request_logs` 只有 `initial_aggregate_api_id` / `attempted_aggregate_api_ids_json`
  与最终 error，没有逐跳 status/code。

### 事实 8：并发/过载类错误当前的冷却语义

- 内存冷却：连续 5 次失败 → 冷却 5 分钟
  （`routing/aggregate_api_cooldown.rs`：`AGGREGATE_API_FAILURE_THRESHOLD=5`、
  `AGGREGATE_API_COOLDOWN_SECS=5*60`）。
- 持久健康：`FAILURE_THRESHOLD=5`、`TRANSIENT_COOLDOWN_SECS=5*60`
  （`aggregate_api_health.rs:18-19`），但本机 `aggregate_api_health_configs`
  两条记录均 `enabled=0`，实际拦截者是内存冷却。
- 已观测到冷却耗尽候选导致 503：`request_logs.id=80041`
  `error=all aggregate apis are cooling down`。
- 本机 `gpt-5.6-terra` 可用（route enabled 且 API active）候选：4 个
  （sort 205 esfaery / 250 wegoo-codex / 400 timcc / 416 wegoo-codex-0.2）。
  这是「首击即冷却」风险的关键约束。

### 事实 9：既有消费者与测试 seam

- `classify_upstream_failure` 的消费者：`aggregate_api.rs`（非 2xx 分支 `:2848`、
  preflight 分支 `:2995`、SSE 终态分支 `:3418`）与 `proxy_pipeline/stream_preflight.rs`。
  账号池 `candidate_executor.rs:565-660` 消费 preflight outcome，但**不**使用
  `classify_upstream_failure`，其策略独立（`should_failover_terminal_gateway_error`）。
- 回归 seam：`aggregate_api_tests.rs` 的 `run_upstream_scenario_with_stream()`
  （`:1223`），已能复现 blocking 上游响应；现有相关用例
  `aggregate_sse_rate_limit_fails_over_to_next_candidate`（`:942`）、
  `aggregate_sse_server_error_retries_same_candidate_once`（`:1000`）、
  `aggregate_sse_with_delivered_content_does_not_replay`（`:1107`）。
- `stream_preflight_tests.rs` 覆盖前缀分类。

## Requirements

- R-001：`/v1/responses` 等走 strict-guard 的流式路径上，当**上游返回 SSE 终态错误**
  且**尚未向客户端交付任何字节/语义内容**时，网关必须能够把请求归还给候选循环，
  而不是直接 502。
- R-002：`server_error`、`service_unavailable_error`、`gateway_concurrency_limit`
  必须与 `rate_limit_exceeded` 一样进入候选级 failover（`CandidateFailover`）。
- R-003：`rate_limit_exceeded` 在 SSE 终态路径上的既有 failover 意图必须真正生效（由 R-001 解锁）。
- R-004：一旦已交付语义内容（输出文本/工具调用/`output_tokens>0`），
  必须保持既有 no-replay 不变量：不得重放请求、不得切候选。
- R-005：`invalid_request_error` / `invalid_request` 及 400/422/413 的请求级终态语义保持不变。
- R-006：容量错误（`selected model is at capacity…`）与能力降级、reasoning-guard
  的独立预算与既有优先级不变。
- R-007：这三类错误命中候选后，必须按现有冷却机制计入候选健康状态，避免持续把请求打到已过载的上游；本任务不新增冷却参数，沿用现有连续 5 次失败 → 5 分钟冷却策略（用户已确认）。
- R-008：聚合路径必须产生逐候选失败观测，至少包含：trace_id、候选 id、候选位次、
  upstream_url、HTTP 状态、错误 code、决策（failover / retry_same_candidate / terminal）、
  是否零交付、重试序号。禁止记录 prompt/body、原始 SSE 帧、Authorization、tool 参数、secret。
- R-009：`AggregateApiAttemptRecord`（含 `failure_category`）必须落库可查，
  不能被 `Responded { .. }` 丢弃。
- R-010：本任务不得改变账号池（account_pool）路径的候选策略与重试预算。

## Acceptance Criteria

- AC-001（R-001, R-002）：新增集成测试，候选 A 返回 200 + SSE
  `data: {"type":"response.failed","error":{"code":"gateway_concurrency_limit",…}}`，
  候选 B 返回成功；断言上游被访问 2 次且客户端收到 B 的成功响应。
  当前实现下该测试 RED。
- AC-002（R-001, R-002）：同上 fixture 换成 `server_error`（含
  `type=service_unavailable_error` 变体）与 `rate_limit_exceeded`，断言均切换到 B。
- AC-003（R-004）：候选 A 先产出 `response.output_text.delta` 再 `response.failed`；
  断言上游仅被访问 1 次，且客户端收到已交付内容 + 错误终态（不重放）。
- AC-004（R-003）：`aggregate_sse_rate_limit_fails_over_to_next_candidate`
  等既有用例仍通过；不再存在「分类正确但分支不可达」的死代码路径。
- AC-005（R-005, R-006）：`aggregate_sse_invalid_request_terminates_immediately`、
  容量错误与 capability 相关既有用例保持通过。
- AC-006（R-007）：冷却行为有可观测断言（新增单元测试），且不会因单次瞬时错误
  把全部 Terra 候选同时冷却成 503。
- AC-007（R-008）：复现 AC-001 场景时，日志中出现一条含
  `trace_id`/`candidate_id`/`position`/`status`/`error_code`/`decision` 的逐候选事件；
  且该行不含 body、SSE 原文或凭据。
- AC-008（R-009）：一次多候选请求结束后，持久化记录能查询到每个候选的
  `failure_category` 与 outcome。
- AC-009（R-010）：账号池路径测试与既有重试预算测试全绿。

## Out of Scope

- 非 `/v1/responses`（非 strict-guard，实时直吐）路径的候选切换——
  没有全量缓冲即无法回退，属既有架构限制。
- `apply_gateway_route_strategy_to_aggregate_candidates`（balanced 轮转）在生产路径无调用者的既存问题。
- 前端日志页面改版（除 R-009 落库字段外）。
- OMP 侧重试/模型降级配置。
- 上游聚合商自身的容量治理。

## Technical Notes

- R-007 决策已关闭：不改冷却阈值与时长，不新增过载类专用冷却参数。新的候选级失败必须继续走现有 `cooldown_eligible_failure` → `gateway_record_aggregate_api_failure` 链路；单次请求每候选最多贡献一次失败计数，因此不会因单次错误直接触发全部候选冷却。
