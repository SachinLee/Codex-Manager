# 技术设计：聚合 API 冷却状态与手动重置

## 边界

- 不改动冷却阈值、冷却时长、路由优先级或 SQLite schema。
- 新能力仅读取/清理服务进程内存中的 aggregate API 冷却状态。
- 一切通过现有 service RPC、Web command map、前端 typed client 传递；不把密钥、完整上游错误响应或请求体返回给浏览器。

## 服务端

### 运行时快照

在 `gateway/routing/aggregate_api_cooldown.rs` 中新增面向服务层的快照类型与读取函数。每个聚合 API 返回：

- `aggregate_api_id`
- `is_cooling_down`
- `consecutive_failures`
- `failure_threshold`
- `cooldown_until`
- `remaining_secs`
- `last_failure_at`
- `reason`

读取快照时复用现有过期清理逻辑，并用当前时间重新计算 `remaining_secs`。原因只使用稳定的内部分类/文案（当前为连续 aggregate API 失败），不返回上游原始错误。

### 清理语义

单个 API 的手动重置必须同时：

1. 删除 aggregate API 的冷却 entry（失败计数、截止时间）。
2. 删除关联的 aggregate API runtime policy action。

这样路由筛选、运行时状态和 request-log policy action 不会出现“已解除但仍显示冷却”的分裂状态。成功请求触发的既有清理也复用相同语义。

### RPC

在 aggregate API RPC 域新增两个受现有 service access guard 保护的能力：

- `aggregateApi/runtimeStatus/list`：返回所有当前/近期内存状态；前端按 API id 与配置列表合并。
- `aggregateApi/runtimeStatus/reset`：接收单个 `aggregateApiId`，验证该聚合 API 存在后清理其运行时状态，返回已清理的快照/成功结果。

命令名称在 Tauri 侧使用下划线形式，在 Web command map 映射到上述 camelCase RPC method。服务模式与桌面模式走同一业务实现。

## 前端

### 数据层

- 在 `AggregateApi` 配置类型之外定义 `AggregateApiRuntimeStatus`，避免把瞬态内存状态伪装成持久化配置。
- `account-client` 新增 `listAggregateApiRuntimeStatuses()` 和 `resetAggregateApiRuntimeStatus(id)`。
- 聚合 API 页面使用独立 React Query（约 2 秒轮询）；倒计时依据 `cooldownUntil` 每秒在本地刷新。配置列表不参与该高频刷新。

### 表格

- 第一列固定约 `300px`，保持现有 URL/模型/成本 Tooltip。
- `Guard` 后新增 `路由状态` 列，约 `176px`。
- 原 `状态` 标题更名为 `启用`，其开关语义不变。
- 正常 API：轻量绿色 `可路由`；不显示主重置入口。
- 冷却 API：橙色 `冷却中 mm:ss`、`连续失败 5/5`、`解除冷却` 次级按钮。
- 原因、最近失败时间、截止时间使用已存在 Tooltip/Popover 构件展示。可给冷却单元格加淡橙色强调，不使用整行醒目底色。

### 重置交互

点击 `解除冷却` 打开现有 `ConfirmDialog`：

> 解除后，{供应商} 将立即重新加入路由候选；若上游仍异常，可能再次失败并进入冷却。是否继续？

确认后执行 mutation；成功 toast、失效/更新 runtime status query；失败显示经 `getAppErrorMessage()` 处理的错误。

## 验证与兼容性

- 冷却单元测试覆盖：失败阈值、快照剩余时间、重置清理冷却+policy action、过期状态。
- RPC 定向测试覆盖：列表与重置、未知 API 的校验。
- 前端 type/build 验证确保 Tauri 和 web transport 参数名称一致。
- 不添加无法稳定运行的浏览器 E2E；UI 行为由 typed client/构建验证与人工运行态复核覆盖。

## 回滚

删除/禁用新增 UI 调用即可恢复旧页面；运行时状态本来就是内存数据，无迁移和持久化回滚成本。RPC 添加为向后兼容能力，不影响旧客户端。
