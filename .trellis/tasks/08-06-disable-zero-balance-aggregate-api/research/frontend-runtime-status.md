# 聚合 API 零余额临时禁用：前端运行态、RPC 与 transport 调研

## 调研范围与结论等级

- 本文只追踪既有**失败冷却**运行态的服务 RPC、Tauri/Web transport、`apps/src/` 表格与手动重置交互；没有阅读余额供应商适配器实现，也没有修改产品代码或运行构建、测试、lint。
- **已证实**表示直接由下列仓库文件和符号支撑；**[推论]**表示由已证实的当前契约和本任务 PRD 导出的规划约束。
- 任务目标要求零余额状态不得覆盖显式启停或失败冷却：`.trellis/tasks/08-06-disable-zero-balance-aggregate-api/prd.md` — `R1`、`R2`、`Scope Boundaries`。

## 已证实：现有冷却状态的类型与业务语义

### 持久化配置、失败冷却与健康监测是三个独立面

| 面 | 证据锚点 | 当前语义 | 对零余额禁用的约束 |
| --- | --- | --- | --- |
| 显式配置启停 | `apps/src/types/api-key.ts` — `AggregateApi.status`; `apps/src/app/aggregate-api/page.tsx` — `toggleMutation`（约 511–533）、表格“启用”`Switch`（约 1130–1136） | `status` 是聚合 API 配置字段；开关调用 `accountClient.updateAggregateApi()`，并失效 `aggregate-apis`、`apikeys`、`startup-snapshot`。 | **必须独立。** 自动零余额结果不能修改或伪装成该持久化 `status`，也不能改变开关的含义。 |
| 失败冷却运行态 | `crates/core/src/rpc/types.rs` — `AggregateApiRuntimeStatus`; `apps/src/types/api-key.ts` — 同名接口; `crates/service/src/gateway/routing/aggregate_api_cooldown.rs` — `list_aggregate_api_cooldown_statuses`、`clear_aggregate_api_cooldowns` | 条目按 `aggregate_api_id + upstream_model` 表达失败次数、阈值、截止时间、剩余秒数、最后失败和原因。 | **必须独立。** 此类型的字段、模型粒度及清理副作用都面向“连续失败冷却”，不代表余额观测。 |
| 主动健康监测 | `apps/src/types/api-key.ts` — `AggregateApiHealthState`、`AggregateApiHealthConfig`; `apps/src/app/aggregate-api/page.tsx` — `activeProbeMutation`（约 431–454）、“健康监测”单元格（约 1048–1110） | 有单独的 `aggregate-api-health` 查询、15 秒轮询、主动探测开关与探测模型选择。 | **必须独立。** 零余额不是健康探测的 `unknown/healthy/degraded/unhealthy/cooldown/recovering` 状态。 |

失败冷却的清理不是纯 UI 标记：`crates/service/src/gateway/mod.rs` — `gateway_reset_aggregate_api_runtime_status()` 调用 `aggregate_api_cooldown::clear_aggregate_api_cooldowns(api_id)`；`crates/service/src/gateway/routing/tests/aggregate_api_cooldown_tests.rs` — `aggregate_api_cooldown_snapshot_and_reset_clear_policy_action()` 断言清理了对应 system policy action。因此，既有 `runtimeStatus/reset` **不能**被重新定义为“解除零余额禁用”，也不应让一次零余额解除清掉失败冷却或其审计/策略行动。

### 现有运行态数据契约

- Rust 对外结构：`crates/core/src/rpc/types.rs` — `AggregateApiRuntimeStatus`（约 975–990）以 `#[serde(rename_all = "camelCase")]` 返回 `aggregateApiId`、`upstreamModel`、`isCoolingDown`、失败计数/阈值、时间与 `reason`；列表包装为 `{ items }`。
- 浏览器结构：`apps/src/types/api-key.ts` — `AggregateApiRuntimeStatus`（约 59–69）与 Rust 的失败冷却字段逐项对应。
- 宽容解析：`apps/src/lib/api/normalize.ts` — `normalizeAggregateApiRuntimeStatus()`、`normalizeAggregateApiRuntimeStatusList()`（约 1034–1067）接受 camelCase/snake_case，并在缺失 ID 时丢弃条目。
- 配置列表另含 `balanceQueryEnabled`、最近余额刷新时间/状态/错误/JSON：`apps/src/types/api-key.ts` — `AggregateApi`（约 25–57）。这些是余额配置与最近快照，**不是**现有路由运行态。

