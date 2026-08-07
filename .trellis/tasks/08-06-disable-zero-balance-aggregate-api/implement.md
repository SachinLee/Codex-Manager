# 实施计划：聚合 API 零余额路由禁用

## 执行姿态

先建立 Core storage 与服务路由的可观察回归契约，再接入跨端 RPC 和 UI。不得用测试替代真实候选筛选；所有测试必须验证对外可见的路由、状态或权限行为。

## 实施单元

- [ ] **Unit 1：持久化零余额状态与原子转换**

**目标：** 增加可跨服务重启保存的零余额阻断/手动放行状态，并让余额缓存与状态转换保持一致。

**Requirements：** R1、R2、R3、R4；AC 1、2、6、7。

**依赖：** 无。

**文件：**
- Create: `crates/core/migrations/132_aggregate_api_zero_balance_route_state.sql`（实施前重核当前下一个未占用迁移号）
- Create: `crates/core/src/storage/aggregate_api_zero_balance.rs`
- Modify: `crates/core/src/storage/mod.rs`
- Modify: `crates/core/src/storage/aggregate_apis.rs`
- Modify: `crates/core/src/storage/aggregate_apis_sql.rs`
- Test: `crates/core/src/storage/aggregate_api_zero_balance.rs`

**方案：**
- 注册新 migration 与 Core storage module；表以 `aggregate_api_id` 主键、`ON DELETE CASCADE` 关联 `aggregate_apis`，并保留 `zero_balance_blocked` / `manually_released`、最近零余额观察时间、解除时间和更新时间。
- 定义 typed storage state 与受限的查询、列出 blocked IDs、列出所有 UI 状态、手动标记 released、清除状态等方法，全部使用参数化 SQL。
- 将余额结果缓存写入与“精确零写 blocked、正余额删除、其它结果不变”的 state transition 合为一个 transaction。存储失败必须返回错误，禁止继续吞掉写入失败而报告刷新成功。
- 在同一 transaction 内确认父 API 仍存在、余额 cache 更新恰好影响一行，并基于提交时的 `balance_query_enabled` 决定是否允许 zero-balance transition；并发删除必须返回 not-found/提交失败，关闭余额查询与其 state 删除必须阻止晚到零余额刷新重建 block。
- `balance_query_enabled` 转为 false 时，在配置更新 transaction 内清理该 API 的 zero-balance state，兑现“无余额查询不参与”。
- 不从旧 `last_balance_*` 回填状态；不存 secret、上游 body 或原始错误。

**模式参考：** `crates/core/src/storage/aggregate_api_health.rs::save_aggregate_api_health_observation` 的 transaction/upsert 形状；`crates/core/src/storage/aggregate_apis.rs::update_aggregate_api_balance_result` 的当前余额缓存边界；`crates/core/src/storage/mod.rs` 的 migration 注册。

**测试场景：**
- 有效 `remaining == 0` 原子保存余额快照与 `zero_balance_blocked`。
- 有效正余额原子清除 blocked 或 manually-released 状态。
- 查询错误、无效快照、缺少余额值和负值不会创建或清除既有状态。
- 手动解除写入 `manually_released`，持久化 reopen 后仍可读；之后有效零余额刷新重新改为 blocked。
- 关闭 `balance_query_enabled` 清理状态；删除 aggregate API 触发外键级联清理。
- 在途刷新与关闭余额查询交错时，晚到的有效零余额不会重新创建 state；与删除 API 交错的零、正、未知刷新都不会伪报成功。
- migration 反复初始化可用且 foreign-key check 无违规。

**验证：** 内存 SQLite 初始化与 reopen 能观察到所有状态转换，且余额缓存/路由状态不会出现半写入。

- [ ] **Unit 2：服务余额判定与最终候选路由门**

**目标：** 在所有余额刷新来源上应用同一转换，并在真正发送请求前排除当前零余额 blocked API。

**Requirements：** R1、R3、R4；AC 1、2、5、6、7。

**依赖：** Unit 1。

