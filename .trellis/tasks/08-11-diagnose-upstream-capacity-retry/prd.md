# 上游容量重试与路由耗尽终态

## Goal

对上游精确容量错误，Aggregate API 与账号池在客户端尚未收到业务内容时，以同一可观测、限时、无副作用的策略重放原请求；当路由策略允许的尝试已耗尽时，网关仅返回一次明确的 `502` 终态，而不将 `429`/`503` 作为最终结果透传。

## Confirmed Facts

- 容量分类器只接受固定文案，可带 `key=value` 前缀和 `[...]` 诊断后缀：`crates/service/src/gateway/mod.rs:66-83`。
- Aggregate API 对 HTTP 非成功和桥接前 SSE 容量错误使用两次重放、0–500 ms / 0–1 s 全抖动及 request deadline：`crates/service/src/gateway/upstream/protocol/aggregate_api.rs:55-148`、`:2146-2179`、`:2325-2375`。
- 账号池只有一次立即容量重放，未读取 `Retry-After`：`crates/service/src/gateway/upstream/proxy_pipeline/candidate_executor.rs:23`、`:693-775`。
- 账号池容量检测要求 bridge 保存 `pending_failover_request`，因此已向客户端交付业务字节的流不会重放：`response_finalize.rs:287-290`、`:345-354`、`:422-431`。
- 两条路径均不读取 `Retry-After`；现有容量测试只覆盖预算、deadline 和指标，未断言 mock upstream 的真实请求次数：`aggregate_api_tests.rs:21-65`。
- `codex-retry-gateway` 先从解析 JSON 和原始正文严格分类容量文案，再由单一请求循环调度重试：`../codex-retry-gateway/gateway.mjs:1661-1688`、`:11270-11325`、`:12489-12572`。不迁移其五次共享预算或 UI/config。
- 网关内部循环有界：Aggregate API 容量重放两次、账号池一次、模型回退最多 `MAX_MODEL_FALLBACK_HOPS=3`：`aggregate_api.rs:55-148`、`candidate_executor.rs:23`、`proxy.rs:809`。
- 容量预算耗尽时，Aggregate API 将最终状态设为 `503`：`aggregate_api.rs:2161-2166`、`:2361-2366`；账号池直接以原上游状态结束：`response_finalize.rs:422-431`。
- 账号池候选耗尽、空候选、混合路由缺少聚合候选和模型路由不可用，都会通过 `proxy.rs` 的多个终态分支返回 `503`；聚合候选为空还可直接返回 `404`/`503`：`proxy.rs:849-1053`、`:1198-1201`、`:1328-1462`，`aggregate_api.rs:1425-1519`、`:1531-1581`。
- 终态响应器按调用方给定的状态原样写入 HTTP 响应：`respond_model_route_error` / `respond_aggregate_route_error` / `respond_hybrid_route_error`（`proxy.rs:258-567`）和 `finalize_terminal_candidate`（`response_finalize.rs:160-184`）。
- 用户的连续请求日志表明外部客户端会在收到当前终态后重新进入网关；仓库没有 Codex 客户端的重试实现，因此只承诺网关端“一次终态响应、无后续内部重试”。

## Requirements

### R1. 安全容量重放

- 仅在未写出客户端业务字节、且能恢复原始请求时重放；文本、工具调用或其他业务事件已交付后绝不重放。
- 重放使用原始不可变请求体，不追加变换、续写标记或修改用户模型。
- 容量错误不触发账号冷却、账号切换、Aggregate API 候选切换或模型回退。

### R2. 统一容量策略

- Aggregate API 与账号池均为首次请求加两次容量重放，并使用相同的 `Retry-After`、全抖动和 deadline 语义；预算不与 reasoning guard、传输重试或通用 429 策略共享。
- 仅接受不超过约 2 秒的 `Retry-After`；头缺失、非法或更长时采用现有 0–500 ms / 0–1 s 全抖动；deadline 优先。
- 保持精确容量文案匹配；可从已解析错误载荷取值，但不得因泛化 `capacity` 文本触发重放。

### R3. 路由耗尽的 502 终态

- 下列终态必须返回 `502`：容量预算耗尽；同一模型的全部普通候选/账号失败；所有候选被 cooldown 或零余额排除；管理模型没有可用的配置路由，且没有可继续的模型回退。
- `502` 终态保留脱敏的现有诊断（如无可用账号、模型路由不可用或最后上游错误）并通过既有 `terminal_text_response` 返回 `server_error`；不新增错误码或改变错误体协议。
- 请求参数错误、鉴权/授权错误、真实不存在的模型、显式配额/日限额和非容量 4xx 保持现有状态；不得把任意 `429`/`5xx` 全部改成 `502`。

### R4. 可观测性与回归覆盖

- 每次容量命中、计划等待、实际重放、deadline 截止和预算耗尽记录 trace ID、上游路径、上游/账号 ID、状态与延迟；不得记录密钥或请求正文。
- 两条路径的指标语义一致，避免实际重试但容量命中/耗尽计数为零。
- 本地 mock upstream 覆盖 HTTP 429/503 JSON、纯文本、首个 SSE 错误、成功恢复、预算耗尽、路由耗尽和已交付流；断言实际请求次数及客户端收到的最终状态。

## Acceptance Criteria

- [ ] 相同容量样本在 Aggregate API 和账号池各执行首次请求加两次同候选重放；mock upstream 的请求次数可断言。
- [ ] 不超过约 2 秒的 `Retry-After` 决定下次请求下界；其他情况使用有界全抖动，deadline 优先。
- [ ] JSON 包装或纯文本中的精确容量文案会触发重放；近似文案不会触发。
- [ ] 已交付业务事件后的容量错误绝不重放；未交付时客户端不看到中间容量错误。
- [ ] Aggregate API 与账号池的容量预算耗尽均返回一次 `502`；不切换候选/模型、不触发冷却，且可由日志与指标追踪。
- [ ] 全部普通候选/账号耗尽，或配置模型最终无匹配 API 时，网关返回 `502`、`error.type=server_error` 与既有脱敏诊断；模型不存在、鉴权失败和日限额等保留原状态。
- [ ] 新增测试先失败后通过；相关服务测试和格式检查通过。

## Out of Scope

- 模拟或控制外部 Codex 客户端的重试策略。
- 对容量错误改选 Aggregate API、账号或模型，或对任意 429/5xx 一概重试/归一化。
- 迁移 `codex-retry-gateway` 的 UI、配置格式或 reasoning/429 共享预算模型。

## Key Decisions and Risks

- 用户已确认 `Retry-After` 最大等待约 2 秒；超过上限的头回退全抖动。
- 用户已确认容量错误不切换候选或模型。因此容量预算耗尽即为本次请求的路由策略终态，不主动尝试其他 API。
- 外部客户端是否绝对停止重试不在本仓库可验证范围；本修复的可验证边界是网关不再在容量/路由耗尽后发送 `429`/`503`，而是一次 `502` 终态。