**[推论]** 零余额需要其自身的 typed runtime-status 契约和自己的 list/reset 命令标识；不能向 `AggregateApiRuntimeStatus` 填入“零余额”而复用 `isCoolingDown`、`consecutiveFailures` 或 `reason`。这样才能保留“零余额解除只解除零余额状态”与“冷却解除清冷却 + policy action”的可观测差异。

## 已证实：现有冷却读取与手动重置的端到端命令链

### 共享 typed client 入口

`apps/src/lib/api/account-client.ts`：

- `accountClient.listAggregateApiRuntimeStatuses()`（约 822–827）调用 `invoke("service_aggregate_api_runtime_status_list", withAddr())`，再用 `normalizeAggregateApiRuntimeStatusList()` 返回强类型数组。
- `accountClient.resetAggregateApiRuntimeStatus(apiId)`（约 829–836）调用 `invoke("service_aggregate_api_runtime_status_reset", withAddr({ id: apiId }))`，缺少可解析结果会抛错，故 UI 不会把空成功当作有效解除。
- `withAddr()` 位于 `apps/src/lib/api/transport.ts` — `withAddr`（约 102–111），给同一 client 调用附加当前 service address；`id` 是现有 reset 的唯一业务参数。

### 桌面 Tauri 链

```text
AggregateApiPage / hook
  -> accountClient.list/resetAggregateApiRuntimeStatus()
  -> transport.invoke("service_aggregate_api_runtime_status_{list|reset}")
  -> Tauri command
  -> rpc_call_in_background("aggregateApi/runtimeStatus/{list|reset}")
  -> service RPC dispatch
  -> aggregate API service function
  -> gateway 冷却运行态
```

逐段证据：

1. `apps/src/lib/api/transport.ts` — `invoke()`（约 160–178）在 `isTauriRuntime()` 为真时执行 `tauriInvoke()`，并以 `unwrapRpcPayload()` 解包。
2. `apps/src-tauri/src/commands/aggregate_api.rs` — `service_aggregate_api_runtime_status_list()`、`service_aggregate_api_runtime_status_reset()`（约 19–37）分别转发 `aggregateApi/runtimeStatus/list` 与 `aggregateApi/runtimeStatus/reset`，后者发送 `{ id }`。
3. `apps/src-tauri/src/commands/shared.rs` — `rpc_call_in_background()` 是上述 command 的共有异步 RPC 桥；`apps/src-tauri/src/commands/registry.rs` — `invoke_handler!`（约 178–180）登记了两个 command，故不会出现 desktop-only 的未注册命令。
4. `crates/service/src/rpc_dispatch/aggregate_api.rs` — `try_handle()`（约 50–61）把两个 RPC method 分别分派到 `list_aggregate_api_runtime_statuses()` 与 `reset_aggregate_api_runtime_status()`。
5. `crates/service/src/aggregate_api.rs` — `list_aggregate_api_runtime_statuses()`、`reset_aggregate_api_runtime_status()`（约 1959–1975）对 reset 拒绝空 ID 与未知聚合 API；后者才进入 `gateway_reset_aggregate_api_runtime_status()`。
6. `crates/service/src/gateway/mod.rs` — `gateway_list_aggregate_api_runtime_statuses()`、`gateway_reset_aggregate_api_runtime_status()`（约 1300–1310）连到失败冷却状态。

### service-mode Web 链（同一业务 RPC）

```text
同一 accountClient / transport.invoke(...)
  -> transport.invokeWebRpc() 的 WEB_COMMAND_MAP
  -> POST runtimeCapabilities.rpcBaseUrl（Web 默认 /api/rpc）
  -> crates/web 的 rpc_proxy
  -> service /rpc（附加内部 RPC token 与可选 actor header）
  -> 同一 rpc_dispatch::aggregate_api::try_handle()
  -> 同一 service/gateway 函数
```

逐段证据：

