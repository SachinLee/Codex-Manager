# Design: 仪表盘用量趋势图悬浮明细 + 聚合 API 页面模型日用量移除

## 上下文与当前行为

### 仪表盘（AdminUsageTrendChart）

`AdminUsageTrendChart` 的 `chartData` useMemo 在将每个时间桶数据转换为 Recharts
行时，通过 `metricValue(usage, metric)` 提取单一数值并赋给曲线键（如 `total`、
`model0`）。原始 `DashboardTokenUsage` 对象在此处被丢弃，导致
`ChartTooltipContent.formatter` 只能看到数值，无法读取 `estimatedCostUsd`、
`inputTokens`、`cachedInputTokens`。

**已有数据链路确认**（均无需后端变更）：
- `DashboardTokenUsage` 接口：`apps/src/types/dashboard.ts:4-15`，已含
  `estimatedCostUsd`、`inputTokens`、`cachedInputTokens`。
- 模型序列：`DashboardModelUsageSeries.points[i].usage: DashboardTokenUsage`（第 29-33 行）。
- 总计序列：`DashboardUsageSeriesPoint.usage: DashboardTokenUsage`（第 23-27 行）。
- 服务端 `DashboardTokenUsageResult` 已填充上述字段并经现有 RPC 传输；前端
  `DashboardAdminUsageSummary` 解码后保留完整 usage 对象，存于内存 `summary` 中。

### 聚合 API 页面（aggregate-api/page.tsx）

`AggregateApiPage` 当前在 `dailyUsageQuery`（`requestlog/aggregate_api_daily_usage`，
按连接聚合 API 日用量）之外，还独立发起 `modelDailyUsageQuery`
（`requestlog/model_daily_usage`），在页面顶部渲染"今日模型用量"卡片，提供按模型的
展开表格与每行 tooltip。此卡片造成模型级用量入口重复，且在页面活跃、自动刷新、
窗口聚焦、断线重连和 keep-alive 返回时均会发起 RPC。

## 变更边界

仅修改以下四个文件：

| 文件 | 变更类型 |
|---|---|
| `apps/src/components/dashboard/admin-usage-trend-chart.tsx` | 新增 tooltip 费用与缓存率（AC-001/AC-002） |
| `apps/tests/dashboard-usage-tooltip.spec.ts` | 新建：仪表盘悬浮回归（AC-001/AC-002） |
| `apps/src/app/aggregate-api/page.tsx` | 删除模型日用量 query、state、函数、import 与 card（AC-003） |
| `apps/tests/aggregate-api-usage-refresh.spec.ts` | 更新：零调用断言替换原有 model usage 断言（AC-003） |

不改动：后端 RPC、服务端类型、数据库、`requestlog/model_daily_usage` 后端能力、
`accountClient.listModelDailyUsageStats` 前端客户端方法、`chart.tsx`、路由配置、
仪表盘类型或任何其他文件。

## 仪表盘 Tooltip 方案与组件职责

### 核心机制：`_usage` 后缀键约定

在 `chartData` row 上为每条曲线追加一个与数值键同名但加 `_usage` 后缀的键，
存储原始 `DashboardTokenUsage`：

| 曲线 | 数值键（原有） | usage 键（新增） |
|---|---|---|
| 总计曲线 | `total` | `total_usage` |
| 模型曲线 N | `model{N}` | `model{N}_usage` |

Recharts 仅渲染与 `<Line dataKey>` 精确匹配的字段，`_usage` 后缀键不声明为任何
`Line` 的 `dataKey`，故不参与图表绘制，不影响任何曲线数值。

### `chartData` useMemo（变更点 1）

```ts
// 行类型从 Record<string, number | string> 扩展：
const row: Record<string, number | string | DashboardTokenUsage> = {
  bucketStartTs: point.bucketStartTs,
  label,
  name: label,
  total: metricValue(point.usage, metric),
  total_usage: point.usage,              // 新增：总计曲线 usage
};
for (const definition of modelDefinitions) {
  const usage = modelPointMaps.get(definition.model)?.get(point.bucketStartTs);
  row[definition.key] = usage ? metricValue(usage, metric) : 0;  // 不变
  if (usage) {
    row[`${definition.key}_usage`] = usage;                       // 新增：模型 usage
  }
}
```

`metricValue` 调用路径、零值处理、模型排序均不变。

### `tooltipCacheRate`（变更点 2 — 模块内私有函数）

```ts
function tooltipCacheRate(usage: DashboardTokenUsage): string {
  const input = Math.max(0, usage.inputTokens);
  if (input <= 0) return formatCacheRateValue(0); // "0%"，满足 PRD 无输入为 0%
  return formatCacheRateValue(
    Math.min(Math.max(0, usage.cachedInputTokens), input) / input,
  );
}
```

复用 `formatCacheRateValue`（`@/lib/utils/billing`），不创建新的通用格式化工具。
此为模块内计算辅助函数，不是通用格式化工具。

### `formatter`（变更点 3）

