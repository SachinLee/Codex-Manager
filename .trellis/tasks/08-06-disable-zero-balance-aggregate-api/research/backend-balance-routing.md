# 后端余额与聚合路由调研

> 范围：`crates/service/` 的聚合 API 余额查询/缓存、刷新触发、路由候选、运行时策略与失败冷却；不包含前端。本文只记录已验证的仓库事实、边界和规划约束，不包含实现代码。

## 已验证的数据流

### 1. 配置、上游响应与持久化快照

1. 余额查询能力和最近一次结果属于持久化的 `aggregate_apis` 行：
   - schema：`crates/core/migrations/054_aggregate_api_balance_query.sql:1-15`；
   - Rust 模型：`crates/core/src/storage/mod.rs::AggregateApi`（`1504-1531`）；
   - 字段为 `balance_query_enabled`、模板/端点配置，以及 `last_balance_at`、`last_balance_status`、`last_balance_error`、`last_balance_json`。
2. 服务端读取聚合 API 时把这些字段映射到公开的 `AggregateApiSummary`：`crates/service/src/aggregate_api.rs::list_aggregate_apis`（`1908-1956`）→ `crates/core/src/rpc/types.rs::AggregateApiSummary`（`846-875`）。余额访问 token 不在该 summary 中；该 token 来自 `AggregateApiWithSecrets` 的独立 `balance_access_token` 字段（`crates/core/src/storage/mod.rs::AggregateApiWithSecrets`，`1533-1538`）。
3. 唯一的余额查询入口是 `crates/service/src/aggregate_api.rs::refresh_aggregate_api_balance`（`2558-2636`）：
   - 读取 `find_aggregate_api_with_secrets_by_id`，在 `balance_query_enabled == false`、缺少 API、缺少 provider secret 或模板配置不合法时返回 `Err`；这些早期错误不会调用 `update_aggregate_api_balance_result`。
   - 按 generic / new_api / custom 模板进入 `query_generic_balance`、`query_new_api_balance`、`query_custom_balance`（`1148-1286`）。generic 会在指定错误上从 `/user/balance` 回退到 `/v1/usage`（`1160-1188`）。
   - 三个提取器都产出 typed `AggregateApiBalanceSnapshot`（`crates/core/src/rpc/types.rs::AggregateApiBalanceSnapshot`，`1151-1161`）：`is_valid`、`remaining: Option<f64>`、`unit`、`total`、`used` 等。generic/custom 在有效响应缺少 `remaining` 时返回错误；new_api 在 `success` 但缺少 `data.quota` 时返回错误（`crates/service/src/aggregate_api.rs::extract_generic_balance`，`975-1075`；`extract_new_api_balance`，`1077-1104`；`extract_custom_balance`，`1107-1145`）。
4. 查询结果至本地状态的表示是三分的，不能把 `balance: None` 当作零：

| 上游/提取结果 | `AggregateApiBalanceRefreshResult` | 已写入的余额缓存 |
| --- | --- | --- |
| snapshot 有效（包括 `remaining == 0.0`） | `Ok { ok: true, balance: Some(snapshot), message: None }` | `last_balance_status = "success"`，JSON 化的 snapshot |
| HTTP/解析/缺字段等 `Err` | `Ok { ok: false, balance: None, message: Some(...) }` | `last_balance_status = "failed"`，`last_balance_json = NULL` |
| 上游返回了可解析但 `is_valid == false` 的 snapshot | `Ok { ok: false, balance: Some(snapshot), message: Some(...) }` | `last_balance_status = "failed"`，仍写入 JSON snapshot |
| 查询前置条件不成立（如未启用查询） | `Err(...)` | 本次不更新，保留旧缓存 |

证据：`crates/service/src/aggregate_api.rs::refresh_aggregate_api_balance`（`2558-2636`）和 `crates/core/src/storage/aggregate_apis.rs::Storage::update_aggregate_api_balance_result`（`579-593`）。存储写入使用 `let _ = ...` 忽略写失败，因此刷新返回 `ok` 不能证明快照已经成功持久化；规划必须决定临时路由状态以“本次已知结果”还是“已成功持久化的结果”为事实来源。