**文件：**
- Modify: `crates/service/src/aggregate_api.rs`
- Modify: `crates/service/src/gateway/upstream/protocol/aggregate_api.rs`
- Test: `crates/service/src/aggregate_api_tests.rs`
- Test: `crates/service/src/gateway/upstream/protocol/aggregate_api_tests.rs`
- Test: `crates/service/src/gateway/routing/tests/aggregate_api_cooldown_tests.rs`

**方案：**
- 在 `refresh_aggregate_api_balance` 内，根据 typed snapshot 的精确语义调用 Unit 1 的原子存储方法，使 RPC、配额刷新与后台 polling 共用该逻辑；不在三个调用方各自实现分支。
- 在余额错误产生处建立安全分类：非成功 HTTP 只保留状态码/稳定分类，成功响应中的无效原因在写入 snapshot 前归一为固定文案。之后 `last_balance_error`、refresh result、batch log、RPC 和 UI toast 只传递该安全值；不存储或返回上游 body、message/error、header、token、鉴权参数或账户标识。
- 在 `proxy_aggregate_request` 的最终候选筛选阶段一次加载 zero-balance blocked API IDs，在既有 cooldown/health 短路之后、日预算筛选之前排除零余额候选；保留候选顺序、模型路由、日预算与现有短路优先级。
- 剩余候选全部因 zero-balance 被排除时才返回明确、不含敏感细节的 503，并写安全的结构化服务日志。混合原因不扩展现有请求日志模型或伪造 attempted IDs；零余额绝不记录成 cooldown。
- 不修改 `gateway_is_aggregate_api_in_cooldown`、`AggregateApiRuntimeStatus` 或 `policy_action`。也不将一次上游请求成功当作余额恢复。

**模式参考：** `crates/service/src/gateway/upstream/protocol/aggregate_api.rs::proxy_aggregate_request` 的 candidate filtering 与 all-excluded 请求日志；`crates/service/src/aggregate_api.rs::refresh_aggregate_api_balance` 的三种余额模板结果归一化。

**测试场景：**
- 两个同协议且其余条件相同的 candidate 中，排序靠前的 blocked API 被跳过，第二个收到请求，排序不被重写。
- 全部 candidate 因零余额阻断时不发上游请求，返回 503 且原因不冒充 cooldown。
- 一次成功正余额刷新使该 API 在其它 gate 允许时恢复候选；查询失败不会解除此前 block。
- 手动 released API 可通过本门，但仍被 manual disabled、模型路由禁用、legacy failure cooldown 或 health gate 独立排除。
- 既有五次失败冷却、按模型隔离、cooldown reset 清 policy action 的契约不因本功能改变。
- 上游以错误 body 或 invalid 字段回显伪造 token 时，该 token 不会进入 `last_balance_*`、零余额状态表、refresh RPC、Aggregate API list、request log、batch log 或桌面/Web toast；管理员只得到稳定的安全分类。
- 所有候选均处于 cooldown/health、仅零余额、或在零余额筛选后均超日预算时，分别保留既有 503、零余额 503、既有 429 的优先级和安全日志语义。

**验证：** 所有刷新触发路径经过同一判定函数；候选过滤对零余额与失败冷却可分别观察和解除。

- [ ] **Unit 3：零余额状态的管理员 RPC 与双运行时 transport**

**目标：** 提供独立、类型安全且受既有访问控制保护的状态 list/reset 能力。

**Requirements：** R2、R4；AC 3、4、5。

**依赖：** Unit 1、Unit 2。

**文件：**
- Modify: `crates/core/src/rpc/types.rs`
- Modify: `crates/service/src/aggregate_api.rs`
- Modify: `crates/service/src/lib.rs`
- Modify: `crates/service/src/rpc_dispatch/aggregate_api.rs`
- Modify: `apps/src-tauri/src/commands/aggregate_api.rs`
- Modify: `apps/src-tauri/src/commands/registry.rs`
- Modify: `apps/src/lib/api/transport-web-commands/aggregate-api.ts`
- Modify: `apps/src/lib/api/account-client.ts`
- Modify: `apps/src/lib/api/normalize.ts`
- Modify: `apps/src/types/api-key.ts`
- Test: `crates/service/src/rpc_dispatch/aggregate_api_tests.rs`
- Test: `crates/service/tests/rpc.rs`
- Test: `crates/service/src/http/rpc_endpoint_tests.rs`

