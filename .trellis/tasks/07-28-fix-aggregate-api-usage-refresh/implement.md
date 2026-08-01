# 实施计划：聚合 API 今日用量刷新

## 实施步骤

1. 在 `apps/src/app/aggregate-api/page.tsx` 统一加固 `dailyUsageQuery` 与 `modelDailyUsageQuery`：
   - 仅激活页轮询；
   - 激活、重新聚焦、重连时立即重取；
   - 避免 Tauri keep-alive/WebView 可见性状态永久压停轮询；
   - 保持 5 秒刷新目标和旧数据占位行为。
2. 新增 `apps/tests/aggregate-api-usage-refresh.spec.ts`：
   - mock runtime 与 RPC；
   - 首次返回旧聚合统计，后续返回新统计；
   - 断言顶部费用和列表今日用量无需整页 reload 即更新；
   - 断言离开再返回页面时主动重取。
3. 审查查询次数，确保只有当前聚合 API 页面执行周期查询，未激活 keep-alive 页面不持续请求。

## 验证命令

- `pnpm -C apps exec playwright test tests/aggregate-api-usage-refresh.spec.ts`
- `pnpm -C apps run test:runtime`
- `pnpm -C apps run build:desktop`
- 直接调用本地 `requestlog/aggregate_api_daily_usage` RPC，确认服务端当前值与刷新后的页面一致。

## 风险点

- `apps/src/app/aggregate-api/page.tsx` 是大型页面，改动限定在两个 query 配置，不扩散业务编排。
- Playwright RPC mock 需覆盖页面初始化所需的方法，未处理的方法必须显式失败，防止假阳性。
- 不修改 Core/Service；若前端回归仍无法稳定复现，则回到调查阶段检查运行 bundle 与 Tauri WebView visibility/focus 事件。

## 回滚点

前端 query 选项和新增测试可整体回滚；没有持久化数据或兼容性迁移。