`ChartTooltipContent.formatter` 在 `chart.tsx:244` 以
`formatter(item.value, item.name, item, index, item.payload)` 调用，
故第三参数 `item` 携带 `item.dataKey`（`"total"` 或 `"model{N}"`）
及 `item.payload`（完整 chartData row）。

```tsx
formatter={(value, name, item) => {
  if (Number(value) === 0 && String(name) !== "total") return null; // 不变
  const usageKey = `${String(item.dataKey)}_usage`;
  const usage = item.payload[usageKey] as DashboardTokenUsage | undefined;
  const displayName = String(name) === "total" ? t("全部模型") : String(name);
  return (
    <div className="flex min-w-40 flex-col gap-0.5">
      <div className="flex items-center justify-between gap-4">
        <span className="truncate text-muted-foreground">{displayName}</span>
        <span className="font-mono font-medium text-foreground">
          {formatMetric(Number(value))}
        </span>
      </div>
      {usage != null && (
        <div className="flex items-center justify-between gap-4 text-[10px] text-muted-foreground">
          <span>{t("费用")}</span>
          <span className="font-mono">{formatUsdAmount(usage.estimatedCostUsd)}</span>
        </div>
      )}
      {usage != null && (
        <div className="flex items-center justify-between gap-4 text-[10px] text-muted-foreground">
          <span>{t("缓存率")}</span>
          <span className="font-mono">{tooltipCacheRate(usage)}</span>
        </div>
      )}
    </div>
  );
}}
```

现有行为不变点：非 total 零值行返回 `null`；`displayName` 逻辑不变；
`formatMetric(Number(value))` 不变。

## 聚合 API 页面清理边界

### 删除的局部依赖（`apps/src/app/aggregate-api/page.tsx`）

| 类型 | 标识符 | 行号 | 说明 |
|---|---|---|---|
| Import | `ChevronDown` | 8 | 仅用于展开按钮 `<ChevronDown className="...">`（行 713）；文件中无其他使用 |
| Import | `ModelDailyUsageStat` | 88 | 仅供 `buildModelDailyUsageTooltip` 参数类型使用 |
| 模块函数 | `buildModelDailyUsageTooltip` | 119–130 | 仅用于模型日用量表格 cell tooltip |
| useState | `modelDailyUsageExpanded` / setter | 195 | 仅控制模型日用量卡片展开/收起状态 |
| useQuery | `modelDailyUsageQuery` | 269–289 | `accountClient.listModelDailyUsageStats` 的唯一调用点 |
| JSX Card | 今日模型用量卡片 | 688–786 | 整个 `<Card>` 块，含卡头、展开按钮、Table、Skeleton |

### 保留功能（不受影响）

| 功能 | 保留原因 |
|---|---|
| `dailyUsageQuery`（`requestlog/aggregate_api_daily_usage`） | 按连接日用量核心功能，含 5 秒自动刷新、聚焦/重连刷新 |
| `buildDailyUsageTooltip` | 为上游连接表格每行构建详细 tooltip |
| `formatCacheRateValue`、`formatUsdAmount`、`formatMillionTokenAmount` | 仍用于 `buildDailyUsageTooltip` 和连接行展示 |
| `Tooltip`、`TooltipContent`、`TooltipTrigger` | 在上游连接表格行中仍有使用 |
| `Table`、`TableBody`、`TableCell` 等 | 上游连接表格仍使用 |
| `Skeleton` | 上游连接表格加载态仍使用 |
| `accountClient.listModelDailyUsageStats` | 前端客户端方法本身不删除；仅删除此页面的调用点 |
| 后端 `requestlog/model_daily_usage` 能力 | 严格超出本任务范围，不得删除 |

### 无请求不变量

删除 `modelDailyUsageQuery` 后，以下所有触发条件均不再产生
`requestlog/model_daily_usage` RPC 调用：

- 页面初次挂载（`refetchOnMount: "always"` 随查询一并移除）
- 5 秒自动刷新（`refetchInterval: 5_000` 随查询一并移除）
- 窗口聚焦（`refetchOnWindowFocus: "always"` 随查询一并移除）
- 网络重连（`refetchOnReconnect: "always"` 随查询一并移除）
- keep-alive 返回激活（`isQueryEnabled` 驱动的 `enabled` 随查询一并移除）

`requestlog/aggregate_api_daily_usage`（`dailyUsageQuery`）的完全相同刷新配置保持不变。

## 数据流（仪表盘）

```
DashboardAdminUsageSummary.modelUsage[i].points[j].usage  (DashboardTokenUsage 已在内存)
        │
        ▼ chartData useMemo
row = {
  total:          metricValue(point.usage, metric),    // 原有
  total_usage:    point.usage,                         // 新增
  model0:         metricValue(usage, metric),          // 原有
  model0_usage:   usage,                               // 新增（有数据时）
  ...
}
        │
        ▼ Recharts ComposedChart data prop (any[])
ChartTooltipContent.formatter(value, name, item)
  usageKey = `${item.dataKey}_usage`
  usage    = item.payload[usageKey] as DashboardTokenUsage | undefined
        │
        ▼ 渲染
指标值行（已有）
费用行：formatUsdAmount(usage.estimatedCostUsd)         // 新增
缓存率行：tooltipCacheRate(usage)                       // 新增
```

