# 执行计划：API 模型发现与路由配置

## 实施顺序

1. **冻结 RPC 数据合同**
   - 在 `crates/core/src/rpc/types.rs` 增加 discovery item/result。
   - 明确 `ok/items/statusCode/discoveredAt/message` 的空值和错误语义。
2. **实现服务端发现核心**
   - 在 `crates/service/src/aggregate_api.rs` 增加模型目录 URL 构造、有限响应读取、常见目录解析、去重和安全错误归一化。
   - 复用已保存 secret、认证注入和 aggregate API upstream client；不写 storage。
3. **接通 RPC 与桌面壳**
   - 更新 `crates/service/src/rpc_dispatch/aggregate_api.rs`。
   - 更新 `apps/src-tauri/src/commands/aggregate_api.rs` 和 `apps/src-tauri/src/registry.rs`。
4. **接通前端 typed transport**
   - 更新 `apps/src/lib/api/transport-web-commands/aggregate-api.ts`、`apps/src/lib/api/account-client.ts`、`apps/src/types/api-key.ts`。
   - 使用现有 `invoke` / `withAddr`，不引入 raw fetch 或旁路 Web 请求。
5. **接入模型与路由页面的 API 模型面板**
   - 在 `apps/src/app/models/page.tsx` 增加按 `aggregateApiId` 索引的内存状态、单 API 刷新和“获取全部 API 模型”。
   - 每个 API 单独展示供应商名称、API ID、provider 类型、脱敏 URL、状态、时间、数量和模型列表；同名模型不得跨 API 合并。
   - 批量调用逐 API 隔离 loading/error/result，保留已完成结果。
   - 查询与展示只读，不写入模型编辑草稿、路由或任何持久化层。
6. **补充验证覆盖**
   - Rust：解析 `data[]` / `models[]` / 根数组、去重、缺失 ID、空目录、非 JSON、401/403/404、认证头、路径规范化、超时和不落库。
   - RPC/transport：命令注册和 Web RPC 映射测试。
   - 前端：多个 API 的结果按 API ID 分组、同名模型来源可区分、单 API 失败不影响其它 API、批量全部完成、离开页面后不恢复、不产生任何写入副作用。
   - 若现有 UI 测试基础不足，至少执行编译和人工浏览器验证。
## 目标文件

- `crates/core/src/rpc/types.rs`
- `crates/service/src/aggregate_api.rs`
- `crates/service/src/aggregate_api_tests.rs` 或拆分的 discovery 测试模块
- `crates/service/src/rpc_dispatch/aggregate_api.rs`
- `apps/src-tauri/src/commands/aggregate_api.rs`
- `apps/src-tauri/src/registry.rs`
- `apps/src/lib/api/transport-web-commands/aggregate-api.ts`
- `apps/src/lib/api/account-client.ts`
- `apps/src/types/api-key.ts`
- `apps/src/app/models/page.tsx`
- `apps/tests/transport-web-commands.test.mjs`（必要时）
## 验证命令

- 服务端：`cargo test -p codexmanager-service aggregate_api`
- 前端运行时映射：`pnpm -C apps run test:runtime`
- 前端静态导出：`pnpm -C apps run build:desktop`
- 若修改 Tauri command registry：`cargo test --workspace`（或按环境记录无法执行原因）。
- UI 行为：启动可用的桌面/Web service-mode，打开“模型与路由”，分别刷新多个 API，再使用“获取全部 API”，确认每个结果旁有明确 API 身份、同名模型不混淆、单个失败不影响其它 API；离开页面重开并确认临时结果不恢复。
- 本任务只读：数据库、本地模型目录、模型路由和供应商配置在整个查询展示流程中不得有任何写入。

## 风险检查点

- 新建 API 没有 ID/secret 时不得发 discovery；保存成功后才允许。
- 任何错误路径都不能包含 Authorization、API key、Basic 凭据、带 query 的 URL 或完整上游响应。
- 发现成功但用户未保存模型时，数据库和网关路由必须保持不变。
- Provider-specific models URL 不能破坏现有 `/messages`、`/responses`、Gemini generateContent 或健康探测路径。
- 发现结果必须是页面内存状态，不能进入 SQLite、`aggregate_api_supplier_models`、startup snapshot 或 managed model catalog。
- 结果索引必须使用 `aggregateApiId`；不得用 `upstreamModel` 作为全局唯一键。

## 完成门槛

- PRD、设计和执行计划均与“不落库、按 API 展示、只读查询、全部按钮隔离”一致。
- 服务端、RPC、Tauri、Web fallback 和前端类型链路同步。
- 目标测试与构建命令通过，或记录精确失败原因。
- 代码评审重点覆盖认证泄漏、解析边界、批量隔离、API 来源标识和零写入副作用。
