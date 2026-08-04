# 聚合 API 上游状态检测

## Goal

为聚合 API 上游提供可解释、可持续更新的状态检测能力，让用户识别不可用、退化或受限的上游，并让网关在后续可基于可信状态做保守路由治理，减少无效请求与故障扩散。

## User Outcome

- 用户能在聚合 API 列表和详情中看到当前健康状态、最近检测时间、延迟、错误类别、连续失败次数和恢复时间。
- 所有 active 上游都能从真实网关请求获得被动状态；用户可按单个上游开启低频主动探测。
- 不稳定上游会被临时摘除，恢复必须由成功请求或 half-open 主动探测确认；人工 active/disabled 配置不会被健康检测覆盖。

## Confirmed Decisions

- 路由策略：保守自动治理。确定性故障或达到连续失败阈值的瞬时故障可临时摘除；单次超时/5xx 只记录观测。
- 主动探测：按聚合 API 逐个 opt-in，迁移后默认关闭；被动观测对所有 active API 生效。
- 探测范围：健康时检测代表模型；模型级 cooldown 恢复时检测对应模型。
- 探测节奏：健康 15 分钟，degraded 5 分钟，cooldown 到期执行 half-open，并使用 jitter。
- 确定性鉴权故障：401/403 单次确认后进入 source 级临时摘除，默认 30 分钟后 half-open；不改人工启停状态。
- MVP UI：列表显示按业务含义归并的推荐使用/可用/需注意/不可用/冷却中状态；详情显示主动/被动最近观测、连续失败、冷却时间和最近事件；提供立即检测、解除冷却与人工启停；暂不做桌面通知、webhook 或外部告警。

## Repository Facts

- `crates/service/src/aggregate_api.rs:2476` 已有按 provider 发起真实最小请求的 `test_aggregate_api_connection`。
- `crates/core/src/storage/aggregate_apis.rs:736` 目前只保存最新 `last_test_at/status/error`，没有健康历史或调度配置。
- `crates/service/src/gateway/routing/aggregate_api_cooldown.rs:199` 已按 `api_id + upstream_model` 维护进程内连续失败和 5 分钟 cooldown，成功会清除该模型状态。
- `crates/service/src/usage/refresh/runner.rs:348` 已提供动态 polling、jitter、失败退避和后台 worker 启动模式；`batch.rs:115` 已刷新聚合 API 余额。
- 聚合 API 的人工 `status`、健康状态和临时 cooldown 必须保持独立。
- 参考分析见 `research/reference-analysis.md`：sub2api 提供 scheduled test、challenge、degraded、历史与 single-flight；CLIProxyAPI/CPA 提供模型级 cooldown、错误分类、持久化恢复和避免网络误判的策略。

## Requirements

1. 定义主动探测与被动观测的统一事件、状态机、错误分类、置信度和恢复规则。
2. 主动探测复用现有 provider probe/upstream client，支持 CodexManager 当前聚合 API provider/protocol，不另建旁路鉴权。
3. 主动探测配置按 source opt-in，支持 enable、interval、代表模型覆盖和并发/超时安全上限；默认关闭且不产生迁移后的外部调用。
4. 被动观测接入现有 aggregate API attempt outcome，忽略 request-scoped 错误，按 source/model scope 维护连续失败、cooldown 和 half-open。
5. 对 401/403、404/model-not-supported、429/Retry-After、timeout/408/5xx、DNS/TLS 和请求错误给出不同治理范围与恢复时间。
6. 持久化当前健康摘要和有限历史事件；过期历史可裁剪，敏感 header、token、完整响应体不得入库。
7. 提供统一 RPC/API wrapper，保证 Tauri、service-mode web 和 Next.js 使用同一命令链；提供列表、详情、立即检测、重置/恢复和配置更新能力。
8. UI 需清楚区分人工 disabled、健康 unknown/healthy/degraded/unhealthy/cooldown/recovering 与余额检测结果。
9. 所有自动摘除必须可解释、可审计、可人工恢复，并以 source active 为前置条件；健康系统异常不得阻塞正常路由。
10. 覆盖 SQLite migration/storage、状态机、调度并发、probe 错误分类、路由候选、RPC transport、前端状态呈现和回归测试。

## Acceptance Criteria

- [ ] 新建 active 聚合 API 后，不会因迁移默认值自动发起主动探测；开启该 API 的检测后按 15 分钟周期运行，degraded 时变为 5 分钟，并有 jitter/single-flight/并发限制。
- [ ] 健康探测使用代表模型；某模型处于 cooldown 时，half-open 探测使用该模型；探测成功恢复对应 scope，失败按分类更新状态。
- [ ] 单次 timeout/5xx 不摘除；达到连续失败阈值后才进入 cooldown；401/403 单次确认后可进入 source 级临时摘除；request-scoped 错误不计入。
- [ ] 429 使用 Retry-After，否则指数退避；404/model-not-supported 仅影响模型 scope；健康成功清零相应失败计数并允许恢复路由。
- [ ] service 重启后，未过期的持久化 cooldown/health 状态仍能阻止候选；过期项进入 half-open 或 unknown，不永久阻塞。
- [ ] API/页面展示当前状态、来源、最近检测时间、延迟、错误类别/脱敏原因、连续失败、cooldown 截止和最近历史；列表使用推荐使用/可用/需注意/不可用/冷却中业务标签；人工 disabled 与健康状态不混淆。
- [ ] 手动检测、手动恢复和配置开关通过现有 `transport`/RPC 链路在 desktop 与 service-mode web 均可用。
- [ ] 删除 source 会级联清理配置、当前状态和历史；健康 worker 退出/失败不会导致服务启动失败或路由线程阻塞。
- [ ] 相关 Rust、SQLite、RPC、frontend 测试通过；文档记录新增设置/环境变量和验证命令。

## Out of Scope

- 自动删除、修改或永久禁用上游。
- 高频压力测试、完整模型质量评测、内容安全评测。
- 桌面通知、webhook、邮件、第三方告警集成。
- 余额/配额查询本身的重构；仅在状态设计中区分其结果，不把余额成功等同于 API 可用。

## Risks and Deferred Items

- 真实 probe 会产生上游调用成本；默认 opt-in、代表模型和 15 分钟周期控制风险。
- 某些聚合供应商可能拒绝 challenge 或不支持统一最小请求；探测器必须保留 provider-specific adapter，并允许 unknown/observe 而非误判 unhealthy。
- 进程内 cooldown 现有行为需要与持久化健康状态合并，迁移期间以更保守的排除结果为准，但不得改变人工 status。
- 多实例 service 目前不是主要部署形态；若未来共享 SQLite 的多进程部署，需要增加 lease/分布式 single-flight 决策。
