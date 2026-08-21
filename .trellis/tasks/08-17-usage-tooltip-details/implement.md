# Implementation Plan: 仪表盘用量趋势图悬浮明细 + 聚合 API 页面模型日用量移除

Quality profile: **standard**

## Preconditions

- 已确认 `DashboardTokenUsage` 在仪表盘 summary 的总计序列与模型序列中均已包含 `estimatedCostUsd`、`inputTokens` 与 `cachedInputTokens`。
- 已确认 `ChartTooltipContent` 把 Recharts 项的 `dataKey` 和完整 row payload 传给 formatter。
- 复用 `@/lib/utils/billing` 的 `formatUsdAmount` 和 `formatCacheRateValue`；现有的 `费用` 与 `缓存率` i18n 键已覆盖仪表盘支持的语言。
- `aggregate-api/page.tsx` 唯一的 `listModelDailyUsageStats` 调用在 `modelDailyUsageQuery`（行 269–289）；`accountClient.listModelDailyUsageStats` 前端方法本身不删除。
- 本任务不得修改服务端、RPC、数据库、仪表盘类型、后端 `requestlog/model_daily_usage` 能力，或聚合 API 页面的连接配置、余额、路由、模型发现和按连接日用量行为。

---

## Slice 1: AC-001、AC-002 — 建立仪表盘悬浮可观察回归（RED 先行）

- **Behavior**：管理员在真实渲染的仪表盘上悬浮模型或总计曲线时，悬浮框显示当前指标值、该曲线同一时间桶的 USD 费用与缓存率；Token 与请求数模式均成立。
- **Code boundary**：新建 `apps/tests/dashboard-usage-tooltip.spec.ts`（新文件，不修改产品代码）。
- **Test seam**：现有 Playwright 静态导出运行环境和 `/api/runtime`、`/api/rpc` mock 模式。
- **RED**：
  - 新建测试文件，提供至少两个时间桶的 `dashboard/adminUsageSummary` fixture；每个桶同时含总计和一个默认选中模型的非零 `DashboardTokenUsage`，且彼此使用不同的费用、输入与缓存输入值（以便测试能区分错误配对）。
  - mock 管理员会话、初始化与网关模式，使首页渲染 `AdminDashboard`；未处理 RPC 方法应返回 500。
  - 打开首页，等待 `模型用量趋势图` 容器可见；在模型线的已知时间桶触发真实鼠标悬浮（通过 SVG `<circle>` 坐标或固定相对位置），断言悬浮框含模型名、原 Token 值、`费用` 标签、fixture 对应的美元数值、`缓存率` 标签和 fixture 对应百分比。
  - 切换到 `请求数` 模式后对同一桶重复悬浮，断言请求数值替换了 Token 值，但费用与缓存率来自同一 usage 对象（与 Token 模式值相同）。
  - 启用 `全部模型` 后悬浮总计曲线，断言总计自身的费用与缓存率，而非模型的费用与缓存率（fixture 中两者数值不同）。
  - 实施 Slice 2 前，费用与缓存率可见性断言必须失败。
- **GREEN**：实施 Slice 2 后执行 `pnpm -C apps exec playwright test tests/dashboard-usage-tooltip.spec.ts`，所有用例通过。
- **Validation**：仅检查悬浮框内文本是否与 fixture 数值匹配，不读取源代码文本，不依赖 DOM 类名以外的实现细节。
- **Dependencies**：无。
- **Rollback**：删除该新测试文件；无运行数据或环境变更。

---

## Slice 2: AC-001、AC-002 — 保留同桶 usage 并渲染费用与缓存率两项明细

- **Behavior**：图表数字路径完全保持既有行为；仅向悬浮框增加费用和缓存率两行。
- **Code boundary**：`apps/src/components/dashboard/admin-usage-trend-chart.tsx`。
- **Implementation**：
  1. 从 `@/lib/utils/billing` 导入 `formatUsdAmount` 与 `formatCacheRateValue`（若文件中尚未导入）。
  2. 在本模块定义私有 `tooltipCacheRate(usage: DashboardTokenUsage): string`：用非负 `inputTokens` 作分母，将 `cachedInputTokens` 截断到 `[0, inputTokens]`；无输入时把 `0` 交给 `formatCacheRateValue`，显示 `0%`。不得创建新共享工具。
  3. 将 `chartData` row 的值类型扩展为可保存 `DashboardTokenUsage` 对象；在已有数值键外，增加 `total_usage: point.usage`，并在模型点存在时增加 `${definition.key}_usage: usage`。不得改变 `total` 或 `model{N}` 的 `metricValue` 赋值。
  4. 将现有 `ChartTooltipContent.formatter` 从 `(value, name)` 扩展为 `(value, name, item)`；以 `item.dataKey` 拼接 `_usage` 后缀，并从 `item.payload` 读取对应 usage。保留“非总计零值模型行返回 `null`”、显示名和 `formatMetric` 调用，随后在 `usage != null` 时渲染 `费用` 与 `缓存率` 两行。
  5. 缺少 usage 时只保留既有指标行；不得补发请求或修改图表数据、排序、选择、缩放。
