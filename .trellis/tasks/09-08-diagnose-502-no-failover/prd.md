# 诊断 `gpt-5.6-terra/xhigh` 的 502 未切换

## 目标与范围

确认生产请求为何未从 `esfaery` 切换到下一 Aggregate API，并区分：

- 已向上游发送过请求但候选失败；
- 候选耗尽后应返回给客户端的有界终态；
- 路由阶段没有选中可发送候选时，请求日志“上游”为空。

本任务只诊断，不修改产品代码。

## 已确认的生产证据

### 三个截图对应请求走的是 Aggregate API，不是账号池

`request_logs` 的 `account_id` 为空、`actual_source_kind=aggregate_api`、`route_strategy=ordered`、`route_source=route_strategy`。三个请求均由 `ag_6e0a03cb1…`（供应商 `esfaery`）实际执行：

| Trace ID | 终态记录时间 | 终态错误 | 时长 | Aggregate 候选尝试数 |
| --- | ---: | --- | ---: | ---: |
| `trc_1788840719606_fc` | `1788840816` | `code=server_error …` | 96,725 ms | 1 |
| `trc_1788840928165_fe` | `1788841058` | `code=upstream_error …` | 130,706 ms | 1 |
| `trc_1788840947586_ff` | `1788841174` | `code=rate_limit_exceeded …` | 226,903 ms | 1 |

每条对应一条 `gateway_upstream_attempt_events` 记录：`attempt_index=0`、`outcome=failed`、`delivery_started=0`、`error_class=upstream`、`error_code=NULL`。三条请求都没有第二候选或同候选重放记录。

模型路由当前配置了 8 个 Aggregate API route；其中 4 个 Aggregate API 状态为 active：`esfaery`、`wegoo-codex`、`timcc-0.2`、`wegoo-codex-0.2`。因此“当前仅配置 esfaery”不是解释。历史记录不足以重建当时其余三个候选的冷却快照；但下述代码会在首个 SSE 终态错误后无条件返回，不论后续候选当时是否可用。

### 实际错误不是外层 HTTP 502 JSON 分支

`gateway-trace.log` 显示三条请求均收到 `text/event-stream`，随后出现 `response.failed` 或 `error` SSE 终态事件：

- `rate_limit_exceeded` 请求：`last_sse_event=error`，`terminal_seen=true`；
- `upstream_error` 请求：`last_sse_event=response.failed`，`terminal_seen=true`；
- `server_error` 请求：`last_sse_event=response.failed`，`terminal_seen=true`。

这些请求必须先经过 `aggregate_api.rs` 的成功 HTTP 响应 bridge；若上游 HTTP 状态本身为非成功，代码会在 `aggregate_api.rs:2657+` 的非成功响应分支处理，而不会进入 SSE bridge。请求日志中的 502 是 bridge 将 SSE 终态错误归一化后的客户端终态，不是可由 `error_code_from_response_body()` 读取的原始 JSON 502 body。

`gateway_upstream_attempt_events.error_code` 对三条均为 `NULL`，与错误码只从 SSE 错误消息中取得这一事实一致。

## 根因

### 1. 原有 502 限流 failover 已实现，但只覆盖非成功 HTTP JSON 响应

`crates/service/src/gateway/upstream/support/upstream_failure.rs` 将：

- `401/403/404/405/429/501`；或
- 标准响应 JSON 中 `error.code == "rate_limit_exceeded"`

归为 `CandidateFailover`。`aggregate_api.rs:2828-2869` 在“尚未创建 bridge”的非成功 HTTP 分支使用这一决策；`aggregate_api_tests.rs:880-900` 已验证 `HTTP 502 + {"error":{"code":"rate_limit_exceeded"}}` 会发送两次请求并切到下一候选。

截图请求不在这个分支，因此“把所有 502 改成候选 failover”不能修复它们。

### 2. Aggregate Responses SSE 终态错误绕过分类并过早结束候选循环

在 `crates/service/src/gateway/upstream/protocol/aggregate_api.rs`：

1. `respond_with_upstream()` 在 `2982-2998` 消费请求并把 SSE bridge 结果返回。
2. `3138-3145` 从 `upstream_error_hint` 或 `stream_terminal_error` 形成 `final_error`；实际 trace 的错误文本正来自此处。
3. 即使 `final_error` 存在，`3357-3358` 仍设置 `succeeded=true` 并跳出内层循环。
4. `3373-3422` 把该候选标记为失败后，仍立即返回 `AggregateAttemptOutcome::Responded`，没有回到外层候选循环。

因此三个请求每个只调用了 `esfaery` 一次，原因不是原始 502 被错误归类为同候选重试，而是 SSE `response.failed/error` 路径根本没有调用 `classify_upstream_failure()` 或候选 failover 逻辑。