1. `apps/src/lib/api/transport.ts` — `invoke()`（约 160–178）在非 Tauri runtime 转入 `invokeWebRpc()`；该函数从 `WEB_COMMAND_MAP` 查 descriptor 并调用 descriptor 的 `rpcMethod`（约 25–60）。
2. `apps/src/lib/api/transport-web-commands.ts` — `createWebCommandMap()`（约 16–29）合入 aggregate command map；`apps/src/lib/api/transport-web-commands/aggregate-api.ts` — `createAggregateApiWebCommands()`（约 3–27）将两个 Tauri 风格名称映射到相同的 `aggregateApi/runtimeStatus/list`、`aggregateApi/runtimeStatus/reset` method。
3. `apps/src/lib/api/transport.ts` — `postWebRpc()`（约 78–97）由 `loadRuntimeCapabilities()` 取得 `rpcBaseUrl` 后调用 `postJsonRpc()`；`apps/src/lib/api/rpc-http.ts` — `postJsonRpc()`（约 76–117）构造 JSON-RPC body，并把服务错误转换为可显示的异常。没有 aggregate API 的 raw `fetch` 分支。
4. `crates/web/src/main.rs` — `protected_app`（约 515–559）把 `/api/rpc` 路由到 `service_gateway::rpc_proxy` 并套上 `auth::web_auth_middleware`。
5. `crates/web/src/service_gateway.rs` — `rpc_proxy()`（约 297–346）向内部服务 RPC POST JSON，并只在代理内添加 `x-codexmanager-rpc-token`；accounts 模式时再添加角色和用户 ID header。浏览器不会得到该服务 token。
6. `crates/service/src/http/rpc_endpoint.rs` — `validate_axum_headers()`（约 252–302）要求 token、拒绝 cross-site 请求和非 loopback Origin；通过后由相同 RPC dispatch 处理。

**兼容性结论：** 零余额状态的读取/手动解除应沿用这条 `accountClient -> invoke -> Tauri command / Web command map -> 同一 RPC dispatch` 链。新增浏览器直连、绕过 `/api/rpc` 的 `fetch`，或只登记一个 runtime 的命令，都会破坏已经存在的双模式一致性。

## 已证实：权限边界

### UI 可用性门槛

- `apps/src/hooks/useRuntimeCapabilities.ts` — `useRuntimeCapabilities()` 调用 `resolveRuntimeCapabilityView()`；`apps/src/lib/runtime/runtime-capabilities.ts` — `resolveRuntimeCapabilityView()`（约 312–350）定义 `canAccessManagementRpc: mode !== "unsupported-web"`。
- `apps/src/app/aggregate-api/page.tsx` — `AggregateApiPage`（约 184–220）用 `canAccessManagementRpc && serviceStatus.connected` 计算 `isServiceReady`；冷却 reset 按钮用 `!isServiceReady || resetCooldownMutation.isPending` 禁用（约 985–992）。
- `apps/src/lib/api/transport.ts` — `postWebRpc()`（约 78–97）会在 `unsupported-web` 拒绝管理 RPC。因此新解除动作应保持同一 UI gating 和 transport 行为，不能因按钮显示而绕开 runtime 能力检查。

### 服务访问与角色边界

- service-mode Web 的 `/api/rpc` 先受 `crates/web/src/auth.rs` — `web_auth_middleware()`（约 591–615）保护；当 password/accounts 模式未认证时，API 返回 `401 { error: "web_auth_required" }`。`apps/src/lib/api/rpc-http.ts` — `postJsonRpc()`（约 94–112）会识别该错误并跳转 Web 登录。
- 内部服务端点再受 `crates/service/src/http/rpc_endpoint.rs` — `validate_axum_headers()`（约 252–302）的 token 和同源/loopback 限制；Web 代理而非浏览器保存 token（`crates/web/src/service_gateway.rs` — `rpc_proxy()`）。
- accounts 模式的角色由代理注入：`crates/web/src/service_gateway.rs` — `rpc_proxy()`（约 313–319）；服务由 `crates/service/src/http/rpc_endpoint.rs` — `rpc_actor_from_axum_headers()`（约 82–90）构造 `RpcActor`。
- `crates/service/src/rpc_dispatch/mod.rs` — `MEMBER_METHOD_ALLOWLIST`（约 194–241）**没有** `aggregateApi/runtimeStatus/list` 或 `aggregateApi/runtimeStatus/reset`；`ensure_method_allowed()` 和 `handle_request_with_actor()`（约 250–300）在分派 aggregate API 前执行该检查。因此在 `accounts` 模式，member 无法调用当前冷却读取或 reset，admin/system_admin 可以调用。
- `crates/service/src/rpc_actor.rs` — `RpcActor::is_admin()`（约 32–35）定义 `system_admin` 与 `admin`；同文件 `normalize_role()`（约 55–67）将缺失/未知角色归为 `system_admin`。同时，`member_method_allowed()` 在 password 模式直接允许所有 method（`crates/service/src/rpc_dispatch/mod.rs` — 约 242–255）。

