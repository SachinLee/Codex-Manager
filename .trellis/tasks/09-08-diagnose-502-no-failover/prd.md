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

## 2026-09-09 回归验证与 OMP 分层

- 已运行 `cargo test -p codexmanager-service aggregate_sse --lib`：6 个测试中 4 个失败。SSE `response.failed/error` 的 `rate_limit_exceeded`、`server_error`、全部候选失败和前缀错误码场景均只到达一个候选，未满足候选轮转断言。
- 当前候选切换补丁不可达：`aggregate_api.rs` 向 `preflight_stream_response()` 传入 `GatewayUpstreamResponse::Blocking`，但 `stream_preflight.rs` 的 SSE 判定仅接受 `GatewayUpstreamResponse::Stream`；preflight 因而直接返回 Ready。
- 后续 bridge 路径把终态错误同时标为 `terminal_error` 和 `saw_terminal=true`，但零交付归还条件要求 `terminal_error.is_some() && !saw_terminal`。请求不会归还，`aggregate_api.rs` 的 `pending_failover_request` 分类分支不会执行，随后仍以 `succeeded=true` 返回单候选 502。
- 普通 5xx 的同候选预算当前为硬编码常量 3（首次加 3 次重放）；`runtime_config.rs` 已有默认 1 的 `CODEXMANAGER_AGGREGATE_TRANSPORT_RETRY_ATTEMPTS` 读取函数，但没有调用方，环境配置目前不生效。
- `models.yml` 仅定义 CodexManager provider/Responses 协议和模型覆盖；OMP 客户端重试与模型降级实际来自 `C:/Users/shuan/.omp/agent/config.yml`。该配置启用 3 次 OMP 层重试和模型降级；`advisor`、`task` 等角色各有独立降级链，`default` 没有显式链。CodexManager 数据库中 `gpt-5.6-terra` 的 `fallback_model_slugs_json` 为空。

## 推荐策略

1. 先修复 SSE 零语义交付切换的可达性：preflight 必须识别 Blocking SSE，且应把已解析的终态错误交由现有分类器决定“下一候选”或“同候选重试”；绝不在输出文本或工具语义出现后重放。
2. 将每候选的普通 5xx 重试预算接到已存在的运行时配置，默认采用 1 次重试（首次加 1 次），随后继续同模型候选；`rate_limit_exceeded`、鉴权和模型不支持仍立即切候选。
3. 若需要模型降级，把 `gpt-5.6-terra` 的降级链配置在 CodexManager 模型目录中，使其只在该模型的全部候选/账号路径耗尽后执行；OMP 链仅保留跨提供商的最后兜底，并将客户端重试降为 0 或 1 次，避免重复整轮候选扫描。
4. 回归覆盖必须断言：零输出 SSE 限流立即到候选 B；普通 SSE 5xx 在受控同候选预算后到 B；全部候选耗尽后才模型降级；出现输出或工具语义后只返回一次终态、绝不重放。

## 修订实施规划范围

本次只产出并评审实施方案；未经本方案后的明确批准，不修改产品代码、模型目录或本机 OMP 配置。

### 需求

- **R-001**：对尚未交付语义内容的 Aggregate Responses SSE 终态错误，CodexManager 必须先完成同模型候选策略，再把请求交给模型降级。
- **R-002**：普通零交付 `5xx` 每候选最多首次加一次重试；结构化限流、鉴权和模型不支持错误直接进入下一候选；请求级错误立即终止。

结构化终态的分类键优先取 `error.code`，缺失时取 `error.type`。本任务的最小已验证候选级键为 `rate_limit_exceeded`、`authentication_error` 和 `model_not_found`；`invalid_request_error` 仍是请求级终态。不得用开放式错误文本猜测覆盖这些分类。
- **R-003**：`gpt-5.6-terra` 的 CodexManager 模型降级目标为 `gpt-5.6-sol`，只在 Terra 的全部候选和账号路径耗尽后执行。
- **R-004**：OMP 外层最多重试一次；现有跨 Provider 降级链只作为 CodexManager 完整终态后的最终兜底。

### 验收标准

- **AC-001**：`HTTP 200` 的零输出 SSE `rate_limit_exceeded` 请求候选 A 后直接成功切到 B。
- **AC-002**：零输出 SSE 普通 `server_error` 在候选 A 首次加一次重试后，成功继续候选 B。
- **AC-003**：输出文本、工具语义或其他可见语义事件出现后，后续 SSE 失败只返回一次终态，绝不重放到候选或降级模型。
- **AC-004**：Terra 全部候选及账号路径耗尽时，服务在同一网关请求中以 Sol 重新路由；没有配置 Sol 路由时返回有界终态。
- **AC-005**：传输重试预算来自 `CODEXMANAGER_AGGREGATE_TRANSPORT_RETRY_ATTEMPTS`，默认值为 1，文档说明 0、1 和无效值的语义。
- **AC-006**：OMP `config.yml` 的 `retry.maxRetries` 为 1；不修改 `models.yml` 的 provider/secret 定义，也不删除现有跨 Provider fallback chain。

### 明确不在范围

- 不改变 Aggregate API 的健康阈值、冷却时长、计费/日限额公式或数据库模式。
- 不新增 RPC、Tauri 命令、前端表单或环境变量。
- 不把已交付流的错误伪装为可重放失败。
