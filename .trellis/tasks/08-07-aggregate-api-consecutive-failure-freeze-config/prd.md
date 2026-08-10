# 添加聚合API连续失败冻结开关

## Goal

给每个上游聚合API增加一个可配置开关 `enable_consecutive_failure_freeze`，默认开启（保持原有连续5次502冻结行为），允许用户关闭此功能（部分用户不想要冻结机制）。

## Confirmed Facts

- 连续失败冻结逻辑在 `crates/service/src/aggregate_api_health.rs` 中处理。
- `AggregateApiRuntimeStatus` 和 `aggregate_api.rs` 中有 `consecutive_failures` 计数。
- 当前冷却逻辑在 `aggregate_api_cooldown.rs` 和 `proxy_aggregate_request` 中触发。
- 已有 `AggregateApi` 模型支持扩展字段。
- 前端聚合API页面已在使用 `AggregateApiSummary` 和 runtime status。
- 数据库 `aggregate_apis` 表已有 `status`、`cooldown_until` 等字段。

## Requirements

### R1. 配置字段
- 新增字段 `enable_consecutive_failure_freeze: bool` 到 `AggregateApi` 模型。
- 默认值 `true`（不改变现有行为）。
- 支持在创建/更新API时指定该值。

### R2. 路由逻辑
- 连续失败次数 >= 5 时，仅当 `enable_consecutive_failure_freeze == true` 时才触发冷却/冻结。
- 其他冷却机制（auth 401、model unsupported、rate limit 等）保持不变。

### R3. 前端管理面
- 聚合API列表页新增“连续失败冻结”开关列（与“启用”并列显示）。
- 管理员可在列表列或单个API编辑弹窗中配置该开关。
- 新增API时该开关默认已勾选。

### R4. 后端 API 合约
- 更新 `list_aggregate_apis`、`create_aggregate_api`、`update_aggregate_api` 等 RPC 方法，使 `enable_consecutive_failure_freeze` 可序列化/反序列化。
- 管理界面通过 typed RPC 获取/设置该值。

### R5. 数据库与迁移
- 新增 migration `134_aggregate_api_enable_consecutive_failure_freeze.sql`。
- 现有数据必须保持默认开启（`true`）。

### R6. 聚合 API 列表精简
- 移除当前无实际用途的“健康监测”列，以及仅为该列加载的探测配置和探测成本数据。
- 保留连通性测试所需的健康数据读取，不移除既有健康 API。

## Acceptance Criteria

- [ ] 连续5次502时，开关开启时触发冻结，关闭时不触发（仅影响连续失败冻结）。
- [ ] 其他冷却机制（401、model unsupported、rate limit 等）不受开关影响。
- [ ] 前端列表页能看到每个API的“连续失败冻结”开关状态。
- [ ] 新增API默认开关已勾选。
- [ ] 数据库字段存在且默认值为 `true`。
- [ ] 老数据迁移后 `NULL` 字段按 `true` 处理。
- [ ] 聚合 API 列表不再显示“健康监测”列，且不会为该列请求探测成本或主动探测配置。
- [ ] 管理界面编辑后，RPC 返回正确值，状态立即生效。

## Scope Boundaries

- 不影响其他冷却机制。
- 不修改现有连续失败阈值（5次）。
- 仅向 `aggregate_apis` 新增一个布尔字段，不改变其他表或 API 方法名。
