# 设计：Aggregate Responses SSE 候选优先与模型降级

## 状态与质量等级

本文件取代此前依赖 bridge 后 `pending_failover_request` 的方案。该方案已由 `cargo test -p codexmanager-service aggregate_sse --lib` 证伪：4 个 SSE 候选切换断言只发出一次上游请求。

**质量等级：critical。** 改动面向公开网关路由，重试会影响上游调用次数、每日预算预留和最终模型计费。实现须满足 `R-001` 至 `R-004` 与 `AC-001` 至 `AC-006`，并走 critical 的验证与独立审查门禁。

本设计只规定实现；不授权修改产品代码、本地模型目录或 OMP 配置。

## 已确认的当前链路

```text
OMP role/model
  -> POST CodexManager /v1/responses (gpt-5.6-terra)
  -> Aggregate candidate loop
  -> HTTP 200 + text/event-stream
  -> Responses SSE preflight
  -> bridge / client delivery
  -> all source routes exhausted: proxy model-fallback loop
  -> final HTTP response to OMP
  -> OMP outer retry / cross-provider fallback
```

当前 Aggregate 分支把 blocking `reqwest::blocking::Response` 包成 `GatewayUpstreamResponse::Blocking` 后传入 `preflight_stream_response()` (`aggregate_api.rs:2963-3000`)。该 preflight 仅把 `GatewayUpstreamResponse::Stream` 视为 SSE (`stream_preflight.rs:207-215`)，因而不会读取该响应的 SSE 前缀。

随后 bridge 读取 `response.failed` 时会同时设置 `terminal_error` 和 `saw_terminal=true`。bridge 的可归还条件仍要求 `!saw_terminal` (`delivery.rs:409-448`、`469-515`)，所以该后置路径不能承担 SSE 终态候选切换。这个条件不能放宽：它不是可靠的“未交付语义”证明。

## 设计决策

### 1. Preflight 是零语义 SSE 终态的唯一重放 seam

`stream_preflight.rs` 负责在任何语义事件到达 delivery 前读取并重放有限前缀；`aggregate_api.rs` 负责候选、重试预算、预算预留和健康决策。`delivery.rs` 只保留现有读错误/已交付处理，不承担候选策略。

**不变量：** 一旦 prefix 分类为 `Deliver`，该流只能交给 bridge；无论最终是否报错，均不可重新发送原请求。

### 2. Stream preflight 返回事实，不返回 Aggregate 策略

在 `proxy_pipeline/stream_preflight.rs` 增加内部结构化终态结果：

```rust
StreamTerminalFailure {
    message: String,
    error_code: Option<String>, // error.code；缺失时 error.type
}
```

并增加 `StreamPreflightOutcome::TerminalFailure(StreamTerminalFailure)`。

- SSE 判定基于 `response.headers()` 的 `Content-Type: text/event-stream`，而不依赖 `GatewayUpstreamResponse` variant；`headers()` 已同时覆盖 `Blocking` 与 `Stream`。
- 对 `error`、`response.failed`、`response.incomplete`，只有在此前未分类为 `Deliver` 时，提取结构化终态事实。message 采用现有受限错误 hint；分类键优先 `error.code`、回退 `error.type`，而不是保留原始 SSE frame。
- 在 `support/upstream_failure.rs` 复用同一 error-code/type 投影，使 HTTP JSON 和 SSE 嵌套 error object 不各自维护解析规则。该文件同时是分类键的唯一 allow-list 所在处。
- `Failover`、`StatusFailover`、`RetryUsageNotice`、`TransportFailover` 保留其已有语义。`RetryUsageNotice` 不是普通 5xx，继续遵循现有独立配额提示策略。

这使 `preflight_stream_response()` 成为深模块：调用方获得“可交付流 / 终态失败事实 / 传输失败 / 既有配额提示”的小接口，不需要理解 SSE 帧、前缀回放或 body ownership。

### 3. Aggregate 分支按唯一的零交付决策点执行策略

`aggregate_api.rs` 将新的 preflight `TerminalFailure` 与既有 bridge `pending_failover_request` 终态都归一为候选循环内的 `PreDeliveryFailure { message, error_code }`。两种来源共享**同一个** `classify_upstream_failure(502, message, error_code, false, false)` action match：preflight 来源保留原 request；bridge 来源先从 `pending_failover_request` 恢复 request。

这会替代而非复制当前 `aggregate_api.rs:3253-3310` 的 SSE 候选决策。preflight 分支不得直接 `break` 成为第二套策略；它只提供 failure fact，避免 ordinary SSE 5xx 意外跳过同候选重试。

分类器补齐 SSE 的显式键，并为 HTTP JSON 与 SSE 共用：

| 分类键 | 决策 |
| --- | --- |
| `rate_limit_exceeded`、`authentication_error`、`model_not_found` | `CandidateFailover`：当前上游/路由不适用，直接候选 B |
| `invalid_request_error` | `RequestTerminal`：请求不可重放 |
| 未知 `server_error`、传输错误 | `RetrySameCandidate`：消耗本候选 transport budget |

只有上述显式键改变分类器；保留原有容量、Chat capability、reasoning-guard 的优先级和分支。