5. 目前提取器没有拒绝负数：`remaining` 来自 JSON number 或 new_api quota 换算，未作非负校验（同上三个 extractor）。零也没有被特殊处理。唯一已验证的非负过滤在**配额展示/汇总**而非路由中：`crates/service/src/quota/read.rs::balance_json_usd`（`97-101`）仅接受 finite 且 `>= 0.0`，`summarize_aggregate_balance_usd`（`103-114`）据此汇总。另一个 token 估算函数把零视为已知值、把负值视为无效：`crates/service/src/quota/model_pricing.rs::estimate_remaining_tokens_from_usd_with_catalog`（`194-208`）。

### 2. 刷新触发时机

`refresh_aggregate_api_balance` 是三种已发现触发路径的汇合点：

1. 单个手动刷新 RPC：`crates/service/src/rpc_dispatch/aggregate_api.rs` 的 `"aggregateApi/refreshBalance"` 分支（`250-253`）。
2. 手动配额来源刷新：`crates/service/src/quota/read.rs::refresh_quota_sources`（`1397-1434`）从 `Storage::list_balance_query_aggregate_api_ids` 取得启用余额查询的 ID 后逐个调用。该 storage 查询只过滤 `balance_query_enabled = 1`，**不**过滤 API `status`：`crates/core/src/storage/aggregate_apis.rs::balance_query_aggregate_api_ids_sql`（`1298-1303`）。
3. 后台轮询：`crates/service/src/lifecycle/startup.rs::start_server`（`66-85`）调用 `usage_refresh::ensure_usage_polling`；它启动 `usage_polling_loop`（`crates/service/src/usage/refresh/mod.rs:248-253`），该 loop 在每轮调用 `refresh_usage_and_aggregate_balances_for_polling_cycle`（`crates/service/src/usage/refresh/runner.rs:29-58`）。该函数随后调用 `refresh_aggregate_api_balances_for_polling_cycle`（`crates/service/src/usage/refresh/batch.rs:115-168`）。后台轮询候选则额外要求 `balance_query_enabled && status == active`（`build_aggregate_api_balance_refresh_ids`，`171-176`）。默认间隔 600 秒、jitter 上限 5 秒、轮询失败退避上限 1800 秒，最小配置间隔 30 秒：`crates/service/src/usage/usage_scheduler.rs:5-11`；启停和间隔从环境配置加载：`crates/service/src/usage/refresh/settings.rs::reload_background_tasks_from_env`（`260-283`）。

**规划含义：** 零余额状态的唯一写入/清除钩子应放在这个共同刷新结果边界，不能仅接在 RPC 或轮询路径，否则另一条刷新路径会留下过期状态。

## 当前路由链与余额的缺口

### 候选构造与最终筛选

1. 协议候选由 `crates/service/src/gateway/upstream/protocol/aggregate_api.rs::resolve_aggregate_api_rotation_candidates`（`1279-1311`）从 `Storage::list_active_aggregate_apis_by_provider_type` 取得；storage SQL 的活跃条件仅为 `status == active`：`crates/core/src/storage/aggregate_apis_sql.rs::AGGREGATE_API_ACTIVE_STATUS_CONDITION`（`30-31`），查询实现见 `crates/core/src/storage/aggregate_apis.rs::list_active_aggregate_apis_by_provider_type`（`360-381`）。
2. `crates/service/src/gateway/upstream/proxy.rs::resolve_aggregate_candidates_for_route`（`304-327`）可将显式 API 提升到首位；`apply_aggregate_model_filter`（`345-387`）再根据 enabled 的 Model V2 route 过滤并把 route 的 `upstream_model` 写入 candidate 的 `model_override`。此处已保持“配置 active”和“模型 route enabled”的独立语义。
3. 最终请求路径在 `crates/service/src/gateway/upstream/protocol/aggregate_api.rs::proxy_aggregate_request`（候选过滤段 `1410-1462`）中，对每个 candidate 调用 `gateway_is_aggregate_api_in_cooldown`。被现有冷却/健康策略排除的 candidate 不会到达日预算筛选及上游请求；全被排除时记录 503 与已跳过的 ID。
4. `gateway_is_aggregate_api_in_cooldown` 是**失败相关状态的复合谓词**：`crates/service/src/gateway/mod.rs`（`1249-1260`）返回“内存 aggregate API cooldown”或“持久化 health routing block”。该谓词和现有日志文本都以 cooldown 为语义，不能直接承担余额为零状态。
5. 余额缓存字段当前不参与网关路由。针对 `crates/service/src/gateway/upstream/` 的 `balance_query_enabled` / `last_balance_status` / `last_balance_json` 搜索只命中 test fixture 的结构体初始化；候选解析、模型过滤和 `proxy_aggregate_request` 都没有读取这些字段。余额只在 quota read-model 使用，状态转换为 `ok/error/unknown` 的位置是 `crates/service/src/quota/read.rs::aggregate_source_balance_status`（`143-149`）。