**方案：**
- 新增 `AggregateApiZeroBalanceStatus`，含 API ID、state、最近观察/解除/更新时间；列表返回 `{ items }`，手动解除返回单项状态。字段使用 camelCase RPC 序列化，normalizer 兼容现有 camel/snake payload 形状。
- 定义 `aggregateApi/zeroBalanceStatus/list` 与 `aggregateApi/zeroBalanceStatus/reset`，并保持 Tauri command 的 underscore 命名与 Web command map 的相同 camelCase RPC method。reset 在 service 层先验证非空 ID 和 API 存在；仅将现有 blocked 状态标为 manually released，对正常或已放行 API 幂等返回且不创建状态行。
- reset 仅转换此表状态；绝不调用 runtime cooldown/health reset，不修改 `last_balance_*`、`status` 或 API secret。
- 保持 Web auth、内部 RPC token、origin/loopback 检查和 accounts-mode admin 限制；新 method 不加入 `MEMBER_METHOD_ALLOWLIST`。

**模式参考：** `aggregateApi/runtimeStatus/list|reset` 的 dispatch、Tauri、Web map、`accountClient` 和 normalizer 全链；`ensure_method_allowed` 的 admin-only aggregate API 管理边界。

**测试场景：**
- list 只返回可公开的 zero-balance state，绝不含余额查询凭据、secret 或原始上游响应。
- reset 拒绝空/未知 ID；blocked API 成功变为 manually released；正常或已放行 API 的重复调用幂等且不制造虚假状态。
- reset 不清 legacy cooldown/health/config disabled；这些剩余 gates 仍阻止路由。
- accounts 模式 member 得到 `permission_denied`，admin 可调用；password-mode 维持当前全局认证策略。
- desktop command 与 Web command map 都映射到同一 RPC method 与 `{ id }` 参数。

**验证：** 同一 typed client 在 Tauri 和 service-mode Web 均能获得相同状态和错误语义，且权限绕过不可行。

- [ ] **Unit 4：管理页状态展示、解除交互与本地化**

**目标：** 让管理员识别零余额路由排除、理解手动放行语义，并安全地执行解除。

**Requirements：** R2、R3、R4；AC 3、4、5。

**依赖：** Unit 3。

**文件：**
- Create: `apps/src/hooks/useAggregateApiZeroBalanceStatuses.ts`
- Modify: `apps/src/app/aggregate-api/page.tsx`
- Modify: `apps/src/lib/i18n/messages/sections/en-aggregate-api.ts`
- Modify: `apps/src/lib/i18n/messages/sections/ko-aggregate-api.ts`
- Modify: `apps/src/lib/i18n/messages/sections/ru-aggregate-api.ts`
- Create: `apps/tests/aggregate-api-zero-balance.test.mjs`
- Create: `apps/tests/aggregate-api-zero-balance.spec.ts`

**方案：**
- 以独立 React Query key 轮询 zero-balance status，并按 API ID 合并到既有配置列表；使用与 cooldown hook 相同的页面活跃和服务可用 gating，不高频刷新完整配置列表。
- 扩展“运行状态”单元格：blocked 显示明确的零余额原因、观察时间和解除按钮；manually released 显示仍为零但已手动放行。冷却和健康状态仍保留自己的 badge、Tooltip 和按钮。
- 使用 `ConfirmDialog` 明确提示解除仅撤销零余额排除，若下一次成功余额查询仍为零会再次禁用；mutation 通过新的 typed client 方法，显示专用 toast 和 `getAppErrorMessage()` 错误。
- 任何单项或批量余额刷新结算后，同时失效 `aggregate-apis` 与 zero-balance-status query；手动解除只刷新后者。
- UI 不展示原始余额查询错误、token、header 或上游 body；沿用 `isServiceReady` 禁用和 loading 状态。
- 状态不能只依赖 badge 颜色：提供完整文本原因和可访问名称。新的解除按钮保持原生 Button 语义、键盘可达、最小 24×24 CSS px 命中区与可见 focus；确认框关闭后把焦点返回触发按钮。