| 分类结果 | 行为 | 健康与预算 |
| --- | --- | --- |
| `CandidateFailover` | 不发送给客户端；结束当前候选内层循环，外层继续 B | 释放本次未交付的 spend reservation；记录候选失败/冷却资格 |
| `RetrySameCandidate` | 使用 transport budget 重试 A；预算耗尽后外层继续 B | 每次零交付重放前释放 reservation；仅预算耗尽后标记候选失败 |
| `RequestTerminal` | 停止候选和模型 hop；返回一次终态 | 不冷却；按现有终态路径结算/持有 |
| `CapacityRecovery` / `CapabilityRetry` | 复用已有容量/能力分支与独立预算，不能降级成普通重试 | 保持既有 health-neutral 和 immutable-body 不变量 |
| `ReasoningGuardRetry` | 不应由 preflight 产生；保守终态并保留诊断 | 不新增重试 |

`TransportFailover` 同样使用本候选 transport budget：首次后最多一次同候选重试，预算耗尽才切 B。保留原有指数退避计算，但以实际配置的初始预算计算 retry ordinal。

`Failover` / `StatusFailover` 在 Aggregate 原始非 2xx 分支已先处理；若仍从 preflight 抵达，维持当前安全的候选推进，不绕开已有 status 分类。候选循环的 request、reservation、deadline 和 retry counters 保持在原处；只归一失败事实与策略 match，不抽取接收大量可变参数的浅 helper。

### 4. Transport budget 的唯一来源

移除 `AGGREGATE_API_RETRY_ATTEMPTS_PER_CHANNEL` 对 transport retry 的硬编码使用。候选开始时读取：

```text
aggregate_api_transport_retry_attempts()
```

其默认值已经是 `1`，含义为“首次之后再重试一次”。该初始值同时控制：

- mutable remaining budget；
- 内层循环的最大尝试数；
- 指数退避的 attempt ordinal。

`0` 表示不进行同候选重试；无效或缺失值回退 `1`。能力、容量和 reasoning-guard 的独立预算不改变。

将该现有环境变量写入 `docs/en/report/environment-and-runtime-config.md`：范围仅 Aggregate API 的零交付 transport/SSE server-error 重试，不控制候选总数、模型 hop、健康阈值或 OMP 重试。

### 5. 模型降级与 OMP 的职责分层

`proxy.rs` 已从 `ManagedModelV2.fallback_model_slugs` 读取第一条可用 fallback，并限制为最多三个 model hops。无需新增路由代码。

部署配置在已有模型目录 UI 中将：

```text
gpt-5.6-terra -> gpt-5.6-sol
```

保存为 Terra 的 fallback model list。此列表在 Aggregate 和账号路径都返回 `RequestReleased` 后才被 proxy loop 消费；Sol 使用自己的候选集合。没有 Sol 路由时，该 hop 返回有界错误，不再无限尝试。

OMP 的 `C:/Users/shuan/.omp/agent/config.yml` 只改：

```yaml
retry:
  maxRetries: 1
```

不修改 `models.yml`，不写入或暴露 provider secrets，不删除现有角色的跨 Provider `fallbackChains`。OMP 在 CodexManager 返回最终响应后才可执行外层策略，不能干预单个网关请求中的候选循环。

## 状态转移

```text
Responses SSE before semantic event
  -> TerminalFailure(rate_limit/auth/model unsupported)
  -> next same-model candidate

Responses SSE before semantic event
  -> TerminalFailure(generic server/upstream 5xx) or TransportFailover
  -> retry same candidate once
  -> next same-model candidate

Responses SSE after semantic event
  -> Deliver to bridge
  -> any later error is final; no replay

All Terra candidates and account paths exhausted
  -> RequestReleased
  -> proxy selects Terra fallback Sol
  -> Sol resolves its own routes
  -> outer OMP retry/fallback only after the final gateway result
```

## Observability and privacy

No migration or request-log schema changes. Preserve `gateway_upstream_attempt_events`, `attempted_aggregate_api_ids_json`, `model_source=model_fallback`, daily-spend attempt lifecycle and existing sanitized URL handling.

Structured logs added or extended at the Aggregate preflight action point must include trace id, candidate id, candidate position, action (`candidate_failover`, `retry_same_candidate`, `terminal`), bounded error code, retry ordinal and status. They must not include prompt/body, raw SSE frame, authorization data, tool arguments or secrets.

## Compatibility, rollout and rollback

- HTTP non-2xx classification, Chat preflight, all-cooldown, zero-balance, capability routing and reasoning guard retain their existing paths.
- The default retry change is intentionally latency-affecting: normal 5xx goes from first + 3 retries to first + 1 retry. Operators may set the documented value to `0` or a larger bounded `usize` only when they accept the resulting trade-off.
- Rollback source by reverting the two gateway files; restore the prior OMP retry value and remove Terra's catalog fallback list if needed. No migration or persisted code setting must be rolled back.

## Alternatives rejected

1. **Only change `delivery.rs` to accept `saw_terminal=true`** — unsafe: it lacks a proof that no semantic event was already delivered.
2. **Make every 502 advance immediately** — loses transient recovery and bypasses the shared classifier.
3. **Let OMP choose the next candidate** — impossible: OMP only receives CodexManager's final HTTP result and has no candidate health, route or spend context.
4. **Add a new public settings/RPC surface for retry budget** — unnecessary; an existing environment setting and runtime reader already define the control plane.

## Open risks

No product decision is unresolved. Implementation must verify the new structured preflight outcome exhaustively at both consumers (`aggregate_api.rs` and `candidate_executor.rs`), because the latter keeps account-pool policy and must not inherit Aggregate retry budgets accidentally.