## 格式化与零值不变量

| 值 | 格式化函数 | 零/空值行为 |
|---|---|---|
| `estimatedCostUsd` | `formatUsdAmount(v)` | `0` → `"$0.00"`；非有限数 → `"-"` |
| 缓存率 | `tooltipCacheRate(usage)` via `formatCacheRateValue(r)` | `inputTokens <= 0` → `"0%"`；`rate === 0` → `"0%"` |
| 指标值 | `formatMetric(Number(value))` | 不变 |
| 零值模型行 | 现有 `Number(value) === 0` 判断 → `null` | usage 路径不可达，行为不变 |

## 安全与隐私

无新增网络请求、RPC 调用或字段传输。仪表盘 tooltip 数据来自已在内存的 `summary`
对象。聚合 API 页面的变更删除一条现有查询，减少 RPC 调用量。

## 无后端变更依据

仪表盘：`DashboardTokenUsage` 已由服务端填充并传输，前端 `summary` 对象保留所有字段。
`chartData` useMemo 仅将已有内存引用附加到 row，不触发任何新查询。

聚合 API：`requestlog/model_daily_usage` 后端能力与 `accountClient.listModelDailyUsageStats`
前端方法均不删除。本任务仅删除 `aggregate-api/page.tsx` 中的调用点，使其他调用方
（现有或未来）不受影响。

## 兼容性

- `AdminUsageTrendChart` 的 props 接口不变。
- `chartData` 元素类型扩展为 `Record<string, number | string | DashboardTokenUsage>`；
  Recharts `ComposedChart data` prop 类型为 `any[]`，无类型冲突。
- `formatter` 由两参数扩展为三参数：`(value, name)` → `(value, name, item)`；
  TypeScript 从 `ChartTooltipContent` 的 `formatter` prop 类型推断，无需显式注解。
- `aggregate-api/page.tsx` 移除后，`formatCacheRateValue`、`Tooltip`、`Table`、
  `Skeleton` 等保留导入均仍有使用，无 unused import 报错。

## 风险与回滚

| 风险 | 级别 | 缓解 |
|---|---|---|
| DashboardTokenUsage 对象混入 chartData 导致 Recharts 异常 | 低 | Recharts 仅使用与 `<Line dataKey>` 精确匹配的键；`_usage` 键未声明为任何 dataKey |
| TypeScript strict 模式类型报错（仪表盘） | 低 | `Record<string, number \| string \| DashboardTokenUsage>` 覆盖所有赋值和读取场景 |
| 无数据时 `usage` 为 `undefined` | 无 | formatter 检查 `usage != null`；零值模型行由已有判断屏蔽 |
| 移除 `ChevronDown` / `ModelDailyUsageStat` 后 unused import 报错 | 低 | 两者的唯一使用点均在今日模型用量 Card 内；Card 与函数一并删除后无残留引用 |
| 聚合 API 其他功能受误删影响 | 无 | 删除范围精确到 6 个标识符与 1 个 JSX Card；所有保留 import 仍有使用 |

**回滚**：
- 仪表盘：恢复本任务前已验证的组件与 Playwright 测试内容；无数据迁移或接口变更。
- 聚合 API：恢复页面与刷新回归在本任务前已验证的内容；无数据迁移或接口变更。

## 替代方案与拒绝原因

| 方案 | 拒绝原因 |
|---|---|
| 在 formatter 闭包中通过 `summary` + `bucketStartTs` 二次查找 | 需将 `summary` 引入 formatter 闭包；row 已持有数据，查找冗余 |
| 从显示值（`formatCompactTokenAmount` 输出）反推统计 | PRD 明确禁止；有损且不可靠 |
| 扩展 RPC 字段或新增 API | PRD 明确禁止；现有字段已足够 |
| 在 row 中仅存 `bucketStartTs` 并维护独立 tooltip Map | 增加额外 state 与同步点；直接存储对象更简洁 |
| 保留模型日用量卡片但隐藏（display:none） | 不满足零 RPC 不变量；查询仍会发起 |
| 将 `requestlog/model_daily_usage` 后端能力一并删除 | PRD 明确超出范围；破坏其他潜在调用方 |

## AC 覆盖映射

| AC | 覆盖机制 |
|---|---|
| AC-001: 模型曲线悬浮明细 | `chartData` 新增 `model{N}_usage`；`formatter` 读取并渲染费用与缓存率 |
| AC-002: 总计曲线悬浮明细 | `chartData` 新增 `total_usage`；同一 `formatter` 路径覆盖 |
| AC-003: 聚合 API 页面移除模型日用量 | 精确删除 `modelDailyUsageQuery`、展开 state、`buildModelDailyUsageTooltip`、`ModelDailyUsageStat` import、`ChevronDown` import 及完整 today-model-usage Card；更新 Playwright 回归，以 RPC 计数证明初载、刷新、聚焦、重连、返回 5 个触发场景均为零调用 |