**模式参考：** `apps/src/hooks/useAggregateApiRuntimeStatuses.ts` 的 query 生命周期，`apps/src/app/aggregate-api/page.tsx` 的 `resetCooldownMutation` / `balanceMutation` / `ConfirmDialog`，以及 `apps/tests/aggregate-api-usage-refresh.spec.ts` 的 `/api/rpc` mock 链。

**测试场景：**
- blocked、manually released、无状态和与 cooldown 同时存在时，页面用不同文案显示状态且只出现对应操作。
- 点击解除先显示确认，确认后只调用 `aggregateApi/zeroBalanceStatus/reset` 并在成功时移除 blocked 显示；失败显示可读错误。
- 余额刷新后新 zero-status 查询及时更新；离开聚合 API 页面后轮询停止，返回后恢复。
- service-mode Web mock 覆盖 `/api/runtime`、`/api/rpc` 的 list/reset payload 与成功/失败分支；现有 cooldown reset 仍发旧 RPC。
- i18n keys 三个现有 aggregate API section 均完整，normalizer 对空/未知项采用安全默认而不渲染错误的解除入口。
- 键盘操作可聚焦并激活解除按钮；确认框聚焦被限制、Escape/取消可退出且关闭后焦点回到触发按钮；屏幕阅读器能从文本状态与动态 toast 得知 blocked、手动放行、成功和失败。

**验证：** 管理员在桌面和 Web 都可看到真实原因、执行不影响其它状态的解除，并获得明确成功/失败反馈。

## 交叉层检查与验证顺序

1. Core storage 定向测试：确认 migration、事务、状态持久化、级联删除与状态机。
2. Service/gateway 定向测试：确认候选过滤、全池 503、正余额恢复、未知结果保持以及与 cooldown/health/config 的组合。
3. RPC/权限测试：确认 input 校验、admin-only access 和 Tauri/Web 映射一致。
4. 前端行为测试：确认 RPC payload、页面生命周期、确认对话框、错误反馈和 locale。
5. 按仓库规则执行 `cargo test --workspace`、`pnpm -C apps run test:runtime`、`pnpm -C apps run build`；若 desktop command registry 或静态导出有改动，再执行 `pnpm -C apps run build:desktop`。
6. 人工 smoke：在服务模式和 Tauri 各选择一个余额查询 API，依次验证有效零余额阻断、手动放行、服务重启后保留、有效正余额自动恢复、失败刷新不释放。

## 风险、回滚与完成门槛

| 风险 | 缓解 |
| --- | --- |
| 将未知余额误判为零 | 只接受成功、有效、精确零的 typed snapshot；测试所有未知分支。 |
| 手动解除误清其它路由状态 | 使用独立 SQLite state 与 RPC，回归验证 cooldown/health/config 不变。 |
| 重启或陈旧缓存重新阻断手动放行 | 不从 `last_balance_*` 启动重建；持久化 `manually_released`，仅新刷新事件切换状态。 |
| RPC 只在一个运行时可用或越权 | 同步 registry/Web map/client，并锁定 accounts-mode admin-only 测试。 |
| 余额刷新与状态写入分裂 | 一个 storage transaction；提交失败即报告刷新失败且不改状态。 |

完成前必须满足：所有新状态在 UI、RPC、存储和路由中语义一致；每个解除操作只影响零余额状态；没有 secret/原始上游内容进入日志或响应；既有人工启停、冷却、health、日预算、模型路由回归保持原行为。