因此当前的完整事实链到达 `last_balance_*` 持久化快照后即结束；没有“余额为零 → runtime state → 路由候选排除”的现有实现。

## 现有运行时状态、冷却与策略边界

### 失败冷却（必须与零余额分离）

- `crates/service/src/gateway/routing/aggregate_api_cooldown.rs` 使用 `OnceLock<Mutex<AggregateApiCooldownState>>`（`39-51`）保存**进程内**、以 `(api_id, upstream_model)` 为键的连续失败和 `cooldown_until`。
- 到 5 次失败进入 5 分钟冷却；30 秒频率进行清理，冷却过期且距上次失败超过 30 分钟才遗忘：常量与 `maybe_cleanup_expired_entries`（`11-70`），写入逻辑为 `record_aggregate_api_failure`（`198-242`）。成功按模型清除，人工 reset 按 API 清除全部模型：`clear_aggregate_api_cooldown`（`245-253`）与 `clear_aggregate_api_cooldowns`（`255-266`）。已有回归覆盖模型隔离和成功只清匹配模型：`crates/service/src/gateway/routing/tests/aggregate_api_cooldown_tests.rs:92-156`。
- 上游请求真正成功时清除该 candidate 的冷却；可冷却失败时记录失败：`crates/service/src/gateway/upstream/protocol/aggregate_api.rs`（`2252-2262`、`2381-2387`）。这证明“请求成功”目前只能恢复失败冷却，不应隐式变成余额状态的恢复条件。
- `AggregateApiRuntimeStatus` 明确是冷却模型：`is_cooling_down`、`consecutive_failures`、阈值、`cooldown_until`、`last_failure_at`、reason（`crates/core/src/rpc/types.rs:976-986`）。列表和 reset 仅委托冷却状态：`crates/service/src/aggregate_api.rs::list_aggregate_api_runtime_statuses` / `reset_aggregate_api_runtime_status`（`1959-1976`）→ `crates/service/src/gateway/mod.rs`（`1300-1310`）。

### health 与 runtime policy

- `aggregate_api_health::is_routing_blocked_with_storage`（`crates/service/src/aggregate_api_health.rs:299-334`）只有 API health config 已启用时才读取持久化 health state；source/model 任一为 `cooldown` 或 `unhealthy` 且未过期即可阻断。因此它与内存 failure cooldown 又是一个独立的阻断来源。
- `reset_aggregate_api_runtime_status` 仅清内存失败冷却及其 policy action，**不会**清 health state；相反 `aggregate_api_health::reset_health` 先持久化 reset health，再调用 runtime reset（`638-664`）。这是管理员手动操作必须保留的既有差异。
- 为了可观察性，失败冷却在 `sync_policy_action` 中写入一个进程内 `GatewayPolicyActionSummary`（`aggregate_api_cooldown.rs:75-111`），key 是 `(PolicyTargetKind::AggregateApi, api_id)`：`crates/service/src/gateway/routing/policy_action.rs::action_key`（`83-85`）。policy action 的 kind 固定为 `"cooldown"`、过期时清理，且一个 API 同时只能有这一条 key（`106-143`、`156-176`）。若将零余额复用为同一 action，会覆盖/清除 failure cooldown 的可观察性；不可这样复用。

### 重启和配置 reload

