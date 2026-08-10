# 技术设计：聚合 API 连续失败冻结开关

## 边界

- 目标：每个聚合 API 可配置 `enableConsecutiveFailureFreeze`（默认 `true`），关闭后该 API 的**连续失败冻结**不再触发。
- 只影响"连续失败达到阈值（5 次）触发冻结"这一种语义；分类冷却（auth 401/403、model unsupported、rate_limited）与零余额禁用、显式启停均不受影响。
- 保留 health 观察历史（状态、事件），只抑制"连续失败 → cooldown"的状态转换。

## 现状（两条连续失败冻结链路，阈值均为 5）

1. **内存冷却** `crates/service/src/gateway/routing/aggregate_api_cooldown.rs`
   - `record_aggregate_api_failure(api_id, model)`：连续 5 次 → `cooldown_until = now + 300s`，按 `(api_id, upstream_model)` key，并 `sync_policy_action` 记录 system cooldown。
   - `is_aggregate_api_in_cooldown(api_id, model)`：路由前过滤（`gateway/upstream/protocol/aggregate_api.rs:1459` 调用）。
2. **持久化 health 冷却** `crates/service/src/aggregate_api_health.rs`
   - `record_observation_with_storage` 的 generic 分支：`consecutive_failures >= FAILURE_THRESHOLD(5)` → `cooldown_until`。
   - `is_routing_blocked_with_storage`：仅当该 API 的 proactive health 探测 `config.enabled` 时参与路由阻断。
3. **gateway 包装** `crates/service/src/gateway/mod.rs`
   - `gateway_is_aggregate_api_in_cooldown` = 内存冷却 OR health 阻断。
   - `gateway_record_aggregate_api_failure` = health 被动观察 + 内存冷却记录（`aggregate_api.rs:2518` 在 `cooldown_eligible_failure` 时调用）。

## 设计

### 存储层（crates/core）

- `aggregate_apis` 表新增列 `enable_consecutive_failure_freeze INTEGER NOT NULL DEFAULT 1`：
  - `ensure_aggregate_apis_table`：CREATE TABLE 加列 + `ensure_column(...)`（兼容旧库）。
  - 新增 migration `134_aggregate_api_enable_consecutive_failure_freeze.sql`（AGENTS.md：schema 变更进 `crates/core/migrations/`）。
- `AggregateApi` / `AggregateApiListSummary` struct 加 `enable_consecutive_failure_freeze: bool`；INSERT/SELECT/mapper 同步（注意 `aggregate_api_with_secrets_by_id_sql` 的列索引：新列插在 `a.last_balance_json` 后，secret/access_token 索引顺延）。
- 新增单列读取 `aggregate_api_consecutive_freeze_enabled(api_id) -> Result<Option<bool>>`（gateway/health 路由判断用，避免整行读取）。
- 新增 `update_aggregate_api_consecutive_freeze(api_id, enabled)` + SQL。

### 服务层（crates/service）

- `aggregate_api.rs`：
  - `create_aggregate_api` 加参 `enable_consecutive_failure_freeze: Option<bool>`，`unwrap_or(true)`。
- `update_aggregate_api` 加同参；显式提供时调用存储更新；**置为 false 时调用 `gateway::gateway_clear_aggregate_api_cooldowns(api_id)`** 立即解除既有内存冷却（避免 UI 残留"冷却中"误导；health 持久化 state 保留为观察历史）。
- `aggregate_api_health.rs::record_observation_with_storage`：generic 分支改为仅在"连续失败达标 **且** 开关开启"时设置 cooldown；开关关闭时 state 保持 `degraded`（失败计数继续累计，观察/事件照常持久化）。
- `is_routing_blocked_with_storage`：开关关闭时动态忽略既有 generic `cooldown`，但保留 `auth`、`model_not_supported`、`rate_limited` 分类冷却，以及 `unhealthy` 状态。
- 开关查询只发生在 generic 失败达标分支或冷却判断内，查询失败按 `true` 保守处理。
- `gateway_record_aggregate_api_failure`：关闭 → 跳过内存 `record_aggregate_api_failure`；health 观察照常；开启/查询失败 → 现有行为。
- `gateway_is_aggregate_api_in_cooldown`：只用开关门控内存冷却，始终继续评估持久化 health 冷却，避免关闭开关误绕过分类冷却。
- `rpc_dispatch/aggregate_api.rs`：`aggregateApi/create`、`aggregateApi/update` 解析 `enableConsecutiveFailureFreeze`（`bool_param`，缺省 None → 默认 true / 保持原值）。
- Web command map 为 RPC 参数透传，无显式 `aggregateApi` 映射，无需改动。

### 前端（apps）

- `types/api-key.ts`：`AggregateApi` 加 `enableConsecutiveFailureFreeze: boolean`。
- `lib/api/normalize.ts`：`normalizeAggregateApi` 加 `asBoolean(source.enableConsecutiveFailureFreeze, true)`。
- `lib/api/account-client.ts`：`AggregateApiPayload` 加字段；create/update 请求体透传。
- `components/modals/aggregate-api-modal.tsx`：新增开关（默认开），编辑回填，保存 payload 携带。
- `app/aggregate-api/page.tsx`：表格新增可直接修改的"连续失败冻结"开关列；移除无实际用途的"健康监测"列及其仅为该列加载的探测成本/配置逻辑。

## 契约

- RPC 方法名不变：`aggregateApi/create`、`aggregateApi/update`、`aggregateApi/list`；仅参数/返回新增 `enableConsecutiveFailureFreeze`（camelCase）。
- 语义：`true`/缺省 = 冻结开启（现状）；`false` = 该 API 连续失败不再冻结。
- 热路径不新增整行查询；失败记录和冷却判断仅读取单个开关列。

## 兼容与回滚

- 旧库：`ensure_column` + migration 默认 `1`，行为不变。
- 回滚：去掉列/字段/分支即可；无需数据迁移（布尔默认值）。
- 开关关闭只解除该 API 的连续失败冻结；分类冷却、零余额禁用、显式启停语义不变。
