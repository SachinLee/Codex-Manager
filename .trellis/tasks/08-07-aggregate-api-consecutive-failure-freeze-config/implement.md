# 实施计划：聚合 API 连续失败冻结开关

## 变更清单（按依赖顺序）

### 1. crates/core 存储层
- [x] `crates/core/migrations/134_aggregate_api_enable_consecutive_failure_freeze.sql`（新增，`ALTER TABLE aggregate_apis ADD COLUMN enable_consecutive_failure_freeze INTEGER NOT NULL DEFAULT 1`）
- [x] `crates/core/src/storage/mod.rs`：`AggregateApi`、`AggregateApiListSummary` 加 `pub enable_consecutive_failure_freeze: bool`
- [x] `crates/core/src/storage/aggregate_apis_sql.rs`：SELECT/secret projection 加列；新增开关读写 SQL
- [x] `crates/core/src/storage/aggregate_apis.rs`：table ensure、INSERT、行 mapper、开关读写方法
- [x] `crates/core/tests/storage.rs`：新增聚合 API 开关 round-trip 测试

### 2. crates/service 服务层
- [x] `crates/service/src/aggregate_api.rs`：`create_aggregate_api` / `update_aggregate_api` 加参数；update 置 false 时清理内存冷却
- [x] `crates/service/src/aggregate_api_health.rs`：generic 分支受开关控制；分类 cooldown 保持有效；回归测试
- [x] `crates/service/src/gateway/mod.rs`：仅内存连续失败冷却受开关控制，持久化分类冷却仍检查
- [x] `crates/service/src/rpc_dispatch/aggregate_api.rs`：create/update 解析 `enableConsecutiveFailureFreeze` 并传参

### 3. apps 前端
- [x] `apps/src/types/api-key.ts`：`AggregateApi` 加 `enableConsecutiveFailureFreeze`
- [x] `apps/src/lib/api/normalize.ts`：`normalizeAggregateApi` 加字段映射
- [x] `apps/src/lib/api/account-client.ts`：`AggregateApiPayload` 加字段；create/update 透传
- [x] `apps/src-tauri/src/commands/aggregate_api.rs`：桌面 create/update command 透传该字段
- [x] `apps/src/components/modals/aggregate-api-modal.tsx`：开关 UI + 回填 + 保存
- [x] `apps/src/app/aggregate-api/page.tsx`：新增连续失败冻结列；移除健康监测列及其专用探测成本/配置请求

## 验证命令

- [x] `cargo test -p codexmanager-core --test storage aggregate_api_consecutive_freeze_flag_roundtrips`（通过；完整 core suite 另有 2 个既有 model billing 失败）
- [x] `cargo test -p codexmanager-service --lib freeze_switch`、`persisted_cooldown_only_blocks_when_proactive_monitoring_is_enabled`、list contract test（通过；完整 service suite 另有 6 个既有 balance/pricing 失败）
- [x] `pnpm -C apps run build`（通过）
- [x] `pnpm -C apps run test:runtime`（206/206 通过）

## 审查门

- [x] 开关默认 true，老数据行为不变
- [x] 分类冷却（auth/model/rate_limited）、零余额禁用、显式启停不受影响
- [x] 热路径不新增整行查询；失败记录和冷却判断仅读取单个开关列
- [x] RPC 契约：create/update/list 增 `enableConsecutiveFailureFreeze`（camelCase），方法名不变

## 回滚点

- 存储层完成（migration + struct + 读写）即可暂停：新列默认 true 无行为变化。
- 服务层完成、前端未动：RPC 已支持但 UI 未暴露，行为仍默认开启。