- **Test seam**：Slice 1 的 Playwright fixture 通过交互和 DOM 可见文本验证，而非匹配源代码文本。
- **RED**：运行 Slice 1 的新测试；费用与缓存率断言失败（产品代码尚未修改）。
- **GREEN**：完成最小代码修改后重跑同一测试，费用与缓存率断言通过；悬浮零值模型桶时，该模型仍不出现。
- **Validation**：人工复核 `chartData` 的数值键与 `_usage` 键一一对应，且没有为 `_usage` 创建任何 `Line dataKey`。
- **Dependencies**：Slice 1。
- **Rollback**：撤销本组件的 import、私有缓存率格式化、`_usage` row 字段和 formatter 明细渲染；不涉及迁移或兼容层。

---

## Slice 3: AC-003 — 聚合 API 页面移除模型日用量 UI 与查询

- **Behavior**：聚合 API 页面不再渲染"今日模型用量"卡片或展开控件；在页面初载、自动刷新、窗口聚焦、断线重连和 keep-alive 返回五种场景下均不调用 `requestlog/model_daily_usage`。按连接的聚合 API 日用量及其自动刷新、聚焦刷新、重连刷新行为不变。
- **Code boundary**：`apps/src/app/aggregate-api/page.tsx`。
- **Deletions（精确）**：
  - `ChevronDown` import（行 8）：文件中唯一使用点在今日模型用量展开按钮，删除后无其他使用。
  - `import type { ModelDailyUsageStat } from "@/types/request-log"`（行 88）：唯一使用点在 `buildModelDailyUsageTooltip` 参数类型。
  - `buildModelDailyUsageTooltip` 函数（行 119–130）：唯一调用点在模型日用量 Card 内 tooltip。
  - `const [modelDailyUsageExpanded, setModelDailyUsageExpanded] = useState(false)`（行 195）：唯一使用点在今日模型用量 Card。
  - `modelDailyUsageQuery` useQuery 块（行 269–289）：`listModelDailyUsageStats` 的唯一调用点。
  - 今日模型用量 `<Card>` JSX 块（行 688–786）：整个 Card 及其内部 CardHeader、展开按钮、Table、Skeleton 行。
- **Preserved（不得触碰）**：`dailyUsageQuery`、`buildDailyUsageTooltip`、`formatCacheRateValue`、`formatUsdAmount`、`formatMillionTokenAmount`、`Tooltip`/`TooltipContent`/`TooltipTrigger`、`Table`/`TableBody`/`TableCell` 等，以及 `accountClient.listModelDailyUsageStats` 前端方法本身。
- **RED（现有回归改写）**：
  - 修改 `apps/tests/aggregate-api-usage-refresh.spec.ts`：
    - 移除 `MODEL_USAGE` fixture 常量（行 149–161）。
    - 保留 `modelUsageCallCount` 计数器和 `requestlog/model_daily_usage` 路由分支，但将该分支响应改为 `{ items: [] }`；任一意外请求都会递增计数，供零调用断言捕获。
    - 移除 `waitForUsageResponses` 中等待 `requestlog/model_daily_usage` 的 `page.waitForResponse` Promise，使其仅等待 `requestlog/aggregate_api_daily_usage`。
    - 初载时断言 `modelUsageCallCount === 0` 且“今日模型用量”标题与展开控件均不存在；在自动刷新、窗口聚焦、断线重连、返回 keep-alive 共五个检查点持续断言零调用，同时继续断言 `requestlog/aggregate_api_daily_usage` 的调用次数递增且按连接行随 `usageVersion` 更新。
    - 移除对 `modelUsageCard`（行 300）、`modelUsageRow`（行 295–299）及相关 `modelUsageCard.getByRole("button", { name: "展开" }).click()` 交互的所有引用（行 323–324）和 `modelUsageRow` 可见性断言（行 324、356–357、372–374）。
  - 在产品代码修改之前运行更新后的测试；此时"今日模型用量"卡片仍然存在，零调用断言**失败**，证明 RED 状态成立。
- **GREEN**：删除 `page.tsx` 中的精确六处后，重跑 `apps/tests/aggregate-api-usage-refresh.spec.ts`：
  - 所有五个触发场景下 `modelUsageCallCount` 均为 0（或路由分支从未命中）。
  - `requestlog/aggregate_api_daily_usage` 调用次数在自动刷新、聚焦、重连、返回时均持续递增。
  - 今日模型用量卡片与展开控件不再出现在 DOM 中。
