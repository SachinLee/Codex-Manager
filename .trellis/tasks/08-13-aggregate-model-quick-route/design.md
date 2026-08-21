# 技术设计：聚合模型预填确认与快捷路由

## 1. 设计结论

从现有 `AggregateApiModelDiscoveryDialog` 的每个发现项打开独立的预填确认弹窗。弹窗提交一个面向任务的“添加聚合模型路由”命令；服务端在一个 SQLite 事务中查找本地模型、创建或复用模型、合并目标聚合 API 路由，并返回完整模型及动作结果。

不让前端先读取完整模型、修改后调用通用 `ManagedModelV2Upsert`。通用 upsert 会整体重写路由集合，前端拼装不完整或过期的模型会覆盖价格、权限和其他路由。

## 2. 跨层合同

新增管理面 RPC：`apikey/managedModelAddAggregateRouteV2`。

请求：

```json
{
  "slug": "provider-model-id",
  "displayName": "Provider Model",
  "aggregateApiId": "agg-1",
  "upstreamModel": "provider-model-id"
}
```

约束：

- `slug`、`aggregateApiId`、`upstreamModel` 去首尾空白后必须非空；`upstreamModel` 是发现项 ID，不由服务端重新探测。
- `aggregateApiId` 必须对应已保存的聚合 API，防止写入孤儿路由。
- `slug` 的模型匹配大小写不敏感；匹配隐藏、禁用和自定义/内置模型，不因模型列表筛选状态漏判。
- `displayName` 仅用于新模型；已有模型不覆盖既有显示名或其他元数据。

响应：

```json
{
  "model": { "...": "完整 ManagedModelV2" },
  "created": true,
  "routeAction": "created"
}
```

`routeAction` 枚举：`created`、`updated`、`unchanged`。`created` 表示本地模型或目标路由新建；`updated` 表示目标 API 已有路由的上游模型名被替换；`unchanged` 表示模型和目标路由已经匹配。响应中的 `model` 是服务端提交后的事实，不返回发现原始响应。

同一合同沿以下链路映射：

```text
React quick-add dialog
  -> managedModelsV2Client.addAggregateRoute
  -> service_managed_model_add_aggregate_route_v2
  -> apikey/managedModelAddAggregateRouteV2
  -> models_v2::add_aggregate_route
  -> Storage::add_managed_model_aggregate_route_v2 (single transaction)
```

## 3. 核心存储事务

在 `crates/core/src/storage/model_catalog_v2.rs` 增加专用输入/结果类型和事务方法；不新增迁移。

事务步骤：

1. 校验输入文本并查询 `aggregate_apis`，不存在则失败。
2. 按 `slug COLLATE NOCASE` 查找本地模型。
3. 无匹配时创建最小自定义模型：
   - `origin=custom`、`enabled=true`、`supportedInApi=true`、`visibility=list`；
   - `display_name=displayName`，为空则使用 `slug`；`provider/family/category/context/maxContext/defaultReasoning` 均为空；`tags=[]`；
   - `capabilities={}`，不声明未经发现接口证明的能力；
   - `instructions_mode=passthrough`、`instructions_text=null`、`fast_policy=passthrough`；
   - `price_status=missing`，所有价格和阶梯为空，权限组为空，降级链为空；
   - 排序使用当前最大排序值加一；
   - 新增启用路由 `(aggregate_api, aggregateApiId, upstreamModel, priority=0, weight=1)`。
4. 有匹配时复制当前完整模型，仅处理目标来源路由：
   - 其他来源路由、价格、能力、显示名、状态、权限组、降级链全部保留；
   - 若没有 `(aggregate_api, aggregateApiId)` 路由，追加一条默认启用、优先级 0、权重 1 的路由；
   - 若有一条，保留其 route ID、启用状态、优先级和权重，只更新 `upstream_model`；相同值则 `unchanged`；
   - 若历史数据存在多条同来源路由，按现有查询顺序（priority 降序、id 升序）保留第一条并移除其余同来源重复项，然后应用本次 `upstream_model`，动作记为 `updated`。
