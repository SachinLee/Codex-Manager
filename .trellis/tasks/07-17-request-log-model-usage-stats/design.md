# Design: 请求日志模型使用统计

## Summary

在请求日志筛选汇总链路上增加「按模型 GROUP BY」聚合，经 `summary.modelStats` 返回前端；UI 以方案 A 全宽可折叠表格展示，与现有 glass-card 风格一致。

## Boundaries

| 层 | 职责 |
|----|------|
| `crates/core` storage | `summarize_request_logs_by_model_filtered` / `_for_keys`，复用 `build_request_log_filters` |
| `crates/core` rpc types | `RequestLogModelUsageStat` + `RequestLogFilterSummaryResult.model_stats` |
| `crates/service` | `requestlog_summary` / `list_with_summary` 映射；权限分 admin / key_ids |
| Tauri + web transport | 命令名与 RPC 保持既有 requestlog 前缀；字段透传 |
| `apps` | types / normalize / service-client；logs 页新组件 |

**不修改**：Dashboard `topModels`、Quota `modelUsage`、计费引擎。

## Data Model / Contracts

### 聚合维度

```
model_key = COALESCE(NULLIF(TRIM(t.model), ''), NULLIF(TRIM(r.model), ''), '(unknown)')
```

### 新增类型（camelCase JSON）

```ts
interface RequestLogModelUsageStat {
  model: string;
  requestCount: number;
  successCount: number;
  errorCount: number;
  totalTokens: number;
  estimatedCostUsd: number;
  inputTokens: number;
  cachedInputTokens: number;
  outputTokens: number;
  reasoningOutputTokens: number;
}

// RequestLogFilterSummary 扩展
modelStats: RequestLogModelUsageStat[];
modelStatsTruncated: boolean;
```

### Token / 费用口径

- 与 `summarize_request_logs_filtered` 中单条 total_tokens / estimated_cost_usd 表达式一致（`request_token_stats` LEFT JOIN）。
- **Guard 重试（v1）**：默认**不按模型分摊**；总汇总卡仍可显示 Guard 附加量。模型表标题或脚注注明「不含 Guard 重试费用/Token」。二期可按 trace 挂到主请求模型。
- 成功：`status_code` 在 200–299。
- 异常：`status_code >= 400` 或 error 非空（与 filter summary error 定义对齐）。
- 缓存率：`cached_input_tokens / input_tokens`（input=0 → null/-）。

### 排序与截断

- SQL：`ORDER BY estimated_cost_usd DESC, total_tokens DESC, model ASC`
- `LIMIT 51`；若 >50，保留前 50，`modelStatsTruncated=true`（可选：第 51+ 合并为「其他」——v1 采用 truncated 标记即可，实现时优先简单路径）。

### API 策略

- **扩展**现有 `requestlog/summary` 与 `requestlog/list_with_summary` 的 `summary` 对象，避免额外往返。
- 向后兼容：旧字段保留；新字段缺省空数组 / false。

## Data Flow

```
UI filters (query, status, startTs, endTs)
  → service_requestlog_list_with_summary / summary
  → storage.summarize_request_logs_filtered (existing totals)
  → storage.summarize_request_logs_by_model_filtered (new)
  → summary.modelStats[]
  → ModelUsageStatsCard
```

## UI Design (方案 A)

### 布局

```
[筛选栏]
[汇总卡 row]
[模型使用统计 card]  ← NEW
[请求明细 table]
```

### 组件拆分

- `apps/src/app/logs/model-usage-stats.tsx`（新建）
  - `ModelUsageStatsCard`
  - 折叠、排序、占比条、空态、skeleton
- `page-sections.tsx` 仅挂载组件，避免继续膨胀

### 交互

| 行为 | 说明 |
|------|------|
| 默认展开 | 是；>8 模型时默认显示 Top 8 +「展开全部」 |
| 排序 | 客户端对已返回的 modelStats 再排序（请求/Token/费用） |
| 占比 | costShare = cost / sum(cost)；sum=0 时用 tokenShare |
| 点击行 | 可选：`onModelClick(model)` → 父级 setSearch + page=1 |
| truncated | 底部提示「仅展示费用最高的 50 个模型」 |

### i18n keys（中文源）

- 模型使用统计
- 当前筛选下暂无模型用量
- 费用占比
- 展开全部 / 收起
- 不含 Guard 重试用量（脚注）
- 仅展示费用最高的 50 个模型

## Tradeoffs

| 选项 | 结论 |
|------|------|
| 扩展 summary vs 独立 RPC | 扩展 summary：与列表同筛、实现更简 |
| request_logs 聚合 vs token_stats only | request_logs：对齐 status/search 与清空日志语义 |
| Guard 进模型表 | v1 不做，避免归属歧义 |
| 服务端排序 vs 客户端 | 服务端默认费用序；客户端可改展示序 |

## Compatibility / Rollout

- 纯增量字段，无需 migration。
- Desktop + service-mode Web 均经现有 transport。
- 无需 feature flag。

## Rollback

- 回退 PR：前端不渲染 modelStats；后端字段可保留无害。
- 若性能问题：可临时 LIMIT 更小或仅时间窗非 all 时计算（不作为默认）。

## Testing Strategy

- core：多模型、status、key 隔离、unknown model、truncated
- service：RPC 字段 camelCase、admin/member
- 前端：normalize 缺省；手动验证筛选联动