- **Validation**：
  - 浏览器驱动聚合 API 页面：确认今日模型用量卡片消失；上游连接表格的日用量、费用、缓存率列仍正常渲染。
  - 浏览器驱动仪表盘首页：确认模型与总计悬浮框正常显示费用与缓存率（Slice 2 已通过）。
- **Dependencies**：Slice 1（测试基础设施确认）；Slice 2 已证明仪表盘功能正常，以便隔离聚合页变更。
- **Rollback**：还原 `page.tsx` 和 `aggregate-api-usage-refresh.spec.ts` 两处变更；不需要服务重启、数据修复或接口降级。

---

## Slice 4: standard 检查与浏览器实际交互验证

- **Behavior**：全部 standard 质量门通过；仪表盘与聚合 API 页面在构建后的实际交互中行为符合预期。
- **Code boundary**：无产品代码修改，仅运行验证命令。
- **资源约束**：所有验证命令串行运行；不得并发启动 Node 测试、Playwright、lint 或 Next 构建。Playwright 固定 `--workers=1`，Node 测试固定 `--test-concurrency=1`，以降低峰值内存占用。
- **Validation 顺序**：
  1. `pnpm -C apps exec playwright test --workers=1 tests/dashboard-usage-tooltip.spec.ts tests/aggregate-api-usage-refresh.spec.ts` — 两个浏览器回归在同一静态服务器、单 worker 中串行执行。
  2. `pnpm -C apps exec node --test --test-concurrency=1` — 运行时测试串行执行。
  3. `pnpm -C apps run lint` — 确认 TypeScript strict + ESLint 通过，且无 unused import。
  4. `pnpm -C apps run build:desktop` — 静态导出构建成功；仅在前序命令退出后启动。
  5. 浏览器驱动构建后的仪表盘首页：悬浮模型曲线与总计曲线的非零桶，目视确认费用（美元）与缓存率（百分比）出现在悬浮框中；悬浮零值桶不出现额外行。
  6. 浏览器驱动构建后的聚合 API 页面：确认“今日模型用量”卡片不存在；上游连接表格的日用量列与 tooltip 正常工作。
- **Dependencies**：Slice 1–3 全部完成。
- **Rollback**：恢复这四个文件在本任务开始前的已验证内容；不需要服务重启、数据修复或接口降级。

---

## AC Coverage

| Acceptance Criterion | Delivery Slices | Observable Proof |
|---|---|---|
| AC-001 模型曲线悬浮详情 | 1、2 | Playwright 在 Token 与请求数模式悬浮模型线，验证指标、费用和缓存率文本与 fixture 匹配 |
| AC-002 总计曲线悬浮详情 | 1、2 | Playwright 启用总计线后悬浮，验证总计自身的指标、费用和缓存率（与模型值不同） |
| AC-003 聚合 API 页面移除模型日用量 | 3、4 | 更新后的 Playwright 回归以 RPC 计数证明初载、刷新、聚焦、重连、返回 5 个场景零调用；同时断言聚合 API 日用量持续刷新 |

---

## Expected Change Set

| File | Change |
|---|---|
| `apps/src/components/dashboard/admin-usage-trend-chart.tsx` | 保留每条曲线、每个时间桶的既有 usage 引用，并在 tooltip 显示费用与缓存率 |
| `apps/tests/dashboard-usage-tooltip.spec.ts` | 新建：Mock 仪表盘 RPC 的端到端悬浮交互回归，覆盖模型、总计、Token 与请求数模式 |
| `apps/src/app/aggregate-api/page.tsx` | 删除 `modelDailyUsageQuery`、`modelDailyUsageExpanded` state、`buildModelDailyUsageTooltip`、`ModelDailyUsageStat` import、`ChevronDown` import（仅用于该卡片）及完整今日模型用量 Card JSX |
| `apps/tests/aggregate-api-usage-refresh.spec.ts` | 更新：移除 MODEL_USAGE fixture 与模型用量等待逻辑，补充 5 个触发场景的零调用 `expect` 断言，同时保留聚合 API 日用量持续刷新断言 |

---

## Risks

- **Recharts 悬浮坐标不稳定**：fixture 至少提供两个稀疏时间桶，以 SVG `<circle>` 的 `cx`/`cy` 属性定位悬浮点，并以 tooltip 中唯一 fixture 数值断言；若布局变化，优先调整定位策略，不改生产组件添加测试专用标记。
- **不同曲线的 usage 被错配**：以 `item.dataKey` 构建 `_usage` 键；模型和总计 fixture 使用不同费用与缓存率，测试可检测错误回退到任意同桶对象。
- **零输入语义漂移**：统一显示 `0%`，与服务端既有 `cache_hit_rate` 行为一致。
- **聚合页删除残留 unused import**：移除 6 处后，TypeScript strict + lint（Slice 4 步骤 4）直接证明无残留；不依赖源码文本匹配作为行为证明。
- **回滚风险**：无持久化、API 或依赖变化；4 个文件的 `git revert` 即可完全恢复原始行为。
