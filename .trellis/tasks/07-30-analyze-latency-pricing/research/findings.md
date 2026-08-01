# 请求日志延迟与 GPT-5.6 长上下文计费核查

## 结论

- 请求日志已持久化 `first_response_ms`，页面当前只以主色文本展示 `duration / firstResponse / outputRate`；不存在基于首响的颜色语义。
- 颜色标记可纯前端实现，不需新增 RPC、数据库字段或迁移。当前运行库 24 小时内有 944/980 条日志保存了首响，分布足以支撑分级：≤2 秒 245、2–5 秒 346、5–10 秒 121、>10 秒 240；36 条缺失。
- GPT-5.6 的当前运行时数据库已启用官方长上下文档位：输入达到 272,000 后，Sol/Terra/Luna 的输入与缓存价为标准价 2×，输出价为标准价 1.5×。
- 截图中的上游 `$0.34` 并非未翻倍；它与 GPT-5.6 Sol 的长上下文公式一致。截图中本地 `$0.196738` 则是相同结构按标准档位的结果；但当前数据库对应记录已保存 `$0.378086` 长上下文费用。因此该截图不能代表当前持久化计费结果，需将其归因于旧前端/旧服务响应或另一数据库实例，不能据此断言当前系统仍未翻倍。

## 首响链路

1. `crates/service/src/gateway/http_bridge/stream_readers/common.rs` 在首次上游流事件到达时只写入一次 `first_response_ms`。
2. `crates/service/src/gateway/observability/request_log.rs` 将它写入 `request_logs.first_response_ms`。
3. `apps/src/lib/api/normalize.ts` 映射为 `RequestLog.firstResponseMs`；`apps/src/app/logs/page-sections.tsx` 以 `formatDuration(log.firstResponseMs)` 显示。
4. UI tooltip 已定义为“从请求开始到首个上游响应片段的耗时”。这不是首个可见文字 token 的严格语义，命名和提示应继续使用“首响”。

### 建议显示规则

| 首响 | 颜色 | 语义 |
| --- | --- | --- |
| 缺失 | muted | 非流式、旧日志或异常路径，不评价快慢 |
| ≤2s | emerald | 快 |
| >2–5s | sky | 正常 |
| >5–10s | amber | 偏慢 |
| >10s | rose | 慢 |

推荐在首响数值左侧增加一条细色条，并让首响数值同色；总耗时和输出速率保持现有中性色，避免把总耗时误标为首响性能。

## 长上下文计费证据

### 当前官方档位

`crates/core/migrations/121_model_catalog_gpt56_official_prices.sql` 和运行时 `model_price_tiers` 都包含：

| 模型 | 标准 输入 / 缓存 / 输出（$/M） | ≥272K 输入 / 缓存 / 输出（$/M） |
| --- | --- | --- |
| gpt-5.6-sol | 5 / 0.5 / 30 | 10 / 1 / 45 |
| gpt-5.6-terra | 2.5 / 0.25 / 15 | 5 / 0.5 / 22.5 |
| gpt-5.6-luna | 1 / 0.1 / 6 | 2 / 0.2 / 9 |

`select_model_price_tier_v2` 选择 `min_input_tokens <= input_tokens` 的最高档位，边界 272,000 包含在长上下文档位。`compute_charge_v2` 以 `未缓存输入 = input - cached`，再加缓存输入和输出，最终才应用可能的商业倍率；默认倍率为 1.0。

### 对照截图的可复算结果

- 上游截图：Sol，输入 319,529、缓存 317,952、输出 213。
  - 未缓存输入：1,577。
  - 长上下文：`(1,577×10 + 317,952×1 + 213×45) / 1,000,000 = $0.343307`，显示 `$0.34`。
  - 标准档位才会是 `$0.173251`。
- 本地截图中显示 `$0.196738` 的 Sol 行对应输入 320,270、缓存 318,976、输出 1,026。
  - 标准档位：`$0.196738`。
  - 长上下文：`$0.378086`。
  - 当前数据库的请求 `id=51819`、创建于 2026-07-30 11:20:00，`request_charge_snapshots.tier_min_input_tokens=272000`，已保存 `$0.378086`；与长档位精确一致。

运行时数据库的官方价格迁移 `121_model_catalog_gpt56_official_prices` 已于 2026-07-22 16:46:48 应用；`gpt56_pricing_revision=2026-07-20-official`。

## 风险与验证

- 不应依据首响缺失值给请求染红；缺失必须使用中性色。
- 首响目前在首个上游帧上计时，协议 keepalive 或控制帧的语义与“首个可见文本”不同。现有测试覆盖 keepalive 不作为首个上游响应；新 UI 测试应覆盖四档阈值与缺失值。
- 对费用问题，先确认截图所属服务实例及响应时间；若当前 UI 仍显示标准档位费用，应直接查询该实例的 `request_charge_snapshots`，不能只看前端列表缓存。

## 外部依据

- OpenAI GPT-5 开发者公告：最大输入长度为 272,000；后续价格以模型比较页为准。https://openai.com/index/introducing-gpt-5-for-developers/
- 仓库将 GPT-5.6 官方价格来源记录为：https://developers.openai.com/api/docs/models/compare （自动读取返回 403；价格已由 `121_model_catalog_gpt56_official_prices.sql` 固化并有运行库实测验证。）
