# 实施计划：聚合模型快捷路由

## 1. 先行合同与类型

1. 在 `apps/src/types/model-v2.ts` 与 `crates/core/src/storage/model_catalog_v2.rs` 定义同语义的请求、动作结果和返回合同：`slug`、`displayName`、`aggregateApiId`、`upstreamModel`、`created`、`routeAction`、完整模型。
2. 明确 `routeAction` 的三种值以及大小写不敏感 slug、已有模型不覆盖元数据、同源路由更新规则。
3. 在 `implement.jsonl` / `check.jsonl` 加入本任务 PRD、设计和相关规格作为实现/检查上下文；删除 seed-only 示例行。

## 2. 核心事务与服务

1. 为 `Storage` 增加 `add_managed_model_aggregate_route_v2`：查询已保存聚合 API，大小写不敏感匹配已有模型，在一个事务内创建最小模型或合并目标路由。
2. 复用现有模型写入/校验逻辑；新模型使用 missing price、空能力、空权限组和默认路由；已有模型只修改目标 API 路由。
3. 处理同一模型/同一 API 的历史重复路由：确定性保留一条，更新 upstream model。
4. 在 `crates/service/src/models_v2/mod.rs` 增加服务函数并调用现有运行时目录同步。
5. 在 `crates/service/src/rpc_dispatch/apikey.rs` 增加管理员 RPC 分支，使用专用类型解析和结构化错误。

## 3. Desktop / Web transport

1. 增加 `apps/src-tauri/src/commands/apikey.rs` 命令并在 `apps/src-tauri/src/registry.rs` 注册。
2. 增加 `managedModelsV2Client.addAggregateRoute` typed wrapper。
3. 增加 `service_managed_model_add_aggregate_route_v2` 的 Web RPC 映射，确保 payload 从 desktop 与 service-mode 得到同一 JSON。
4. 增加前端模型类型与必要的 normalizer/错误处理；不复制后端 storage row 解析逻辑。

## 4. 预填确认 UI

1. 在 `AggregateApiModelDiscoveryDialog` 的每个发现项增加“添加到模型与路由”按钮，传递完整 API 上下文和发现项。
2. 创建 `AggregateApiModelQuickAddDialog`：预填 slug/显示名，来源 API 与 upstream model 只读，展示已有模型会复用的说明，确认后调用 typed wrapper。
3. 接入 loading、失败重试、成功动作反馈和关闭/刷新行为；不把发现数据写入 Zustand、数据库或持久化 query cache。
4. 成功后让当前模型查询失效/重载；保留聚合 API 发现结果的页面内存边界。
5. 更新中文、英文、韩文、俄文对应消息段，所有新增按钮具备文本名称、键盘操作和动态错误可感知性。

## 5. 验证与质量门

### 并发与内存约束

- Rust 单元测试统一使用 `-- --test-threads=2`，不运行无界测试线程；优先运行窄目标包/测试，不直接并行启动多个 Cargo 测试命令。
- Node runtime 测试使用 `node --test --test-concurrency=2`；如通过现有脚本执行，需确认脚本不会额外开启高并发。
- Playwright 使用 `--workers=1`，避免浏览器上下文并发占用内存；同一时间只运行一个 UI 测试命令。
- 构建与测试不交叉并发；出现内存压力时优先拆分验证批次，而不是扩大并发。

### 核心 / 服务

- `cargo test -p codexmanager-core model_catalog_v2 -- --test-threads=2`
- `cargo test -p codexmanager-service aggregate_api_tests -- --test-threads=2`
- 重点覆盖：新模型创建、已有模型不重复创建、已有模型保留价格/权限/其他路由、同源路由 upstream 更新、同源重复路由收敛、未知 API 拒绝、事务失败不留半状态、非管理员拒绝。

### 前端 / Web

- `pnpm -C apps exec node --test --test-concurrency=2`（或使用 `pnpm -C apps run test:runtime` 后确认脚本保持受限并发）
- `pnpm -C apps exec playwright test tests/models-management.spec.ts --workers=1`（若新 UI 流程放在此套件）
- `pnpm -C apps run build:desktop`
- 增加/更新 Web command mapping 测试，验证新命令的 RPC 名称和 payload 映射。

### 手工 smoke

1. 在 Web/service-mode 打开聚合 API，发现一个模型。
2. 打开预填确认弹窗，核对来源 API ID、upstream model、默认 slug/显示名。
3. 确认后在模型目录验证模型与路由；再次对同一发现项操作，验证不创建重复模型且路由结果为 unchanged。
4. 对已有模型且已有同源路由使用不同 upstream model，确认后验证只更新该路由，价格、权限和其他路由不变。
5. 关闭/刷新页面，验证发现结果仍不恢复；模型目录持久化结果保持可见。

## 6. Review gate

- 先检查合同是否在 Rust、Tauri、Web mapping、typed client、前端类型和 UI 中完全对齐。
- 再检查专用事务是否避免普通 upsert 的整集合覆盖风险。
- 最后执行前端构建、Rust 目标测试、runtime/Web mapping 测试与浏览器 smoke；不得以只通过编译替代行为验证。