- 余额快照在 SQLite `aggregate_apis` 表，迁移和 `update_aggregate_api_balance_result` 的更新 SQL 见 `crates/core/migrations/054_aggregate_api_balance_query.sql:1-9` 与 `crates/core/src/storage/aggregate_apis_sql.rs::update_aggregate_api_balance_result_sql`（`163-171`），所以它会跨服务进程保存。
- 服务启动先执行 `gateway::reload_runtime_config_from_env`，再初始化 storage：`crates/service/src/lifecycle/startup.rs::start_server`（`66-74`）。该 reload 清空 aggregate API failure cooldown、policy actions、账号 cooldown 等运行时表：`crates/service/src/gateway/mod.rs::reload_runtime_config_from_env`（`479-490`）→ `aggregate_api_cooldown::clear_runtime_state`（`268-276`）。
- [推论，基于上述 `OnceLock` 进程内状态和启动 clear] 现有 failure cooldown / policy action 在服务重启后不保留；已持久化的余额结果仍保留。当前不存在零余额 runtime state，因此其重启后的恢复/重建/过期行为尚未由代码决定，必须在设计中显式裁决。

## 推荐复用点与禁止混用点（非实现方案）

1. **刷新结果归一化点：** 使用 `crates/service/src/aggregate_api.rs::refresh_aggregate_api_balance`（`2558-2636`）作为唯一的余额状态判定边界。它覆盖单项 RPC、配额批量刷新和后台轮询。触发条件必须是“本次查询成功且 snapshot 有效且 `remaining` 明确为零”；`balance: None`、`ok: false`、`remaining: None` 都必须是未知/失败而非零。需求只声明零，现有提取器又允许负数，因此负数不应被扩展解释为零。
2. **最终候选门：** 在 `proxy_aggregate_request` 的现有冷却筛选位置（`crates/service/src/gateway/upstream/protocol/aggregate_api.rs:1410-1462`）复用“在真正发请求前从 candidate vector 删除”的时序和日志/请求记录形状；零余额应使用独立谓词、独立 skip 原因和独立 all-excluded 反馈，不能塞进 `gateway_is_aggregate_api_in_cooldown`。
3. **管理员 API 链：** 复用 service-side 的 ID 验证与 typed RPC 分发形状：`aggregate_api.rs::reset_aggregate_api_runtime_status`（`1964-1976`）和 `rpc_dispatch/aggregate_api.rs::try_handle`（`35-61`）。但余额解除必须是**独立的服务操作/结果语义**，仅清零余额临时状态，不能调用现有 `runtimeStatus/reset` 后顺带清除 failure cooldown 或 health。
4. **授权边界：** 所有 RPC 在分发至 aggregate module 前执行 `ensure_method_allowed`：`crates/service/src/rpc_dispatch/mod.rs::handle_request_with_actor`（`293-305`）。`aggregateApi/*` 不在 member allowlist 中（`172-245`），所以在非 password-web-auth 模式下只有 admin actor 可调用；新解除方法应经过该相同链路，不能新增旁路。
5. **公开类型边界：** `crates/core/src/rpc/types.rs` 是现有 service transport 契约。零余额状态如需出现在列表/解除响应中，应从这里以不含 secret/raw upstream payload 的 typed 字段表达；不要复用只描述 failure cooldown 的 `AggregateApiRuntimeStatus` 而造成状态来源混淆。
6. **不应直接复用的 policy action：** `policy_action` 的单 key 和固定 `cooldown` kind 无法并存“失败冷却”和“余额为零”两个解释；它仅可作为“需要新的独立可观察性模型”的反例，不能当作零余额状态储存。

## 规划前必须裁决的未证实假设

1. 零余额临时状态是否只保留在进程内，还是需要基于持久化 `last_balance_*` 在重启后重建；当前实现两种语义均不存在。若选择进程内，应明确服务 restart / gateway env reload 会释放它；若选择重建，应定义 `last_balance_at` 的最大新鲜度与更新失败后如何处理陈旧零快照。
2. 在余额查询失败、`is_valid == false` 或 `remaining == None` 后，既有零余额临时禁用是保持、解除还是等待人工操作；“查询不到不参与”已足以证明不能**新建**禁用，但没有定义对先前已知零状态的迁移。
3. `refresh_aggregate_api_balance` 忽略余额数据库写入错误。若零状态依赖持久化快照，需要决定持久化失败时是拒绝转换还是只采用本次内存结果。
4. 手动刷新/配额批量刷新可以碰到 disabled API，而后台轮询只刷新 active API。零状态的写入应否只对 active API 生效、以及 disabled 后是否应清状态，现有代码没有回答；无论如何，配置 `status` 和 Model V2 route enabled 仍必须是更基础的候选资格条件。