**[推论]** 新的零余额 list/reset method 在 accounts 模式必须继续不进入 `MEMBER_METHOD_ALLOWLIST`，才满足“管理员手动解除”的既有角色语义；password 模式已被现有全局策略视为受认证的管理面，不能仅为这个功能改变它。

## 已证实：页面状态展示、确认交互与刷新

### 表格和现有冷却交互

- 当前唯一的运行态列是 `apps/src/app/aggregate-api/page.tsx` — 聚合 API 表格表头“运行状态”（约 789–807），表格共有 12 列（同文件 `AGGREGATE_API_TABLE_COLUMNS`）。它是可复用的**显示位置**，不是可复用的失败状态契约。
- `apps/src/hooks/useAggregateApiRuntimeStatuses.ts` — `useAggregateApiRuntimeStatuses(enabled)`：以 query key `aggregate-api-runtime-status` 每 2 秒轮询、`staleTime: 1_000`、后台不轮询；以 API ID 聚合为 `Map<string, AggregateApiRuntimeStatus[]>`，并每秒更新本地 `nowSeconds`。
- `apps/src/app/aggregate-api/page.tsx` — `AggregateApiPage`（约 216–219）只在页面活跃且服务可用时启用该 hook；`useDesktopPageActive()`（`apps/src/hooks/useDesktopPageActive.ts`）以 shell path 确认 `/aggregate-api/` 活跃。
- 同页表格行（约 824–1032）过滤 `isCoolingDown && cooldownUntil > now` 的条目：显示倒计时、失败计数、上游模型、原因、最后失败和截止时间。一个 API 可有多个模型冷却，页面以 `coolingStatuses.length` 汇总显示。
- 当前解除按钮只在 `isCoolingDown` 时出现；点击只设置 `resetCooldownApi`，并不立即调用 RPC（同页约 985–992）。
- `resetCooldownMutation`（同页约 456–467）成功后只失效 `aggregate-api-runtime-status`，显示“已解除冷却，API 已重新加入路由候选”；错误经 `getAppErrorMessage()` toast，确认目标清空。
- `apps/src/components/modals/confirm-dialog.tsx` — `ConfirmDialog` 的 `handleConfirm()`（约 49–74）会在 `onConfirm()` 未返回 `false` 时关闭，并在等待期间禁用取消/确认及 pointer dismissal。因此现有构件可以复用为零余额解除的确认壳，错误需继续通过 mutation 的 `onError` 明确反馈。
- 既有冷却文本已在三种 locale 中成套存在：`apps/src/lib/i18n/messages/sections/en-aggregate-api.ts`、`ko-aggregate-api.ts`、`ru-aggregate-api.ts` — 运行状态/冷却/解除相关 key（约 101–114）。零余额的说明、按钮、成功与失败文本需要同样覆盖三处，不能复用“冷却”文案而造成错误归因。

### 余额刷新与运行态刷新是不同机制

- `apps/src/app/aggregate-api/page.tsx` — `balanceMutation`（约 468–479）和 `refreshAllBalancesMutation`（约 481–510）调用既有 `accountClient.refreshAggregateApiBalance()`；两者最终只失效 `aggregate-apis`。
- 同页 `balanceEnabledApiIds`（约 339–342）只纳入 `balanceQueryEnabled` 配置项；工具栏“刷新余额”按钮（约 583–594）和行内刷新按钮（约 930–933）复用这套 mutation。
- 冷却运行态 query 与配置列表不同：配置查询 `queryKey: ["aggregate-apis"]`、`staleTime: 60_000`（同页约 221–227），运行态 query 用独立 key 和 2 秒轮询。