5. 通过现有 `write_model` 在同一事务中写入模型、价格、阶梯、路由和权限关联；任一步失败则整个事务回滚。
6. 返回提交后的完整模型和动作结果。

服务层调用成功后复用 `sync_active_gateway_catalog_best_effort`；数据库原子性不依赖前端刷新或运行时同步成功。

## 4. 前端交互

### 4.1 发现弹窗

修改 `apps/src/components/aggregate-api/aggregate-api-model-discovery-dialog.tsx`：

- 每个发现模型 badge/条目旁增加可键盘访问的“添加到模型与路由”按钮；按钮携带当前 API 对象和 discovery item，不以模型 ID 单独索引。
- 发现项 ID、显示名、来源 API ID 在回调中保持稳定；按钮在发现请求进行时禁用，避免结果未完成时提交。
- 发现结果仍只在当前页面内存中保存；快捷添加只写模型目录和路由，不把发现结果写入缓存表。

新增 `AggregateApiModelQuickAddDialog`（建议放在同目录）：

- 打开时预填：本地模型 slug=发现项 `id`，显示名=`displayName || id`，聚合 API 名称/ID，`upstreamModel=id`。
- slug 和显示名允许管理员在确认前调整；来源 API 与上游模型 ID只读，防止确认弹窗变成任意路由写入器。
- 明确提示：本地已有同名模型时不会创建新模型，只新增或更新该 API 路由；价格、能力和权限不会由发现结果自动推断。
- 服务端返回 `created/routeAction` 后显示差异化成功提示；失败显示脱敏错误并保留弹窗内容以便重试。
- 提交期间禁用关闭/重复提交；使用现有 Dialog focus trap、Escape 关闭和可见焦点样式。错误使用 `role=alert` 或项目现有错误展示模式。

### 4.2 数据刷新

- 成功后调用 `managedModelsV2Client` 的返回结果更新/失效 `MANAGED_MODELS_V2_QUERY_KEY`（如果模型管理页当前挂载则由 Query 重新读取），并失效 `startup-snapshot`、`model-groups` 等现有消费者缓存。
- 聚合 API 页面无需把完整模型目录提升到全局 store；当前发现结果仍随页面卸载丢弃。
- 不在前端调用通用 upsert，不从现有模型列表拼装写入 payload。

## 5. RPC / Desktop / Web 同步

- `crates/service/src/rpc_dispatch/apikey.rs`：仅管理员可调用，解析专用请求，返回专用结果。
- `apps/src-tauri/src/commands/apikey.rs`：新增 `service_managed_model_add_aggregate_route_v2(addr, payload)` 并注册到 `apps/src-tauri/src/registry.rs`。
- `apps/src/lib/api/managed-models-v2.ts`：新增 typed `addAggregateRoute`。
- `apps/src/lib/api/transport-web-commands/apikey.ts`：增加相同 underscore command 到 RPC 方法映射，payload 映射与其他 V2 管理命令一致。
- `apps/src/types/model-v2.ts`：增加请求、动作枚举和响应类型；Rust 使用 `camelCase` 序列化。

## 6. 安全与兼容

- 只允许现有管理员模型管理权限；不扩展普通 API key/member 的模型写权限。
- 服务端不信任前端发现结果的来源存在性，必须重新检查 `aggregate_apis`。
- 不回显密钥、认证头、完整上游 JSON 或 URL 查询参数；本操作不需要再次访问上游。
- 没有数据库迁移；旧客户端继续使用已有模型命令，新增命令是纯增量。
- 聚合 API 删除时既有外键/清理逻辑仍会移除关联路由；本能力不改变删除语义。

## 7. 风险与回滚

- 风险：模型 ID 不适合作为本地 slug。通过现有模型 slug 校验和确认弹窗允许管理员编辑 slug；服务端拒绝空值/非法写入，不自动改写成不可预测值。
- 风险：已有同源重复路由。专用事务按确定顺序收敛为一条，避免网关按优先级产生隐式结果。
- 回滚：移除新增 RPC、命令、弹窗按钮和存储方法即可；无 schema 回滚。已创建的模型/路由属于用户确认后的业务数据，不能通过代码回滚自动删除。
