# 仪表盘用量分析悬浮明细

## Goal

将“按模型、按时间桶”的费用与缓存率明细集中到管理员仪表盘的“用量分析”模型趋势图悬浮框，并从聚合 API 页面移除冗余的“今日模型用量”卡片及其数据请求；不改变既有统计口径、曲线、范围或聚合 API 的按连接日用量。

## Current Behavior and Problem

- `AdminUsageTrendChart` 仅把每个时间桶的 Token 或请求数映射为数值；悬浮框只显示模型名与当前指标值，尽管其 summary 已含费用、输入 Token 和缓存输入 Token。
- 聚合 API 页面单独发起 `requestlog/model_daily_usage` 查询并显示“今日模型用量”卡片、展开控件和模型 tooltip，造成模型级用量入口重复。
- 仪表盘模型序列与总计序列已通过既有 `DashboardTokenUsage` 提供所需字段；本任务不扩展后端、RPC 或统计聚合。

## In Scope

- 管理员仪表盘“用量分析”的模型趋势图悬浮框：对当前时间桶的可见模型和启用的“全部模型”曲线显示现有指标、估算费用和缓存率。
- 聚合 API 页面删除“今日模型用量”卡片、展开状态、模型日用量查询、专用 tooltip 构造函数及仅为这些逻辑存在的类型/图标依赖。
- 聚合 API 页面保留按上游连接的“今日用量”列、费用、缓存率与其刷新行为。
- 更新浏览器回归，以验证仪表盘的悬浮明细，以及聚合 API 页面不渲染该卡片且不请求 `requestlog/model_daily_usage`。

## Out of Scope

- 新的统计口径、数据库迁移、服务端 RPC 字段、请求参数或 `requestlog/model_daily_usage` 后端能力的删除。
- 修改仪表盘图例、时间范围、缩放、模型选择上限或非管理员仪表盘。
- 修改聚合 API 的连接配置、余额、路由、模型发现或按连接日用量行为。

## Actors and Affected Systems

- 管理员：在本地网关模式下查看仪表盘或管理聚合 API。
- 前端受影响文件：`apps/src/components/dashboard/admin-usage-trend-chart.tsx`、`apps/src/app/aggregate-api/page.tsx` 及对应 Playwright 测试。

## Assumptions and Constraints

- 风险配置为 standard：两个可观察前端行为变更均需端到端回归、前端 lint、运行时测试和静态导出构建。
- 费用使用项目既有美元格式化；缓存率为 `clamp(cachedInputTokens, 0, inputTokens) / inputTokens`，无输入时为 `0%`。
- 不存在某模型或时间桶时，仪表盘现有零值与隐藏语义不变；tooltip 不得补算或修改曲线值。
- 聚合 API 页面在首次加载、自动刷新、窗口聚焦、断线重连和返回 keep-alive 页面时都不得调用模型日用量 RPC。
- 验证不得并发运行多个 Node、Playwright 或构建进程；测试使用单 worker/单测试并发，优先降低峰值内存占用，不影响其他工作。

## Acceptance Criteria

### AC-001: 模型曲线悬浮详情

- Scenario: 管理员在 Token 或请求数模式悬浮已选模型的任一非零时间桶。
- Action: 打开模型趋势图的悬浮框。
- Expected: 保留当前指标值，并显示同一模型、同一时间桶的估算费用与缓存率。
- Must not: 改变曲线数值、模型排序、已选模型、时间粒度或请求数/Token 指标切换行为。
- Verification method: Mock dashboard RPC 的 Playwright 悬浮交互回归及构建后实际页面确认。

### AC-002: 总计曲线悬浮详情

- Scenario: 管理员启用“全部模型”后悬浮任一时间桶。
- Action: 打开总计曲线的悬浮项。
- Expected: 保留现有总体指标值，并显示该时间桶总体的估算费用与缓存率。
- Must not: 改变总计曲线的聚合结果或显示条件。
- Verification method: Mock dashboard RPC 的 Playwright 悬浮交互回归及构建后实际页面确认。

### AC-003: 聚合 API 页面移除模型日用量

- Scenario: 管理员访问聚合 API 页面、使其保持激活、切换页面后返回、窗口聚焦或断线重连。
- Action: 观察页面内容与 RPC 请求。
- Expected: 页面不渲染“今日模型用量”卡片或展开控件，且不会调用 `requestlog/model_daily_usage`；按连接的聚合 API 日用量及其刷新继续正常工作。
- Must not: 删除聚合 API 的日用量列、费用/缓存率展示，或删除后端模型日用量能力。
- Verification method: 现有聚合 API Playwright 刷新回归改为明确跟踪该 RPC 的零调用，并断言按连接日用量持续刷新。