**[推论]** 现有余额刷新后立即刷新的是配置快照而非运行态；规划零余额 UI 时，不应把 `lastBalanceJson` 或本地显示结果当作路由禁用真相。零余额运行态应由服务返回并以独立 query 更新；若产品要求“手动刷新余额后马上更新禁用标记”，这需要在设计中明确其独立 query 的失效/刷新时机，而不能假定当前 `aggregate-apis` 失效会完成它。

## 可直接复用与必须保持独立的边界

| 可复用的既有面 | 锚点 | 允许的复用范围 |
| --- | --- | --- |
| typed client 与正常化模式 | `apps/src/lib/api/account-client.ts` — runtime status methods；`apps/src/lib/api/normalize.ts` — runtime normalizers | 新运行态的 TS 接口、normalizer、list/reset client wrapper应遵循该模式；没有必要引入 raw HTTP client。 |
| 双 runtime transport 注册形状 | `apps/src-tauri/src/commands/aggregate_api.rs` — `service_aggregate_api_runtime_status_*`; `apps/src/lib/api/transport-web-commands/aggregate-api.ts` — `createAggregateApiWebCommands`; `apps/src-tauri/src/commands/registry.rs` — `invoke_handler!` | 同一业务 RPC 名称必须同时有 Tauri wrapper、registry 和 Web descriptor。 |
| React Query 的轮询、mutation、反馈结构 | `apps/src/hooks/useAggregateApiRuntimeStatuses.ts`；`apps/src/app/aggregate-api/page.tsx` — `resetCooldownMutation` | 可复用启用条件、轮询方式、query key 隔离、pending disable、`getAppErrorMessage()` 和 toast/失效流程。 |
| 运行状态表格位置、Tooltip、ConfirmDialog | `apps/src/app/aggregate-api/page.tsx` — 运行状态单元格；`apps/src/components/modals/confirm-dialog.tsx` — `ConfirmDialog` | 可在同一物理列中展示另一种明确命名的临时排除原因，并复用确认组件。 |
| service/Web 权限边界 | `crates/service/src/rpc_dispatch/mod.rs` — `ensure_method_allowed`; `crates/web/src/auth.rs` — `web_auth_middleware`; `crates/service/src/http/rpc_endpoint.rs` — `validate_axum_headers` | 新管理 RPC 应被相同 Web session、内部 token、origin 与 accounts-mode admin gate 保护。 |

| 必须保持独立的面 | 原因与锚点 |
| --- | --- |
| `AggregateApi.status` 与“启用”开关 | 这是持久配置；`apps/src/types/api-key.ts` — `AggregateApi.status`，`apps/src/app/aggregate-api/page.tsx` — `toggleMutation`。 |
| `AggregateApiRuntimeStatus`、`aggregateApi/runtimeStatus/list`、`aggregateApi/runtimeStatus/reset` | 是失败冷却、按 model 聚合，reset 会清失败冷却与 policy action；`crates/core/src/rpc/types.rs` — `AggregateApiRuntimeStatus`，`crates/service/src/gateway/mod.rs` — `gateway_reset_aggregate_api_runtime_status()`，`crates/service/src/gateway/routing/tests/aggregate_api_cooldown_tests.rs` — `aggregate_api_cooldown_snapshot_and_reset_clear_policy_action()`。 |
| `AggregateApiHealthState` / 主动探测开关 | 是独立健康监测契约和查询/变异；`apps/src/types/api-key.ts` — `AggregateApiHealthState`，`apps/src/app/aggregate-api/page.tsx` — `activeProbeMutation`。 |
| 当前“冷却中”文案和成功 toast | 其文义承诺“连续失败”与重入候选；`apps/src/lib/i18n/messages/sections/en-aggregate-api.ts` — cooldown 相关 key。零余额必须给管理员真实的排除原因，不能误标为冷却。 |

## 具体测试场景与现有落点

以下是规划应覆盖的可观察契约；这里只记录测试证据与场景，不新增测试。

1. **失败冷却不回归。**
   - 位置：`crates/service/src/gateway/routing/tests/aggregate_api_cooldown_tests.rs` — `aggregate_api_cooldown_snapshot_and_reset_clear_policy_action()`、`aggregate_api_cooldown_isolated_by_upstream_model()`、`aggregate_api_cooldown_success_only_clears_matching_model()`。
   - 场景：零余额状态存在或被手动解除时，五次失败的冷却阈值、按上游模型隔离、cooldown reset 清 policy action 的现有结果不变；零余额解除绝不能清这些失败冷却条目。

