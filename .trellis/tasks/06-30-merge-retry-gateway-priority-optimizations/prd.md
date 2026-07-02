# 合并 codex-retry-gateway 优先优化

## Goal

把 `codex-retry-gateway` 最新版本中适合 CodexManager 的优先级优化落地到本项目现有 Rust gateway 和设置体系中，提升 reasoning guard 的可配置性、命中后的自恢复能力、运行时可观测性，并为模型声明一致性排障提供基础数据。

本任务不复制 Node 网关实现，而是在现有 `crates/service/src/gateway/`、app settings API、Next.js 设置页和测试体系内扩展。

## Requirements

1. Reasoning guard 规则集
   - 现有 `reasoning_tokens=516` 保护应扩展为可配置整数列表。
   - 默认值为 `516, 1034, 1552`，保留兼容已有 516 行为。
   - 非法、空白或重复值应被规范化，不能导致运行时 panic。

2. 流式 / 非流式拦截开关
   - 保留总开关 `reasoningGuardEnabled`。
   - 新增流式和非流式独立拦截开关。
   - 不允许在总开关开启时同时关闭流式和非流式拦截；如果总开关关闭，则全部观察逻辑可继续安全旁路。

3. 命中后网关内部重试
   - 新增 `reasoningGuardRetryAttempts`，默认 `3`，`0` 表示不做内部重试。
   - 命中可拦截的 reasoning guard 响应时，优先在 gateway 内部尝试重试上游，超过次数后再返回 502。
   - Reasoning guard 命中和内部重试不应错误标记账号不可用、供应商失败或冷却。

4. 统计与日志
   - 增加 reasoning guard 命中次数、实际阻断次数、内部重试次数。
   - 至少区分流式和非流式命中/阻断。
   - Prometheus `/metrics` 应暴露新增计数。
   - 网关日志或请求日志应能区分 `observe_only`、`internal_retry`、`blocked`、`bypassed_after_consecutive` 等动作中的关键路径。

5. 模型一致性被动观测基础
   - 记录请求模型、fallback/路由模型、上游响应声明模型、流式最终声明模型等可从现有请求/响应中可靠获得的信号。
   - 本批只做低风险被动观测和指标/日志基础，不做主动探针、不做昂贵上下文请求、不做模型真实性判断。

6. Capacity 错误同候选重试
   - 当上游错误消息精确匹配 `Selected model is at capacity. Please try a different model.` 时，gateway 应优先对同一候选账号做一次内部重试。
   - capacity 内部重试不应切换候选账号、不应记录普通 failover、不应由 gateway error follow-up 标记账号失败或冷却。
   - 匹配范围必须保持窄：只接受该固定文案及项目内已知的错误前缀/诊断后缀形式，不做泛化的 `capacity` 关键词匹配。

7. UI / API / 持久化
   - App settings 返回和 patch 应包含新增 reasoning guard 配置。
   - 设置页的“516 推理保护”区域应改为更通用的 reasoning guard 配置，展示规则集、流式/非流式开关和重试次数。
   - Web/service-mode 和 desktop transport 均应通过现有 typed settings API 工作。

8. Compatibility
   - 现有已持久化设置缺少新增字段时，应自动使用安全默认值。
   - 已有 `reasoningGuardEnabled` 和 `reasoningGuardBypassAfterConsecutive` 语义保持兼容。
   - 现有 gateway 转发、账号路由、quota guard、model forward rules 不应发生无关行为变化。

## Acceptance Criteria

- [ ] 默认 reasoning guard 目标为 `[516, 1034, 1552]`，设置页和 app settings API 可读写该列表。
- [ ] 非流式响应命中任一配置目标时，按 `reasoningGuardRetryAttempts` 先内部重试，超限后返回 reasoning guard 502。
- [ ] 流式响应命中任一配置目标时，遵循流式开关和重试次数；严格缓存路径不能向客户端泄漏应被拦截的 delta。
- [ ] 当对应流式/非流式开关关闭时，命中只计入观察/命中指标，不实际阻断。
- [ ] Reasoning guard 结果不会被 account status 逻辑误判为账号失败或供应商失败。
- [ ] Capacity 固定文案错误会同候选重试一次；成功恢复时客户端只收到恢复后的响应，且 failover 计数不增加。
- [ ] Prometheus metrics 包含新增 reasoning guard 命中、阻断、内部重试指标。
- [ ] App settings normalize/store/defaults 覆盖新增字段，设置页可保存并恢复。
- [ ] 增加或更新后端单元/集成测试和前端 runtime tests，覆盖默认值、配置 patch、命中规则、内部重试和指标。

## Notes

- 已确认现有落点：
  - `crates/service/src/gateway/observability/http_bridge/reasoning_guard.rs`
  - `crates/service/src/gateway/observability/http_bridge/delivery.rs`
  - `crates/service/src/gateway/observability/http_bridge/aggregate/output_text.rs`
  - `crates/service/src/gateway/observability/metrics.rs`
  - `crates/service/src/app_settings/**`
  - `apps/src/app/settings/components/gateway-tab-content.tsx`
  - `apps/src/types/settings.ts`

## Out of Scope

- 不复制 `codex-retry-gateway` 的 Node 管理 UI、安装脚本或 Codex config.toml 修改逻辑。
- 不实现主动探针、长上下文探针、图片探针或自动后台探测。
- 不做模型真实性判断，只记录被动声明和可观测差异。
- 不重构 gateway pipeline 或账号路由策略。
