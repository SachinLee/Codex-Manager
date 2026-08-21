# Outcome

## Delivery
- Status: complete
- Summary: 管理员仪表盘的模型趋势图悬浮框现在保留当前 Token/请求数，并显示同一模型或总计、同一时间桶的美元费用和缓存率。聚合 API 页面已移除冗余的“今日模型用量”卡片及其前端模型日用量请求；按连接的日用量、费用、缓存率与刷新行为保留。

## Acceptance Criteria
| Criterion | Result | Evidence |
| --- | --- | --- |
| AC-001 | PASS | `tests/dashboard-usage-tooltip.spec.ts` 以实际悬浮验证模型 Token、请求数、费用和缓存率；标准 Playwright 串行命令通过。 |
| AC-002 | PASS | 同一测试切回 Token 后启用“全部模型”，验证总计桶 `$3.75` 和 `100%`。 |
| AC-003 | PASS | `tests/aggregate-api-usage-refresh.spec.ts` 在初载、自动刷新、焦点、重连、失活与 keep-alive 返回中断言模型日用量 RPC 为零，同时验证连接日用量继续刷新；标准 Playwright 串行命令通过。 |

## Implementation
- `apps/src/components/dashboard/admin-usage-trend-chart.tsx`
  - chart row 保留 `total_usage` 与 `${modelKey}_usage` 的原始 `DashboardTokenUsage`，而曲线继续仅使用原有数值 `total` / `modelN` dataKey。
  - tooltip 通过 Recharts `item.dataKey` 获取同一桶 usage，附加 `formatUsdAmount()` 的费用和经夹紧后的缓存率；无输入显示 `0%`。
- `apps/src/app/aggregate-api/page.tsx`
  - 删除模型日用量 query、展开 state、模型卡片、专用 tooltip 构造函数和仅服务这些元素的 import/type/icon。
  - 保留 `requestlog/aggregate_api_daily_usage` 的 5 秒、挂载、焦点、重连与 keep-alive 刷新配置。
- `apps/tests/dashboard-usage-tooltip.spec.ts`
  - 新增浏览器回归，覆盖模型与总计、Token 与请求数、`input=0`、负缓存和缓存超过输入的显示边界。
- `apps/tests/aggregate-api-usage-refresh.spec.ts`
  - 追踪 `requestlog/model_daily_usage`，在完整页面生命周期中断言零调用。

## TDD Evidence
- RED: `pnpm -C apps exec playwright test --config=playwright.low-memory.config.ts --workers=1 tests/dashboard-usage-tooltip.spec.ts` 实际执行并失败：tooltip 收到 `model-cache-verified1.00M`，缺少预期 `费用`。
- GREEN: `NODE_OPTIONS=--max-old-space-size=1024` 和系统 Chrome executable 环境下执行 `pnpm -C apps exec playwright test --workers=1 tests/dashboard-usage-tooltip.spec.ts tests/aggregate-api-usage-refresh.spec.ts`，2 passed。临时低内存 Playwright 配置已删除。

## Verification
- `pnpm -C apps run lint` — PASS；退出 0，14 个 warning 均位于任务范围外文件。
- `pnpm -C apps run test:runtime` — PASS；207 passed，0 failed。
- `NODE_OPTIONS=--max-old-space-size=1024 pnpm -C apps run build:desktop` — PASS；静态导出全部页面成功。
- `NODE_OPTIONS=--max-old-space-size=1024` 与系统 Chrome executable 环境下的标准 Playwright 串行命令 — PASS；2 passed，1 worker。
- `trellis-check` — PASS；未发现范围内缺陷或临时配置残留。
- tooltip 两个重复 `usage != null` 守卫合并后，标准 Playwright dashboard 回归 — PASS；随后 `pnpm -C apps run lint` 退出 0（同 14 个任务范围外 warning）。

## Independent Review
- 初次 `code-reviewer` 发现：总计测试在请求数模式错误期待 Token 文本，且缓存边界未覆盖。两项均已修复，并在最终标准 Playwright 串行回归中验证。
- `workflow-reviewer` 已尝试启动，但 OMP 环境返回 `No model selected`；未产生代码评审结论。
- 另一个全新上下文的 `code-reviewer` 最终复审结论为无发现；确认 AC-001/2/3、dataKey 映射、缓存边界、请求移除和测试可靠性。
- `code-simplifier` 仅建议合并 tooltip 的重复 null 守卫；该等价简化已实施，并由新的 `code-reviewer` 静态复审通过，随后重跑受影响的标准 Playwright dashboard 回归和 lint。

## Specification Update
- NOT APPLICABLE：未改变跨层接口、RPC、持久化或公共前端约定；实现复用既有 billing formatter 和局部 Recharts data row 模式，没有足以推广到 `.trellis/spec/` 的新通用代码规范。

## Commits
- NOT COMMITTED：用户未要求提交，且工作区存在来自其他并行工作的未提交改动。

## Remaining Risk
- 工作区共有大量无关未提交/未跟踪改动；本任务仅修改和验证了四个列出的任务文件，未触碰其他变更。
- 构建与标准 Playwright 回归需以 `NODE_OPTIONS=--max-old-space-size=1024` 在当前低内存工作站运行；该限制下已通过。
