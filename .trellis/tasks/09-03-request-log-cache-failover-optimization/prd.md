# 请求日志缓存率与 Aggregate API 故障切换优化

## Goal

依据实际请求日志解释 GPT 缓存率下滑，并设计一个不依赖跨供应商缓存共享的修复方案：让同一会话在 Aggregate API 层稳定地命中同一健康来源；来源不可用时安全切换并收敛到新的健康来源。

## Confirmed facts

研究报告：[`research/cache-analysis.md`](research/cache-analysis.md)。

- 用户在 10:59 前看到的约 66% 有日志依据：当日累计缓存率为 **65.60%**；12:23 的累计值已随故障减少回升到 **71.76%**。滚动 24 小时为 **88.13%**。
- 样本全部经 `actual_source_kind = aggregate_api`、`route_strategy = ordered`、`route_source = route_strategy` 路由；并非 `balanced` 轮询造成。
- 29.83%（54/181）成功请求经历了至少一次 Aggregate API failover。它们的缓存率是 **51.71%**；单次候选成功组为 **81.18%**。
- 5 个高频会话中，来源稳定的相邻请求缓存率是 **78.58%**；来源切换后的相邻请求是 **33.77%**。
- `conversation_bindings` 表为空。现有绑定以 `Account` 为候选，Aggregate API 候选循环未使用它；`session_id` 被记录但没有绑定 Aggregate API 来源。
- 09:00–10:59 的切换高峰与上游限流、临时不可用、Cloudflare 验证和超时重叠。
- Active Candidate `sort=105` 在当日 35 次进入候选链时，最终成功为 0；它是无需代码即可先修复的运维问题。

## Decision

采用**会话级 Aggregate API 来源亲和性**，并保持 failover 的可用性优先级：

1. 同一缓存亲和性键的后续请求优先尝试上次成功的 Aggregate API。
2. 该来源被冷却、余额阻断、disabled、能力不兼容或本次请求失败时，保留现有候选过滤与 fallback；不会为了缓存等待或重试一个已失败来源。
3. fallback 成功后，将绑定更新为最终成功来源，使同一会话后续请求收敛，而非反复回到已失败来源。
4. 该行为通过新的设置显式启用，默认关闭；已有 `ordered`、`balanced` 和指定 Aggregate API 的语义保持不变。
5. 不尝试以跨账号或跨供应商的统一 `prompt_cache_key` 共享缓存：该行为不受 OpenAI 文档保证，也不能取代上游来源稳定性。
6. 不引入延迟重试：数据中的主因是失效来源与候选健康，不是短瞬时容量波动；额外等待会恶化延迟且无法保证缓存恢复。

## Scope

### In scope

- Aggregate API 会话来源绑定的存储、查询、写入、候选重排和健康/能力降级。
- 与现有 cache-affinity route ID 相同的分区语义：`platform_key_hash + protocol + model + cache-affinity key`。
- 只保存来源 ID、绑定时间和必要的安全分类原因；不保存 prompt、完整上游响应或凭据。
- 为最终请求日志增加结构化的 Aggregate API 尝试摘要：候选 ID、顺位、终态和归一化失败类别。保留现有 `attempted_aggregate_api_ids_json` 的兼容读取。
- 存储和网关测试：来源稳定、候选不可用跳过、失败后 rebinding、模型/协议/key 分区以及兼容的禁用路径。
- 运维建议：用户检查 `sort=105` 的实际 GPT/SSE 可用性并在修复前停用；此任务不自动修改用户配置。

### Out of scope

- 自动修改 Aggregate API 的 active 状态、sort 或凭据。
- 跨供应商/跨账号缓存共享实验、提示词改写、客户端 KV cache。
- 改变 `ordered`/`balanced` 的全局排序算法。
- Dashboard、Prometheus 或公开 RPC；请求日志字段足以验证首个版本。它们可在亲和性效果验证后单独规划。
- 新增等待型容量重试。

## Functional requirements

### FR-1 — Opt-in Aggregate API affinity