2. **新零余额运行态的服务契约。**
   - 现有相邻集成夹具：`crates/service/tests/rpc.rs` — `RpcTestContext`（约 1–120）。已在此文件对 `aggregateApi/` 方法作定向搜索，未发现现有 aggregate API RPC case；适合作为新 RPC 端到端请求/错误响应的归属位置。
   - 场景：只有“成功取得且判定为零”的 API 出现在零余额运行态；无余额能力、缺值/未知、查询失败均不出现；空 ID/未知 ID 的解除返回可显示错误；解除仅移除对应零余额排除，且 API 若仍被配置停用、失败冷却或其他规则排除则不会被错误宣传为可路由。

3. **accounts-mode 角色与 Web transport。**
   - 位置：`crates/service/src/rpc_dispatch/mod.rs` — `MEMBER_METHOD_ALLOWLIST`、`handle_request_with_actor()`；`crates/service/src/http/rpc_endpoint_tests.rs` — `axum_rpc_accepts_authenticated_body_within_the_limit()`、`axum_rpc_rejects_unauthenticated_large_body_without_reading_it()`；`crates/web/src/service_gateway_tests.rs` — `rpc_proxy_rejects_body_over_the_bounded_upload_limit()`。
   - 场景：新零余额 list/reset 在 accounts 模式对 member 返回 `permission_denied`，admin 可访问；未经 Web session 的 `/api/rpc` 拒绝；浏览器请求不需要也不暴露内部 RPC token；新 method 由 Web command map 发送到 `/api/rpc` 而非另开端点。

4. **frontend 运行态显示与解除确认。**
   - 已有回归落点：`apps/tests/aggregate-capabilities.test.mjs` — `aggregate API page preserves cooldown countdown and reset controls`（约 86–106）当前以源代码形状确保 hook、倒计时、reset client 与 `ConfirmDialog` 没被移除。
   - 场景：零余额项目显示与冷却不同的状态/原因；没有零余额条目时不显示解除入口；点击解除先出现明确说明的确认框；确认后发送零余额专用 typed client 方法、只刷新其运行态 query、成功/失败 toast 可见；冷却 reset 仍只调用旧方法。

5. **service-mode Web UI 实际路径。**
   - 位置：`apps/tests/aggregate-api-usage-refresh.spec.ts` — `aggregate API usage refreshes while active and resumes after keep-alive navigation`（约 119–398）。该 Playwright 测试已 mock `/api/runtime`、`/api/rpc`、`aggregateApi/list` 和 `aggregateApi/runtimeStatus/list`，并验证页面活跃/切换/恢复时的 refresh 行为。
   - 场景：在该 Web mock 链中返回零余额状态，验证行状态与确认操作的 JSON-RPC method/`id` 参数、成功后状态消失、失败消息可见；同时验证离开 `/aggregate-api/` 后运行态轮询停止、返回页面恢复，保持现有 `isQueryEnabled` 生命周期。

6. **显式启停兼容性。**
   - 位置：`apps/src/app/aggregate-api/page.tsx` — `toggleMutation` 与“启用”`Switch`；现有 Playwright 表格行断言位于 `apps/tests/aggregate-api-usage-refresh.spec.ts`（约 282–322）。
   - 场景：API 被标为零余额暂时排除时，配置开关仍反映持久 `status`；管理员解除零余额后，不会自动把 `status: disabled` 的 API 显示或路由为启用。

## 未证实事项（需在后续服务端调研/设计决定）

1. 本调研未检查余额适配器及路由候选筛选实现，故未证实零余额标记的内存/持久化生命周期、成功非零余额何时自动恢复、或服务重启后的语义。这正是 `prd.md` 的 `Open Questions` 所列问题。
2. 尚无已证实的零余额 RPC、TS 类型、query key 或 UI 文案；本文提到的“独立”是对当前冷却/配置/健康契约的隔离约束，不是这些新符号已存在的陈述。
3. `accounts` 模式的 admin-only 结论有明确代码依据；password 模式按当前 `member_method_allowed()` 策略允许所有方法。是否将“管理员”在 password 模式进一步细分不是本任务可从现有代码证实的需求，且不应随零余额功能改变现有认证策略。