## 建议加入的定向回归场景

以下是实现后必须落在的精确测试模块；均为可观察契约，不是测试内部实现。

1. **精确零值形成临时状态。** 在 `crates/service/src/aggregate_api_tests.rs`，用 mock balance 端点返回有效 `remaining: 0`；断言 refresh 返回 `ok=true`、snapshot 有 `Some(0.0)`，并仅此种有效结果写入零余额临时状态。覆盖 generic，new_api/custom 至少各有 extractor 契约。
2. **未知不是零。** 同一模块模拟 HTTP/JSON 错误、有效但 `is_valid=false`、以及有效响应缺 `remaining`；断言分别遵守现有 `balance: None` / `ok=false` 表示，且一个原本可路由 API 不会被创建零余额排除状态。
3. **负值不等同零。** 在 `crates/service/src/aggregate_api_tests.rs` 为当前允许负值的 snapshot 加回归，断言它不触发“零余额”排除（需求是 exact zero）；同时保留 quota read-model 的非负过滤边界，测试位置为 `crates/service/src/quota/read_tests.rs`。
4. **真实候选池跳过零余额而不改变配置排序。** 在 `crates/service/src/gateway/upstream/protocol/aggregate_api_tests.rs` 或紧邻 `proxy_aggregate_request` 的测试模块，建立两个 active、协议和模型都匹配的聚合 API；标记排序靠前者为零余额，断言最终请求只到另一个 candidate，并断言 disabled API / disabled Model V2 route 本来就不因这项状态改变而被重新纳入。
5. **全池被零余额排除时绝不发上游。** 在与第 4 项相同的代理级测试中，所有 active candidate 都为零；断言没有 mock upstream 收到请求，响应/日志使用明确的零余额排除结果，而不是伪称 failure cooldown。
6. **新的有效正余额恢复资格。** 在 refresh + proxy 联合测试中先成功获得零并排除，再成功获得 `remaining > 0`；断言临时状态清除、candidate 在其它条件满足时恢复，且不会靠一次请求成功或定时 TTL 偶然恢复。
7. **手动解除的精确边界。** 在 `crates/service/src/rpc_dispatch/aggregate_api_tests.rs`（现有 `rpc_request` 和 `try_handle` fixture，`1-91`）覆盖 `id` / `apiId`、空 ID、未知 ID、成功解除、随后可路由，以及下一次有效零刷新可重新排除。解除响应必须与 typed result 对齐且不得返回 secret/raw response。
8. **解除不触碰其它阻断来源。** 在 `crates/service/src/gateway/routing/tests/aggregate_api_cooldown_tests.rs` 和 health 相关测试中，先建立 failure cooldown 或启用 health block，再解除零余额；断言 failure cooldown 仍按模型阻断、persisted health 仍按 config 阻断、配置 `status=disabled` 仍不参与。反向测试也应断言现有 `runtimeStatus/reset` 不会暗中变成余额解除。
9. **管理员授权。** 在 `crates/service/src/rpc_dispatch/mod.rs` 的 actor-aware dispatch 覆盖中，以非 admin 且非 password-web-auth 调用新解除 RPC，断言 `permission_denied`；以 admin 调用才能到达 aggregate dispatcher。这锁定 desktop/service-mode 共用的 service access-control 边界。
10. **生命周期裁决回归。** 一旦第一个未证实假设确定，在运行时状态模块与 storage fixture 中测试它：服务 runtime reload 后，`last_balance_*` 仍存在；零余额临时状态则严格按选定的“释放”或“受新鲜度约束地重建”契约表现，不能依赖偶然的 `OnceLock` 初始化顺序。

## 结论

已验证的安全插入路径是：所有刷新来源 → `refresh_aggregate_api_balance` 的 typed 成功 snapshot → 独立零余额临时状态 → `proxy_aggregate_request` 的最终 candidate filter → 独立 typed 管理员解除 RPC。现有配置启停、Model V2 route、failure cooldown、持久化 health block、policy action 都已各有语义；把零余额塞入其中任一已有状态会破坏需求要求的状态分离。

未修改产品代码、规格或配置；未运行构建、测试、lint 或项目级验证命令。
