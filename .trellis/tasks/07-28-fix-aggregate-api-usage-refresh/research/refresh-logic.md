# 聚合 API 今日用量刷新调查

## 现象

用户截图中，聚合 API 页顶部“今日费用”和列表“今日用量”停留在约 `$19.27 / 19M`，而同页模型统计已显示更高用量。

## 当前刷新链路

1. `apps/src/app/aggregate-api/page.tsx` 使用 `dailyUsageQuery` 请求 `requestlog/aggregate_api_daily_usage`。
2. 顶部“今日 Token / 今日费用 / 平均缓存率”和列表每个上游的“今日用量”共用这一份查询结果，因此它们会一起停滞。
3. 查询当前配置：`staleTime: 10_000`、页面激活时每 5 秒轮询、`refetchIntervalInBackground: false`。
4. 全局 QueryClient 同时关闭 `refetchOnWindowFocus`、`refetchOnReconnect`、`refetchOnMount`。聚合页没有覆盖这些恢复触发器。
5. 页面右上角“刷新余额”只刷新供应商余额及 `aggregate-apis` 查询，不刷新 `dailyUsageQuery` 或模型日统计。

## 运行时证据（2026-07-28）

- 直接读取当前运行数据库的今日 `request_token_stats`：聚合 API 已归属 383 条请求、约 47.89 美元、47,668,873 Token。
- 直接调用当前服务 `requestlog/aggregate_api_daily_usage` RPC：
  - `input`：321 请求，39,795,316 计费用量（含 Guard），约 34.41 美元；
  - `esfaery`：59 请求，7,730,869 Token，约 12.38 美元；
  - `timcc-0.35`：3 请求，403,947 Token，约 1.57 美元。
- RPC 返回值明显高于截图的 `$19.27 / 19M`，说明数据库写入、后端聚合 SQL、RPC 序列化均能返回新数据。

## 结论

根因边界在前端查询生命周期，而不是统计入库或后端聚合。当前轮询依赖 TanStack Query 的页面/文档活跃判断；在 Tauri keep-alive 页面或 WebView 可见性状态切换后，一旦轮询被暂停，页面没有 mount/focus/reconnect 恢复策略，也没有手动刷新用量入口，旧的成功数据会持续显示。

具体触发条件尚不能从静态代码直接证明；但后端 RPC 与截图数据的差异已排除“后端数据仍是 19”的解释。修复应让激活页立即重取、稳定轮询，并在恢复焦点/连接时重取，同时增加端到端回归验证。

## 补充调查（2026-07-29）

当前运行服务的 `requestlog/aggregate_api_daily_usage` 连续两次返回空 `items`，同时 `requestlog/model_daily_usage` 返回真实模型用量。SQLite 中当天 135 条 `actual_source_kind = 'aggregate_api'` 的 token 记录均有正确的 `actual_source_id`，但 `aggregate_api_id` 全为 NULL。

根因位于 `Storage::insert_request_log_with_token_stat`：事务 SQL 漏掉了 `aggregate_api_id`、供应商名和 URL 三个字段。启动期间的历史回填会暂时补齐旧行，导致重启后看似恢复；后续新请求仍会丢失归属，直到下次重启再次回填。

修复必须同时：

1. 在事务插入中写入聚合 API 归属字段。
2. 汇总查询在旧行缺失 `aggregate_api_id` 时，以 `actual_source_kind = 'aggregate_api'` 和 `actual_source_id` 回退。
3. 用真实 Storage 回归测试覆盖写入和回退；前端 mock RPC 测试只能覆盖轮询生命周期，不能覆盖此持久化边界。

## 相关文件

- `apps/src/app/aggregate-api/page.tsx`
- `apps/src/components/providers.tsx`
- `apps/src/hooks/useDesktopPageActive.ts`
- `crates/service/src/requestlog/requestlog_aggregate_api_daily_usage.rs`
- `crates/core/src/storage/request_token_stats.rs`
