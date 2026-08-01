# 修复聚合 API 今日用量刷新

## Goal

聚合 API 页面在网关持续产生新请求时，应自动显示最新的当日 Token 与费用；用户无需重载应用，顶部统计和列表行不得长期停留在旧值。

## Background

- 顶部“今日 Token / 今日费用 / 平均缓存率”和列表“今日用量”共用 `requestlog/aggregate_api_daily_usage` 查询。
- 当前代码只在页面激活时按 5 秒轮询，并继承全局关闭的 mount/focus/reconnect 重取策略；`refetchIntervalInBackground` 也为 `false`。
- 当前运行数据库显示请求的 `actual_source_kind/id` 正确，但事务写入遗漏 `request_token_stats.aggregate_api_id`。聚合 RPC 因此过滤掉当日新请求，页面轮询只能得到空结果。

## Requirements

1. 聚合 API 页面处于当前 shell 页面且服务已连接时，今日聚合用量必须持续刷新，目标间隔不超过 5 秒。
2. 从其他 keep-alive 页面返回聚合 API 页面时，必须立即重取，不得复用旧成功结果直至下一次偶然刷新。
3. 窗口重新获得焦点或服务重连时，必须触发重取。
4. 顶部汇总和列表行必须继续使用同一份查询结果并在同一刷新周期更新。
5. 非当前聚合 API 页面不得持续轮询；刷新失败时保留最后一次成功数据，不清空统计。
6. 修复 Core 事务写入的聚合 API 归属字段；兼容已有 `aggregate_api_id` 缺失、但 `actual_source_*` 完整的历史记录。

## Acceptance Criteria

- [ ] Mock RPC 首次返回旧值、随后返回新值时，页面无需 reload，5 秒轮询窗口内顶部费用和对应列表行都显示新值。
- [ ] 离开并返回 `/aggregate-api/` 后，页面主动发起新的日用量 RPC 请求。
- [ ] 页面失焦/恢复或服务重连后，日用量查询可恢复，不会永久停在旧成功数据。
- [ ] 非当前 shell 页面不会继续执行聚合日用量周期查询。
- [ ] `pnpm -C apps run test:runtime`、目标 Playwright 用例和 `pnpm -C apps run build:desktop` 通过。

## Out of Scope

- 修改费用计算、Guard 重试计费或供应商余额查询。
- 重设计聚合 API 页面或新增后台推送协议。
- 将“今日模型用量”改为仅统计聚合 API 来源。

## Technical Notes

- 运行时调查记录见 `research/refresh-logic.md`。
- 后端 RPC 已验证能够返回当前值，本任务优先修复前端 TanStack Query 激活与恢复策略。
