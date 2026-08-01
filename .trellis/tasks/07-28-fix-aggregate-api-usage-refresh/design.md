# 技术设计：聚合 API 今日用量刷新

## 边界

本次修复限定在聚合 API 页面查询生命周期。数据库、Core 聚合 SQL、Service RPC 和前端 payload normalizer 不改动；运行时证据已确认这些层能返回当前数据。

## 数据流

`request_token_stats` → Core 按聚合 API 汇总 → Service RPC `requestlog/aggregate_api_daily_usage` → `accountClient.listAggregateApiDailyUsageStats` → TanStack Query → 顶部汇总卡与列表行。

顶部汇总卡和列表继续共享同一个 `dailyUsageQuery`，避免产生两个刷新节奏不同的数据源。模型日统计继续使用独立查询，但采用相同的激活/恢复策略。

## 查询策略

- 仅在服务已连接且 `/aggregate-api` 是当前 shell 页面时启用。
- 激活页面后，不信任旧缓存：查询视为立即可刷新。
- 激活期间维持 5 秒轮询。
- 覆盖全局关闭的 mount/focus/reconnect 行为；页面重新进入、窗口恢复焦点、服务重连时立即重取。
- 处理 Tauri keep-alive/WebView 可见性导致的轮询暂停：当前页面允许 TanStack 的后台定时器继续执行；非当前页面仍由 `enabled`/interval 条件关闭，因此不会让所有 keep-alive 页面持续轮询。
- 保留最后一次成功数据，避免刷新期间闪空；请求失败不得清空已有统计。

## UI 兼容

不调整表格、卡片、路由或 RPC contract。右上角“刷新余额”仍只代表供应商余额，不借此混入用量刷新语义。

## 验证设计

新增 Playwright 回归：模拟 Web RPC 首次返回旧统计、后续返回新统计，验证不重新加载页面时顶部费用和对应列表行在轮询窗口内一起更新；再切换离开/返回聚合 API 页，验证缓存会立即重取。

运行前端 runtime tests、目标 Playwright 用例和 desktop build。最后以当前服务 RPC 返回值作为后端健康对照。

## 风险与回滚

- 风险：窗口最小化时当前页面可能继续产生两个 5 秒查询。影响仅为本地 SQLite 只读聚合；非当前页面不查询。
- 回滚：恢复原查询选项即可；无 schema、RPC 或数据迁移。
