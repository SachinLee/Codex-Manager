# 融合 codex-helper/CodexPlusPlus P1-P3

## Goal

实现融合优化方案中的前三项能力：

1. P1：聚合 API / 上游能力诊断 MVP。
2. P2：请求路由证据与自动 Policy Action 基础能力。
3. P3：图片与响应语义验收。

后续 P4+ 暂不做：模型目录增强、Codex App 兼容/增强中心、多实例/Fleet 观测、Stepwise、Zed、worktree、微信助手等能力不进入本任务范围。

## Requirements

- 在不改变现有正常网关转发行为的前提下，为聚合 API 或上游增加能力诊断入口。
- 能力诊断至少覆盖 `/models`、`/v1/responses`、`/responses/compact`、Responses WebSocket、hosted `image_generation` 的可观测结果或明确的未测试状态。
- 诊断结果必须结构化表达 `supported`、`unsupported` 或 `unknown`，并携带原因、HTTP 状态、风险提示和推荐模式。
- 诊断行为必须与正常请求路由隔离，不能因为探测失败而自动禁用账号、聚合 API 或改变路由状态。
- 引入路由证据模型，能记录候选被跳过、冷却、余额耗尽、并发饱和、上游限流等原因。
- 引入系统拥有的临时 policy action 基础模型，首期仅支持 cooldown，不覆盖手动禁用或用户显式配置。
- 请求日志或相关 RPC 输出中应能展示路由证据和 policy action 摘要。
- 对 hosted image generation 响应增加语义验收：HTTP 成功但缺少有效图片结果时，必须被标记为网关语义失败。
- P1-P3 的后端结构应保持在 `crates/service` 的网关/聚合 API边界内；涉及持久化基础时才改 `crates/core`。
- 前端只做 P1-P3 必要的管理面展示，不引入 Codex App CDP 注入类能力。

## Constraints

- 当前工作区已有与网关重试优化相关的未提交改动，不能回退、覆盖或混淆这些改动。
- 不记录真实 API Key、Bearer token、ChatGPT token、账号敏感信息或完整图片 base64 到日志。
- live smoke 类可能消耗额度的动作必须显式区分，默认诊断应尽量轻量。
- 保持桌面模式和 Service/Web/Docker 模式可用；桌面 IPC 继续通过现有 typed API wrapper，Web fallback 继续走 RPC/transport 栈。

## Acceptance Criteria

- [ ] 聚合 API 或上游详情可触发能力诊断，并返回结构化诊断结果。
- [ ] 诊断失败不会改变账号、聚合 API、路由、cooldown 或余额状态。
- [ ] 路由证据模型覆盖至少：quota/balance、rate limit、transport、capacity/local concurrency、capability/unsupported。
- [ ] 系统 cooldown policy action 有 owner、target、reason、created/expires、remaining seconds，并能过期。
- [ ] 请求日志或相关查询能返回 route evidence / policy action 摘要。
- [ ] hosted image generation 缺少 `image_generation_call.result` 时，不再按成功处理。
- [ ] 新增或更新测试覆盖能力诊断解析、policy action 生命周期、语义验收失败。
- [ ] P4+ 能力没有被实现或引入主导航。

## Notes

- 借鉴来源：`codex-helper` v0.20.1 的 relay capabilities、provider signal/policy action、response semantics；`CodexPlusPlus` 的模型/插件增强仅作为后续阶段参考。
