# 技术设计：API 模型发现与路由配置

## 1. 设计目标

在已保存的聚合 API 上执行一次认证的 `GET /models`（按现有 base URL 规范化），将响应转换为按 API 分组的短生命周期展示结果。结果只读查询和展示，不进入 SQLite、模型目录、网关配置或 React Query 的持久化层。

## 2. 边界与职责

### 服务端

- `crates/service/src/aggregate_api.rs`
  - 新增 `discover_aggregate_api_models(api_id)`。
  - 读取已保存 API 与 secret；复用 `upstream_client_for_aggregate_api_candidate`、`apply_probe_auth` 和请求超时策略。
  - 构造模型目录 URL：OpenAI/Claude/compatible 沿用 `<base>/v1/models` 规范；Gemini 使用其现有 provider 语义对应的 models 路径，避免误把 `v1beta` 请求成 `v1`。
  - 只读取有限大小的响应体，解析常见目录形状：`data[]`、`models[]`，必要时接受根数组；条目 ID 优先取 `id`，再取 `name`，显示名取 `display_name` / `displayName` / `name`。
  - 去除空 ID，按 ID 去重并保留首次出现顺序；不返回原始 JSON、认证信息或完整错误体。
  - 将 HTTP/解析/超时结果归一为结构化结果和短错误码/摘要。上游 401/403/404 等不抛成破坏本地配置的写操作。

### RPC / Desktop / Web

- `crates/core/src/rpc/types.rs`
  - 新增 `AggregateApiModelDiscoveryItem` 与 `AggregateApiModelDiscoveryResult`，使用 `camelCase` 序列化。
  - Result 至少包含 `apiId`、`ok`、`items`、`statusCode`、`discoveredAt`、`message`；Item 包含 `id` 与可选 `displayName`。
- `crates/service/src/rpc_dispatch/aggregate_api.rs`
  - 新增 `aggregateApi/models/discover` 分支，仅接收 `id`。
- `apps/src-tauri/src/commands/aggregate_api.rs`
  - 新增 `service_aggregate_api_models_discover`，调用同名 RPC。
- `apps/src-tauri/src/registry.rs`
  - 注册新 Tauri command。
- `apps/src/lib/api/transport-web-commands/aggregate-api.ts`
  - 添加 `service_aggregate_api_models_discover: { rpcMethod: "aggregateApi/models/discover" }`。
- `apps/src/lib/api/account-client.ts`
  - 添加 typed `discoverAggregateApiModels(apiId)`，只返回规范化类型。
- `apps/src/types/api-key.ts`
  - 添加前端 discovery item/result 类型。

### 前端交互

- `apps/src/app/models/page.tsx`
  - 维护 `discoveryByApiId: Record<string, AggregateApiModelDiscoveryResult>`、单 API loading 集合和批量 loading 状态；这些状态只存在页面内存。
  - 在模型目录上方增加“上游 API 模型”面板，按 `aggregateApis` 渲染一行/一卡一 API，显示供应商名称、API ID、provider 类型、脱敏 URL、发现时间、模型数量、状态和错误摘要。
  - 每行提供“获取模型”；面板提供“获取全部 API 模型”。批量操作对 API 分别调用同一 discovery RPC，使用 `Promise.allSettled` 或等价隔离逻辑，不因一个 API 失败而丢失其它结果。
  - 每个结果的模型列表必须带所在 API 的上下文；同名模型按 `(apiId, modelId)` 识别，不做跨 API 合并。
  - 结果只在页面内存中展示；不写入模型编辑草稿、不写入全局 store、startup snapshot 或持久化 query cache。
- `apps/src/components/modals/model-catalog-modal.tsx`
  - 本任务不修改模型路由编辑器；现有手工配置流程保持不变。
- 页面卸载或刷新时清空所有结果；切换页面不会显示之前页面的发现结果。

### RPC / 批量策略

- RPC 维持“单 API 一次 discovery”合同，不新增服务端批量接口；批量按钮由前端按 API ID 调用，便于逐 API 展示状态并限制并发。
- Result 的 `apiId` 是服务端回显的稳定关联键，前端不得仅按模型 ID 建索引。

## 3. 数据流

```text
models/page.tsx 加载已保存 aggregateApis
  -> 按 apiId 单独触发 discovery（或前端隔离批量触发）
  -> Tauri command / Web RPC
  -> service aggregateApi/models/discover(apiId)
  -> storage 读取该 API + secret
  -> 认证 GET models（仅服务端）
  -> 解析/去重/脱敏 result(apiId)
  -> 页面按 apiId 展示模型列表
```

发现阶段没有任何持久化副作用；不同 API 的请求、loading、错误和结果相互隔离。

## 4. 响应与错误合同

成功示例（不保存）：

```json
{
  "apiId": "agg-1",
  "ok": true,
  "items": [
    { "id": "provider-model-a", "displayName": "Provider Model A" }
  ],
  "statusCode": 200,
  "discoveredAt": 1780000000,
  "message": null
}
```

`apiId` 必须在每个结果中返回，用于保证 UI 能明确回答“哪个 API 返回了这些模型”。前端展示 API ID、供应商名称和 provider 类型；URL 仅作来源辅助信息，不显示任何认证参数。

失败只返回稳定摘要，例如：`models endpoint returned HTTP 401`、`models response is not a supported catalog`、`models request timed out`。不得把 query URL、Authorization、x-api-key、Basic 凭据或完整 body 拼进错误。

空目录是 `ok=true`、`items=[]`、`message="models endpoint returned an empty catalog"`，与请求失败区分；批量发现时每个 API 独立保留该状态。

## 5. 兼容与风险

- 不新增数据库迁移；既有数据库无需变更。
- 不复用 `aggregate_api_supplier_models` 旧表，因为其 supplier 级键与当前 API 实例不匹配，且会违反不落库决定。
- 不改变网关 `/v1/models` 对客户端的本地目录行为；新增 RPC 是管理面只读能力，不是代理面或路由写入能力。
- 只允许已保存 API；新建 API 必须先完成保存，避免临时凭据进入新 RPC 合同。
- 限制响应体大小并设置请求超时，防止异常供应商造成内存或界面等待问题。
- 解析只接受白名单字段和常见数组形状，不把任意上游 JSON 透传到 UI。

## 6. 回滚

回滚只需移除新增 discovery RPC、客户端方法和 UI 按钮；数据库无迁移、无缓存数据、无网关运行时开关，因此不需要数据回滚。