当启用设置、请求未显式指定单一 `aggregate_api_id` 且存在可用 Aggregate API 候选时：

- 使用已解析的 cache-affinity route ID 查找该请求身份的 Aggregate API 绑定。
- 若绑定的来源仍在过滤后的候选集内，将它移到第一个尝试位置。
- 找不到绑定时，不改变当前候选排序；首次成功才创建绑定。
- 设置关闭、无 affinity ID、显式指定单一 API、或绑定来源不在可用候选集时，行为与当前版本一致。

### FR-2 — Safe failover and convergence

- 绑定来源在本次请求失败时，沿用当前候选循环，继续使用下一个允许的候选。
- 最终成功来源不同于原绑定时，用最终成功来源更新绑定；未来请求从它开始。
- 请求失败时不创建或更新绑定。
- 已被 cooldown、zero-balance、disabled 或 capability filter 排除的绑定来源不得被强行复活。

### FR-3 — Isolation and retention

- 同一来源绑定不得跨 API key identity、协议或模型复用。
- 使用现有哈希化 route ID；不得把原始 `session_id`、`prompt_cache_key` 或 prompt 内容持久化为新索引键。
- 绑定记录必须受明确 TTL/清理策略约束，避免无限增长。TTL 的默认值应匹配现有 prompt cache 的 24 小时行为，配置值必须有安全上限。

### FR-4 — Exact failover evidence

- 每个最终请求保留当前候选 ID 数组，新增可选结构化尝试摘要。
- 每个摘要至少包含：Aggregate API ID、尝试顺位、终态（success/failure/skipped）和归一化失败类别；原始上游错误仅留在运行日志。
- 日志能回答“哪一个候选在什么位置失败、为何最终换源”，无需将密钥或完整上游 URL 暴露给 UI/API 调用方。

## Acceptance criteria

- [ ] 启用亲和性后，同一有效 affinity ID 的第二次可用请求优先选择上一次成功的 Aggregate API。
- [ ] 绑定来源不可用或本次失败时，请求仍按既有冷却、余额、能力和 fallback 规则成功切换；成功 fallback 后下一次请求优先该 fallback 来源。
- [ ] 关闭设置、无 affinity ID、绑定不存在、显式指定 Aggregate API 的路径均保持当前候选排序。
- [ ] 同一个 session/prompt cache key 在不同 key identity、协议或模型下不共享绑定。
- [ ] 绑定和尝试摘要不含原始 prompt、原始会话 ID、密钥或完整上游错误正文。
- [ ] 新增迁移可升级现有数据库，且默认行为不建立绑定或改变路由。
- [ ] 请求日志能区分初始来源、每个候选尝试的顺位与归一化失败类别。
- [ ] 覆盖以上可观察行为的 storage/gateway 回归测试通过；`cargo test -p codexmanager-service` 及受影响 core tests 是计划质量门。

## Success measure after deployment

同类流量在设置开启后的 24 小时内：

- `attempt_count > 1` 的占比和“相邻请求来源改变”的占比显著低于 2026-09-03 样本的 29.83% 和 11.6%（21/181）。
- 重点会话的“来源稳定”缓存率应接近当前 78.58% 基线；不把 81.18% 的单次成功组视为承诺，因为上游缓存实现和故障状态不受本系统控制。

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| 会话固定来源造成负载热点 | 默认关闭；仅为同一 cache-affinity ID 提升已成功来源，不改变新会话的策略排序。 |
| 绑定到已故障来源降低可用性 | 只在现有候选过滤后重排；本次失败立刻走 fallback 并在成功后 rebinding。 |
| 绑定表无限增长 | TTL、索引和维护清理；不存原始会话标识。 |
| 上游缓存本就不共享 | 设计只改善来源稳定性，不宣称跨来源缓存复用。 |
| 复杂观测数据泄漏 | 只持久化归一化失败类别与 ID/位置；原始错误继续限定在本地 trace log。 |

## Planning status

数据分析与需求已完成。`design.md` 和 `implement.md` 需要将此方案映射到具体存储、网关候选准备和日志落点。用户尚未批准生产实现。