### 3. 该 SSE 终态发生时，请求已经不能安全重放

`UpstreamResponseBridgeResult::is_ok()`（`observability/http_bridge/aggregate/output_text.rs:68-80`）把 `stream_terminal_error` 视为失败。其 `pending_failover_request` 只在请求尚未交付、能够归还时存在。

目前 Aggregate bridge 的零交付保护（`observability/http_bridge/delivery.rs:409-448` 与 `469-515`）仅在：

- 有读错误/terminal error；
- HTTP 200；
- `!collector.saw_terminal`；
- 输出 token 为 0

时归还请求。实际三条 trace 都有 `terminal_seen=true`，所以 bridge 已将该 SSE 终态交付/消费，`pending_failover_request` 为空。此时再切候选可能把已经可见的 SSE 错误、输出或工具调用与第二候选结果混合，不能用“返回后再重试”修复。

现有账号池路径已在 `proxy_pipeline/stream_preflight.rs` 于交付前检查 SSE 前缀；Aggregate Responses 路径只有 Chat 专用 preflight，未复用这一条可重放的前置判定。

## 关于“全部候选不可用”

候选轮转是有界的，不应无限发送上游请求：

- 外层仅遍历一次有限的 `planned_candidates`（`aggregate_api.rs:2173+`）；
- 每个候选的同候选、容量、能力和 reasoning guard 重试预算在 `2317-2328` 初始化，且受 request deadline 限制；
- 对可重放的候选级失败，外层候选耗尽后 `3448-3534` 只向客户端写入一次最终错误；
- 请求可走模型降级链时，`proxy.rs:21-34` 的模型 hop 上限为 3，仍是有界的。

正确策略是：仅当还未向客户端交付语义内容、请求仍可归还，才执行候选 failover；所有候选和允许的降级路径耗尽后，停止上游流量并返回当前已有终态。不能把已交付流式响应再发送给下一候选。

## “上游为空”实际含义

已在生产数据库中定位到真实记录 `trc_1788835156983_4c`：

- 模型 `qwen3.8-flash`；
- 状态 `503`；
- 错误 `all aggregate apis are cooling down`；
- `upstream_url=NULL`；
- `attempted_aggregate_api_ids_json` 长度为 1；
- `gateway_upstream_attempt_events` 数量为 0。

这不是 CodexManager 又发起了一次“没有上游”的请求。`aggregate_api.rs:1924-2011` 在调用上游前过滤处于 cooldown 的候选；全部被过滤后写入 `503 all aggregate apis are cooling down`，并将被跳过的候选 ID 写进 `attempted_aggregate_api_ids_json`，但不会设置 `upstream_url`。所以 UI 的“上游”为空准确表示：本次请求没有实际 upstream dispatch，而不是一次缺失 URL 的上游调用。

当前三条 `gpt-5.6-terra/xhigh` 记录没有 `upstream_url=NULL` 情况；它们各自实际调用了 `esfaery`。UI 仅显示“上游”时无法区分“无候选”“全冷却”“零余额”“模型无路由”等路由终态，造成误解。

## 结论与后续修复边界

1. 截图问题根因是 Aggregate Responses 的 SSE 终态失败路径，没有在客户端交付前执行可重放的错误分类/候选切换。
2. 不应泛化为“所有 502 一律 failover”。普通 5xx、传输错误和已交付流的终态仍需保留既有有界行为。
3. 修复任务应在 Aggregate Responses bridge 前增加有限 SSE 前置判定：仅对零语义交付的 `response.failed/error` 提取结构化 `error.code`，按照现有 `classify_upstream_failure()` 决定同候选重试、候选 failover 或终态；一旦交付则不重放。
4. 修复必须覆盖：
   - `HTTP 200 SSE response.failed`，`error.code=rate_limit_exceeded`：切到候选 B；
   - `HTTP 200 SSE response.failed`，普通 `server_error/upstream_error`：锁定同候选预算与候选耗尽终态；
   - 候选均失败：有限 dispatch 后一次终态，不再尝试上游；
   - 全冷却/无候选：零 upstream dispatch，日志显式呈现路由拒绝原因而非只显示空“上游”。

## 验证证据

- 生产 SQLite：`C:/Users/shuan/AppData/Roaming/com.codexmanager.desktop/codexmanager.db` 的 `request_logs`、`gateway_upstream_attempt_events`、`aggregate_apis`、`model_routes`、`aggregate_api_health_events`。
- 生产 trace：`C:/Users/shuan/AppData/Roaming/com.codexmanager.desktop/gateway-trace.log`。
- 已运行的数据库查询证明三条截图请求各只有一次 Aggregate attempt；另一个全冷却请求有零 upstream attempt events。
- 未修改产品代码，未运行测试套件。
