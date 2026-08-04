# 实施计划

## Phase 0: 开始前检查

1. 阅读 `apps/AGENTS.md`、`.trellis/spec/codexmanager-core/backend/index.md`、`.trellis/spec/codexmanager-service/backend/index.md` 及其引用规范。
2. 确认实现前 migration 最新编号、RPC dispatch/transport 注册链和 `lifecycle/startup.rs` 的后台任务启动点。
3. 创建实现分支并在 `task.py start` 前让用户确认本规划摘要；本文件完成不等于实现授权。

## Phase 1: Core storage / migration

1. 新增 `crates/core/migrations/117_aggregate_api_health.sql`，创建 config/state/events 表、CHECK、索引和已有 source 默认配置。
2. 在 `crates/core/src/storage/mod.rs` 增加 config/state/event DTO，保持 `AggregateApi` 现有字段兼容。
3. 在 `crates/core/src/storage/aggregate_apis.rs` 或新 focused storage module 实现 config CRUD、due probe 查询、state upsert/reducer transaction、history list/prune/delete cascade。
4. 在 `crates/core/src/rpc/types.rs` 增加 health summary/config/state/event/probe result DTO；字段使用现有 camelCase serde 约定。
5. 先写 storage migration、默认值、裁剪、重启恢复和 delete cascade 测试，再执行 `cargo test -p codexmanager-core`。

## Phase 2: Service domain / state machine

1. 新建 `crates/service/src/aggregate_api_health.rs`（必要时拆 `health_classifier.rs`、`health_scheduler.rs`），定义 observation、error category、state reducer 和 route gate。
2. 把现有 probe 逻辑抽成共享内部接口，保留 `test_aggregate_api_connection` 旧返回格式；为 provider、超时、空模型、脱敏错误补 unit tests。
3. 将 gateway attempt outcome 的 aggregate API 调用点接入 reducer；删除/旁路重复的旧 cooldown 计数，确保阈值和状态只有一个事实来源。
4. 加载 persisted state 到内存 gate，处理 cooldown 到期、half-open single-flight、成功清零和人工 reset；storage 故障按设计 fail-open。
5. 实现 scheduler：enabled source due scan、代表模型选择、15m/5m adaptive interval、jitter、worker=2、单 probe timeout、失败退避、退出清理。
6. 在 `crates/service/src/lifecycle/startup.rs` 与 usage refresh 模块注册健康 polling；确保 desktop/service/web 三种模式均启动一次。
7. 增加服务测试：状态转移矩阵、错误分类、Retry-After/指数退避、source/model scope、half-open、并发/取消、重启恢复、candidate exclusion、旧 cooldown 兼容。

## Phase 3: RPC / transport / frontend

1. 在 `crates/service/src/rpc_dispatch/aggregate_api.rs` 注册 list/get/config/probe/reset，并保留 runtimeStatus 兼容分支。
2. 在 `apps/src-tauri/src/commands/aggregate_api.rs` 增加 typed command wrappers。
3. 在 `apps/src/lib/api/transport-web-commands/aggregate-api.ts`、`account-client.ts`、`apps/src/types` 增加映射、normalizer 和 React Query hooks。
4. 在 `apps/src/app/aggregate-api/page.tsx` 与 `apps/src/components/aggregate-api/` 实现 badge、详情事件列表、设置、立即检测和解除冷却；处理 loading/error/disabled/unknown。
5. 增加 RPC dispatch、web command mapping、normalizer 和关键页面交互测试；确保旧 API 客户端字段缺失时使用 unknown/legacy fallback。

## Phase 4: 文档与验证

1. 更新设置页面说明、环境变量/默认值文档和必要的本地 service-mode 运行说明；不新增环境变量除非动态 polling 配置确实需要。
2. 运行 `cargo fmt --all -- --check`、`cargo test -p codexmanager-core`、`cargo test -p codexmanager-service`、`cargo test --workspace`（按环境能力逐步执行）。
3. 运行 `pnpm -C apps run build`、`pnpm -C apps run test:runtime`；涉及静态 desktop 链路时运行 `pnpm -C apps run build:desktop`。
4. 用 mock upstream 覆盖手动 probe、scheduled probe、429/Retry-After、401/403、stream/non-stream 请求后的被动状态；检查 SQLite 中不存在 token/header/body。
5. 运行 `git diff`、敏感信息扫描和相关 migration/storage tests；完成 Trellis quality check 后再提交。

## 风险文件与回滚点

- 高风险：`crates/service/src/gateway/upstream/protocol/aggregate_api.rs`、`crates/service/src/gateway/routing/aggregate_api_cooldown.rs`、`crates/service/src/lifecycle/startup.rs`。每个文件修改后先运行对应 targeted tests。
- migration 回滚点：scheduler 默认关闭；发现 schema/状态异常时关闭 health polling，保留旧 `last_test_*`/cooldown 路由行为。
- UI/RPC 回滚点：保留旧 runtimeStatus/testConnection 命令，前端可隐藏 health panel 而不影响聚合 API CRUD。

## Definition Of Done

- PRD、design、implement 已获用户最终批准且 task 已进入 `in_progress`。
- Core/service/frontend 测试与构建通过，或记录精确未执行命令及原因。
- 状态检测在 active/disabled、unknown/healthy/degraded/unhealthy/cooldown/recovering、主动/被动/手动和重启场景均有可验证行为。
- 没有 secret、完整响应体或 raw upstream URL credentials 出现在事件、日志、RPC 或 UI。
