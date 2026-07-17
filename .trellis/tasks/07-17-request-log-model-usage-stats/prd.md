# 请求日志模型使用统计

## Goal

在「请求日志」页增加**按模型维度的使用统计**（方案 A：全宽可折叠表格卡），与当前筛选条件联动，帮助用户快速了解各模型的请求量、Token 与费用贡献。

## Background

- 页面已有筛选汇总：当前结果条数、累计 Token、筛选费用、长上下文费用。
- 明细表已展示单条请求的模型信息，但缺少聚合视角。
- 后端已有 `request_token_stats` 按模型汇总（Dashboard/Quota 用），但与日志页的 status/search 筛选口径不一致；本需求以**日志筛选视角**为准。

## Requirements

### 功能

1. 在请求日志页的**汇总卡下方、请求明细表上方**展示「模型使用统计」区块。
2. 统计**跟随当前筛选**：时间范围、状态（ALL/2XX/4XX/5XX）、搜索关键字（及现有与 summary 一致的其他 list 参数）。
3. 布局采用**方案 A**：
   - 全宽 `glass-card` 表格
   - 可折叠（默认展开；模型数 > 8 时可默认只展示 Top 8，支持展开全部）
   - 费用占比条
   - 表头可排序（默认按费用 DESC，其次 Token DESC）
4. v1 列字段：
   - 模型名
   - 请求数
   - 成功数 / 异常数
   - Token 合计
   - 估算费用
   - 费用占比（相对当前筛选总费用；总费用为 0 时回退按 Token 占比）
   - 缓存率（cached/input，input 为 0 时显示 `-`）
5. 空态：「当前筛选下暂无模型用量」；加载态使用与汇总区一致的 skeleton，不阻塞明细表既有 loading。
6. 可选增强（v1 建议实现）：点击模型行 → 将模型名写入搜索框并刷新明细，便于下钻。
7. 权限：admin 看全量；非 admin 仅自己 API Key 下的统计（与现有 `requestlog/*` 一致）。
8. 返回上限：最多 Top 50 模型；超出部分可汇总为「其他」桶或标记 truncated（实现时二选一，默认 Top 50 + truncated 标记）。

### 非目标（Out of Scope）

- 不改计费规则 / 模型路由。
- 不做跨日趋势图、CSV 导出（可二期）。
- 不替换 Dashboard / Quota 的 `topModels` 长期用量口径。
- v1 不强制拆出独立 Tab。

### Constraints

- 前端静态导出兼容（`apps` output export）。
- API 通过既有 `apps/src/lib/api/` 封装与 transport 同步。
- 数据口径尽量与现有 `RequestLogFilterSummary` 的 Token/费用计算一致。
- 中文 UI 文案走 i18n；en/ko/ru 同步键。
- 代码与标识符保持英文原文。

## Acceptance Criteria

- [x] 请求日志页在汇总卡与明细表之间展示「模型使用统计」表格卡（方案 A）。
- [x] 变更时间 / 状态 / 搜索后，模型统计与上方汇总同步更新。
- [ ] 同一筛选下，各模型 `requestCount` 之和等于（或在 truncated 时 ≤）`summary.filteredCount`；各模型 Token/费用之和与 summary 总量在无 truncated 时一致（Guard 规则若 v1 未计入需在 UI 脚注说明，且与实现文档一致）。
- [x] 默认按费用降序；可切换排序维度。
- [x] 无数据时空态正确；加载时有 skeleton。
- [x] admin / 非 admin 权限隔离正确。
- [x] 后端 storage + service 单测覆盖：多模型聚合、status 过滤、key 隔离、空模型归一、LIMIT。
- [x] `pnpm -C apps run build` 通过；相关 `cargo test` 通过。
- [x] 旧客户端缺少 `modelStats` 字段时前端降级为空数组，不报错。

## Notes

- 产品确认：方案 A；创建 Trellis 任务 `07-17-request-log-model-usage-stats`。
- 复杂任务：需 `design.md` + `implement.md`，经用户确认后再 `task.py start` 实现。
