# 合并 codex-retry-gateway 优先优化设计

## Architecture

本任务按现有 CodexManager 分层实现：

- `crates/service/src/gateway/core/runtime_config.rs` 作为运行时配置源，新增 reasoning guard 目标列表、流式开关、非流式开关和内部重试次数。
- `crates/service/src/app_settings/` 负责持久化、启动同步、current API 和 patch API。
- `crates/service/src/gateway/observability/http_bridge/` 负责从响应 usage 中识别 reasoning guard 目标并决定观察、重试或阻断。
- `crates/service/src/gateway/observability/metrics.rs` 负责 Prometheus 计数。
- `apps/src/types/settings.ts`、`apps/src/lib/api/normalize.ts`、`apps/src/lib/store/useAppStore.ts` 和 `apps/src/app/settings/**` 负责前端类型、默认值、normalize 和设置页 UI。

不引入第二套代理，也不把 Node 网关代码移植进仓库。

## Config Model

新增设置字段建议：

- `reasoningGuardTargets: number[]`
  - 默认 `[516, 1034, 1552]`
  - 运行时归一化：正整数、去重、排序或保持输入顺序均可，但 UI 展示应稳定
- `reasoningGuardInterceptStreaming: boolean`
  - 默认 `true`
- `reasoningGuardInterceptNonStreaming: boolean`
  - 默认 `true`
- `reasoningGuardRetryAttempts: number`
  - 默认 `3`
  - `0` 表示命中后不重试，直接按现有阻断路径处理

保留字段：

- `reasoningGuardEnabled`
- `reasoningGuardBypassAfterConsecutive`

兼容规则：

- 如果总开关关闭，目标列表和子开关仍可保存，但响应路径不阻断。
- 如果总开关开启，patch 不能把 `reasoningGuardInterceptStreaming` 和 `reasoningGuardInterceptNonStreaming` 同时保存为 `false`。
- 缺失新增字段时使用默认值。

## Response Flow

现有 `reasoning_guard_error(usage)` 只判断 `516`。应改为：

1. 提取 `usage.reasoning_output_tokens`。
2. 判断 token 是否属于 `current_reasoning_guard_targets()`。
3. 返回包含 token 的消息或结构化匹配结果。

`reasoning_guard_block_message(...)` 应升级为决策函数：

- Disabled: reset consecutive state，计入旁路/观察状态可选。
- NotMatched: reset scope。
- ObserveOnly: 目标命中但对应 stream/non-stream 拦截关闭。
- InternalRetry: 目标命中且仍有 retry budget。
- Block: 目标命中且 retry budget 用尽。
- BypassAfterConsecutive: 保留现有阈值语义。

内部重试不应在 HTTP bridge 层直接重新发请求，因为请求执行和候选选择属于 upstream pipeline。推荐做法是扩展 `UpstreamResponseBridgeResult`，携带一个 reasoning guard action 或复用/增强 `pending_failover_request`，让上游执行循环在同一账号或候选策略内重新执行。

若现有 pipeline 更适合通过 `pending_failover_request` 触发重试，则必须保证：

- retry 次数按单个客户端请求累计。
- reasoning guard retry 不标记账号失败。
- 日志能区分 reasoning guard retry 和普通上游错误 failover。

## Metrics

在 `metrics.rs` 中新增原子计数，至少包含：

- `codexmanager_gateway_reasoning_guard_matches_total`
- `codexmanager_gateway_reasoning_guard_blocks_total`
- `codexmanager_gateway_reasoning_guard_internal_retries_total`
- 按 mode 区分的 label 或独立 counters：`stream` / `non_stream`

当前 metrics 已有 labeled map，可选择复用 label map 或添加少量原子计数。为保持简单和低风险，第一版可用固定原子计数加 Prometheus 行输出。

Capacity retry 额外暴露：

- `codexmanager_gateway_upstream_capacity_internal_retries_total`

该计数只记录由固定 capacity 文案触发的同候选内部重试，不混入普通 429/stateless retry 或 reasoning guard retry。

## Upstream Capacity Retry

固定 capacity 文案：

`Selected model is at capacity. Please try a different model.`

处理规则：

- 在 HTTP bridge 写回客户端前识别该错误，把 `Request` 保存在 bridge result 的 `pending_failover_request` 中。
- `response_finalize` 根据 `RetrySameCandidateReason::UpstreamCapacity` 返回同候选重试，不调用 `apply_gateway_error_follow_up`。
- candidate executor 使用独立的固定预算 `MAX_UPSTREAM_CAPACITY_RETRIES = 1`，不复用 reasoning guard retry attempts。
- 匹配函数兼容项目内 `type=...` / `code=...` 前缀和 `[request_id=...]` 等 debug 后缀，但不做宽泛 capacity 关键词匹配。
- 如果 capacity 预算耗尽，gateway 返回该上游错误给客户端，但仍跳过普通 failover follow-up。

## Passive Model Consistency

本批只做基础观测，不做 UI 大面板。建议先记录/聚合最小信号：

- request model
- effective/fallback model
- upstream response model if present
- final stream response model if present

落点可在 HTTP bridge usage/result 或 request log enrichment 附近。若改动过大，允许把被动模型一致性缩减为日志字段和测试覆盖，不在第一批引入持久化表。

## Frontend

设置页“516 推理保护”改为“Reasoning Guard / 推理保护”：

- 总开关
- 目标 token 列表输入，逗号/空格分隔
- 流式拦截开关
- 非流式拦截开关
- 内部重试次数输入
- 连续命中后放行阈值保留

前端 normalize 必须接受后端缺失字段，回退默认值。

## Rollback

所有新增配置都有默认值，回滚代码后旧字段仍可继续工作。若配置已持久化新增字段，旧版本会忽略未知 app settings key。

## Risks

- 内部重试如果接错层，可能导致重复请求不可控或和账号 failover 混淆。
- 流式严格缓存路径必须继续避免泄漏被阻断内容。
- 设置页已有未提交改动，实施时必须只做必要增量并保留用户已有改动。
