# Implement: 上游自定义状态监控（MVP）

> Profile: **critical** — 涉及凭据访问、持久化写入、公共 RPC 契约、路由阻断语义。
> 每个 slice 必须通过 RED/GREEN 验证后再推进下一个。不提前合并多个 slice。

---

## AC 覆盖映射

| AC | Slice(s) |
|---|---|
| AC-001 区分两套插件机制 | 研究文档已验证，无代码 slice |
| AC-002 列出 Rhai 能力边界 | 研究文档已验证，无代码 slice |
| AC-003 插件安全风险结论 | 研究文档已验证，无代码 slice |
| AC-004 兼容不同上游状态结构 | Slice 1 → 2 → 4 → 3 → 6 |
| AC-005 保护上游凭据 | Slice 3（凭据绑定）|
| AC-006 复用现有健康状态与路由语义 | Slice 4 → 5 |
| AC-007 在已有上游界面展示 | Slice 7 → 8 → 9 |
| AC-008 明确 MVP 展示范围 | design.md §10 + PRD Key Decisions 已确认 |

---

## Slice 1: DB migration 137 — aggregate_api_custom_monitors 表

**Behavior**
服务启动时自动创建 `aggregate_api_custom_monitors` 表；删除关联 `aggregate_apis` 记录时级联删除。

**Code boundary**
- 新建 `crates/core/migrations/137_aggregate_api_custom_monitors.sql`
- `crates/core/src/storage/mod.rs`：在 `apply_sql_migration` 调用链中追加 migration 137（紧跟 136 之后）

**SQL 内容**
```sql
CREATE TABLE IF NOT EXISTS aggregate_api_custom_monitors (
    id                      TEXT PRIMARY KEY,
    aggregate_api_id        TEXT NOT NULL REFERENCES aggregate_apis(id) ON DELETE CASCADE,
    name                    TEXT NOT NULL,
    enabled                 INTEGER NOT NULL DEFAULT 1,
    request_method          TEXT NOT NULL DEFAULT 'GET',
    request_path            TEXT NOT NULL,
    request_body_template   TEXT,
    auth_mode               TEXT NOT NULL DEFAULT 'none',
    auth_header             TEXT,
    auth_header_prefix      TEXT,
    response_mapping_json   TEXT NOT NULL DEFAULT '{}',
    schedule_interval_secs  INTEGER NOT NULL DEFAULT 300,
    timeout_ms              INTEGER NOT NULL DEFAULT 10000,
    next_run_at             INTEGER,
    created_at              INTEGER NOT NULL,
    updated_at              INTEGER NOT NULL,
    UNIQUE(aggregate_api_id)
);
CREATE INDEX IF NOT EXISTS idx_custom_monitors_due
    ON aggregate_api_custom_monitors(enabled, next_run_at);
```

**Test seam**
`crates/core/src/storage/mod.rs` 中 `#[cfg(test)]` 的 in-memory DB 初始化路径（`Storage::open_in_memory()` 或等价入口）。

**RED**
```rust
// 在 crates/core/src/storage/ 的集成测试中：
let storage = Storage::open_in_memory().unwrap();
// 预期：表不存在时操作报错或返回空，migration 后正常
assert!(storage.list_custom_monitors("any-api-id").is_err() || storage.list_custom_monitors("any-api-id").unwrap().is_empty());
```
（此时 `list_custom_monitors` 尚未实现，测试无法编译 → 红）

**GREEN**
migration SQL + storage 方法实现后，`list_custom_monitors("any-api-id")` 返回 `Ok(vec![])` → 绿。

**Validation**
```bash
cargo test -p codexmanager-core -- storage
```

**Dependencies**
无前置 slice。

**Rollback**
停止调度器与移除 UI/RPC 暴露即可安全回退；生产环境不得删除已记录 migration 或直接 `DROP TABLE`，以保留健康事件与定义数据供后续版本恢复。

---

## Slice 2: Storage CRUD — CustomMonitorDefinition

**Behavior**
- `upsert_custom_monitor(def)` → 新增或更新；数据库唯一约束保证每个 API 仅一条定义，更新时 `aggregate_api_id` 不可变。
- `list_custom_monitors(api_id)` / `get_custom_monitor(id)` → 返回定义；`delete_custom_monitor(id)` → 删除。
- `has_enabled_custom_monitor(api_id)` → 供既有自动轮询排除已由自定义监控接管的 API。
- `list_due_custom_monitors(due_before, limit)` → 返回 `enabled=1` 且 `next_run_at <= due_before` 的定义，按 `next_run_at ASC` 有界查询。
- `update_custom_monitor_next_run(id, next_run_at)` → 仅在执行结束后更新调度时间戳。

**Code boundary**
- 新建 `crates/core/src/storage/custom_monitors.rs`
- `crates/core/src/storage/mod.rs`：`mod custom_monitors;` + 新增 `CustomMonitorDefinition` struct

**CustomMonitorDefinition struct**（在 `crates/core/src/storage/mod.rs` 末尾追加）
```rust
#[derive(Debug, Clone)]
pub struct CustomMonitorDefinition {
    pub id: String,
    pub aggregate_api_id: String,
    pub name: String,
    pub enabled: bool,
    pub request_method: String,
    pub request_path: String,
    pub request_body_template: Option<String>,
    pub auth_mode: String,
    pub auth_header: Option<String>,
    pub auth_header_prefix: Option<String>,
    pub response_mapping_json: String,
    pub schedule_interval_secs: i64,
    pub timeout_ms: i64,
    pub next_run_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}
```

**Test seam**
公开的 `impl Storage` 方法；通过 in-memory DB 的 unit test 覆盖。

**RED**
```rust
// test: upsert then list
let def = CustomMonitorDefinition { id: "m1".into(), aggregate_api_id: "api1".into(), ... };
storage.upsert_custom_monitor(&def).unwrap();
let list = storage.list_custom_monitors("api1").unwrap();
assert_eq!(list.len(), 1);
assert_eq!(list[0].id, "m1");
```

**GREEN**
方法实现后，上述 assert 通过 → 绿。

**Validation**
```bash
cargo test -p codexmanager-core -- custom_monitor
```

**Dependencies**
Slice 1（migration 137 已存在）。

**Rollback**
删除 `custom_monitors.rs`，从 `mod.rs` 移除 mod + struct 声明。

---

## Slice 3: CustomMonitorAdapter — HTTP 执行 + 凭据绑定 + ResponseMapping 解析

**Behavior**
`execute_custom_monitor(storage, def)` 在 Slice 4 提供的写入接口之上执行，全部留在 Rust 内部，不跨插件边界：
1. 读取关联 Aggregate API 的已保存 URL，并以字符串拼接构造同 origin 的目标 URL；拒绝不符合设计约束的 path，client 使用 `redirect(Policy::none())`。
2. `auth_mode = "aggregate_api_key"` 时，仅接受已保存 `auth_type=apikey`，经校验后的 Header/prefix 注入 `secret_value`；`auth_mode = "aggregate_api_basic"` 时，仅接受 `auth_type=userpass` 并在 Rust 内构造 Basic Header；`none` 不读取秘密。
3. 使用独立缓存的 blocking client 和请求级 `def.timeout_ms`；不得复用 plugin HTTP client，不记录 URL 查询、请求 Header、秘密或响应原文。
4. 解析 ResponseMapping：按受限点分隔路径提取字符串/数字字段，与 healthy/degraded/unhealthy_values 比对（字符串大小写不敏感）。
5. 产出 `MonitorOutcome { ok, http_status, latency_ms, error_category, reason }`，再调用 `record_observation_with_category(storage, api_id, "custom_monitor", ...)`。

**错误分类映射**（对应 design.md §6）：
- TCP/DNS 失败 → `error_category = Some("unreachable")`
- 超时 → `error_category = Some("timeout")`
- HTTP 401/403 → `error_category = Some("auth")`
- HTTP 429 → `error_category = Some("rate_limited")`
- HTTP 5xx → `error_category = Some("server_error")`；其余非 2xx/认证/限流响应 → `Some("http_error")`
- JSON 解析失败 / status_field_path 缺失 → `error_category = Some("parse_error")`
- unhealthy_values 匹配 → `error_category = Some("business_unhealthy")`；degraded_values 匹配 → `Some("business_degraded")`

**Code boundary**
- 新建 `crates/service/src/aggregate_api_custom_monitor.rs`
- 在 `crates/service/src/lib.rs` 中 `mod aggregate_api_custom_monitor;`
- `execute_custom_monitor` 使用独立 `reqwest::blocking::Client`（独立 `OnceLock`、`redirect(Policy::none())`）；超时设在 RequestBuilder，确保每条定义生效。

**Test seam**
通过 `record_observation_with_category` 的副作用验证公开行为：执行后读取 `storage.aggregate_api_health_state(api_id, None, None)`。集成测试使用 in-memory storage + 项目现有的 mock HTTP 方式，覆盖 healthy、401、格式错误、超时、Basic/API-key 认证类型不匹配、拒绝重定向与 Header 值不出现在 event/reason。

**RED**
```rust
// 测试：HTTP 200 + 业务字段 healthy → ok=true, state=healthy
let mock_server = start_mock_server(200, r#"{"status":"ok"}"#);
let def = CustomMonitorDefinition {
    request_path: "/status".into(),
    response_mapping_json: r#"{"status_field_path":"status","healthy_values":["ok"],...}"#.into(),
    auth_mode: "none".into(),
    timeout_ms: 5000, schedule_interval_secs: 60, ..
};
execute_custom_monitor(&storage, &def);
let state = storage.aggregate_api_health_state(api_id, None, None).unwrap().unwrap();
assert_eq!(state.state, "healthy");

// 测试：HTTP 401 → state=cooldown, error_category=auth
let mock_401 = start_mock_server(401, "Unauthorized");
// ... 执行后
assert_eq!(state.state, "cooldown");
assert_eq!(state.last_error_category.as_deref(), Some("auth"));
```

**GREEN**
在 Slice 4 已绿的前提下实现 `execute_custom_monitor`；适配器测试和已有健康状态测试均通过 → 绿。

**Validation**
```bash
cargo test -p codexmanager-service -- custom_monitor
```

**Dependencies**
Slice 1（migration）、Slice 2（storage CRUD）、Slice 4（显式分类健康写入；必须先完成）。

**Rollback**
停用自定义调度器并移除 RPC/UI 暴露；不得删除既有健康状态或事件，避免掩盖已经观测到的路由状态。

---

## Slice 4: record_observation_with_category + trigger_label 扩展

**Behavior**
- `aggregate_api_health.rs` 新增 `pub(crate) fn record_observation_with_category(...)` 接受显式 `error_category: Option<&str>`，跳过 `failure_category()` 推导，其余状态机逻辑与 `record_observation_with_storage` 完全相同。
- `trigger_label()` 新增分支：`"custom_monitor" => "自定义监控"`
- `record_observation_with_storage`（已有函数）不修改签名，不影响任何现有调用方。

**Code boundary**
`crates/service/src/aggregate_api_health.rs` 内部修改（私有函数扩展 + 新增 pub(crate) 函数）。

**Test seam**
`aggregate_api_health.rs` 已有 `#[cfg(test)] mod tests`（lines 793-1017）。在其中增加：
- `test_record_with_explicit_auth_category`：传入 `error_category=Some("auth")`，验证 `state=cooldown`，cooldown_until 被设置
- `test_record_with_business_degraded`：传入 `error_category=Some("business_degraded")`，验证 `state=degraded`，cooldown_until 为 None
- `test_trigger_label_custom_monitor`：验证 `trigger_label("custom_monitor") == "自定义监控"`

**RED**
新增测试引用 `record_observation_with_category` 和新 trigger_label 分支 → 编译失败（函数未定义）→ 红。

**GREEN**
实现后测试通过 → 绿。

**Validation**
```bash
cargo test -p codexmanager-service -- aggregate_api_health
```

**Dependencies**
Slice 1（migration）和 Slice 2（storage）完成后立即实现；本 Slice 必须在 Slice 3 开始前 GREEN，不能并行跨越该依赖。

**Rollback**
删除新增函数和 trigger_label 分支；Slice 3 中的调用方一并回滚。

---

## Slice 5: 自定义监控调度器

**Behavior**
`ensure_custom_monitor_polling()` 启动一个后台线程（与 `ensure_aggregate_api_health_polling()` 模式对齐）：
- 每 30 秒调用 `list_due_custom_monitors(now, 2)`；逐条执行后持久化 `next_run_at = now + schedule_interval_secs`，无额外内存计数器。
- `OnceLock<()>` 保证只启动一次；阻塞请求仅占用该调度线程。
- `ensure_aggregate_api_health_polling()` 在调用原有 `probe_health_with_trigger()` 前，使用 `has_enabled_custom_monitor(api_id)` 排除已经由自定义定义接管的 API。
- 当定义被禁用或删除，下一轮原生轮询自动恢复；`aggregateApi/health/probe` 对已接管 API 返回明确错误，不写入冲突观测。

在 `crates/service/src/lifecycle/startup.rs` 中紧跟 `ensure_aggregate_api_health_polling()` 之后调用 `aggregate_api_custom_monitor::ensure_custom_monitor_polling()`。

**Code boundary**
- `crates/service/src/aggregate_api_custom_monitor.rs`（新增 `ensure_custom_monitor_polling` 与可测试的 `run_due_monitors`）
- `crates/service/src/aggregate_api_health.rs`（排除已接管 API，拒绝冲突手动探测）
- `crates/service/src/lifecycle/startup.rs`（新增一行启动调用）

**Test seam**
调度器核心循环不启动真实线程：`run_due_monitors(storage, now)` 使用 in-memory storage 和 mock upstream，验证：
- due 的定义执行并更新健康状态和 `next_run_at`；not-due 的定义不执行。
- 原生轮询的候选 API 中，启用定义的 API 被排除；禁用/删除定义后重新可探测。
- 已接管 API 的手动 health probe 返回错误且不改变现有 global state。

**RED**
测试 `run_due_monitors` 函数不存在 → 编译失败 → 红。

**GREEN**
实现后测试通过 → 绿。

**Validation**
```bash
cargo test -p codexmanager-service -- ensure_custom_monitor
```

**Dependencies**
Slice 2（`list_due_custom_monitors`、`update_custom_monitor_next_run`、所有权查询）、Slice 3（`execute_custom_monitor`）。

**Rollback**
从 `startup.rs` 移除 `ensure_custom_monitor_polling()` 调用；调度器线程不再启动；已持久化的 health 数据保持有效。

---

## Slice 6: RPC 管理端点

**Behavior**
在 `crates/service/src/rpc_dispatch/aggregate_api.rs` 的 `try_handle()` 中新增：

```
"aggregateApi/monitor/define"  → define_custom_monitor(...)
"aggregateApi/monitor/list"    → list_custom_monitors(api_id)
"aggregateApi/monitor/delete"  → delete_custom_monitor(monitor_id)
```

参数验证（必须在 service 层执行，前端仅复用错误提示）：
- `scheduleIntervalSecs` < 60 → `Err("scheduleIntervalSecs must be >= 60")`；`timeoutMs` 不在 1000–30000 → 明确错误。
- 每个 `aggregateApiId` 只允许一条定义；新建遇到已有定义或更新时改变归属 → 明确错误，不允许多来源竞争。
- `requestMethod` 仅 `GET`/`POST`；`requestPath` 必须为单个 `/` 开头且不含 `//`、反斜杠、控制字符或 fragment；POST body 必须是静态合法 JSON。
- `authMode` 仅 `none`、`aggregate_api_key`、`aggregate_api_basic`；HeaderName/prefix、ResponseMapping 及数组值类型均在服务端解析校验。Basic 模式拒绝 Header/prefix。
- 更新必须携带已有 `id`；创建不传 `id` 且由服务端生成 UUID4。所有 result 不含 secret 或解析前响应。

**Code boundary**
- `crates/service/src/aggregate_api_custom_monitor.rs`：`define_custom_monitor`、`list_custom_monitors`、`delete_custom_monitor` 及服务端输入校验
- `crates/service/src/rpc_dispatch/aggregate_api.rs`：新增 3 条 match arm，沿用 Aggregate API 管理操作的现有授权路径
- `apps/src-tauri/src/commands/aggregate_api.rs` 与 `commands/registry.rs`：注册 `service_aggregate_api_monitor_{define,list,delete}` 代理命令
- `apps/src/lib/api/transport-web-commands/aggregate-api.ts`：为同名命令映射 3 个 RPC method，保持桌面与 Web 模式同一契约

**Test seam**
`crates/service` RPC 测试覆盖 define → list → update → delete、单定义唯一约束、路径/超时/认证模式校验及错误不回显秘密；`apps` runtime test 覆盖桌面命令名和 Web 映射分别落到同一 RPC method。

**RED**
RPC tests 中调用 `handle_rpc("aggregateApi/monitor/define", ...)` → 返回方法未知错误 → 红（方法未注册）。

**GREEN**
注册 match arm + service 层实现后，CRUD tests 通过 → 绿。

**Validation**
```bash
cargo test -p codexmanager-service -- custom_monitor_rpc
pnpm -C apps run test:runtime
```
手动验证：以桌面 IPC 与 Web transport 各调用一次 define/list，确认返回同一非敏感字段集。

**Dependencies**
Slice 2（storage）、Slice 5（所有权调度）；Tauri/Web 传输映射必须与 RPC 在同一 slice 完成。

**Rollback**
停用 3 条 dispatch/传输映射及调度器；保留 service/storage 数据，避免错误地删除已有观测历史。

---

## Slice 7: 前端 accountClient 方法

**Behavior**
在 `apps/src/lib/api/account-client.ts` 中新增：

```typescript
async defineAggregateApiMonitor(params: {
  id?: string;
  aggregateApiId: string;
  name: string;
  enabled: boolean;
  requestMethod: "GET" | "POST";
  requestPath: string;
  requestBodyTemplate?: string | null;
  authMode: "none" | "aggregate_api_key" | "aggregate_api_basic";
  authHeader?: string | null;
  authHeaderPrefix?: string | null;
  responseMappingJson: string;
  scheduleIntervalSecs: number;
  timeoutMs: number;
}): Promise<CustomMonitorDefinition>

async listAggregateApiMonitors(apiId: string): Promise<CustomMonitorDefinition[]>

async deleteAggregateApiMonitor(monitorId: string): Promise<void>
```

新增类型（`apps/src/types/api-key.ts` 或独立文件）：

```typescript
export interface CustomMonitorDefinition {
  id: string;
  aggregateApiId: string;
  name: string;
  enabled: boolean;
  requestMethod: string;
  requestPath: string;
  requestBodyTemplate: string | null;
  authMode: string;
  authHeader: string | null;
  authHeaderPrefix: string | null;
  responseMappingJson: string;
  scheduleIntervalSecs: number;
  timeoutMs: number;
  nextRunAt: number | null;
  createdAt: number;
  updatedAt: number;
}
```

**Code boundary**
- `apps/src/lib/api/account-client.ts`：新增 3 个方法，经 `invoke` + `withAddr()` 调用 Slice 6 已注册命令
- `apps/src/types/api-key.ts`：新增 `CustomMonitorDefinition` interface + normalizer；不创建第二套健康结果类型

**Test seam**
扩展现有 Node runtime fixture，验证 define/list/delete 的命令名、camelCase 参数、`withAddr()` 使用方式和 result normalizer；不使用 raw fetch 或浏览器直连 RPC。

**RED**
在 page.tsx 中 import `accountClient.listAggregateApiMonitors` → TypeScript 编译错误（方法不存在）→ 红。

**GREEN**
方法定义后 TypeScript 通过 → 绿。

**Validation**
```bash
pnpm -C apps run test:runtime
```

**Dependencies**
Slice 6（后端 RPC 端点存在，前端才能实际联调）。

**Rollback**
删除 3 个方法和 interface 定义；无副作用。

---

## Slice 8: /aggregate-api/ 页面健康状态列

**Behavior**
在 `/aggregate-api/page.tsx` 的主表格中，现有"运行状态"列（TableCell，约 line 993-1128）之后增加"健康监控"列：

```
健康监控列内容：
  health = healthByApiId.get(api.id)
  若 health 为 null/undefined：展示 <Badge variant="secondary">未配置</Badge>
  否则：
    <Tooltip>
      <TooltipTrigger>
        <HealthStateBadge state={health.state} />
        <span className="text-[10px] text-muted-foreground">
          {formatTsFromSeconds(health.lastObservedAt, "—")}
        </span>
      </TooltipTrigger>
      <TooltipContent>
        延迟: {health.latencyMs ?? "—"} ms
        HTTP: {health.httpStatus ?? "—"}
        原因: {health.errorReason ?? "—"}
        来源: {health.observationSource ?? "—"}
      </TooltipContent>
    </Tooltip>
    <Button onClick={() => openHealthDetail(api.id)}>详情</Button>
```

`HealthStateBadge` 是新增的 presentational component，按状态映射颜色：
- healthy → emerald
- degraded → amber
- unhealthy / cooldown → rose
- recovering → sky
- unknown → secondary（灰）

列头：`{t("健康监控")}`

**Code boundary**
- `apps/src/app/aggregate-api/page.tsx`：新增 TableHead + TableCell
- 可抽取 `components/HealthStateBadge.tsx` 或内联定义

**healthByApiId 已存在**（line 242-245），无需新增 query。

**Test seam**
`apps/src/app/aggregate-api/page.test.tsx`（若存在）中补充：
- render 时 health state = "healthy" → badge 显示绿色标识
- render 时 health 为 undefined → 显示"未配置"
- 点击"详情"按钮 → 调用 `openHealthDetail` mock

**RED**
测试 `render(<AggregatePage />)` 期望出现健康状态徽章 → 测试失败（元素不存在）→ 红。

**GREEN**
新增 TableCell 后测试通过 → 绿。

**Validation**
```bash
pnpm -C apps run test:runtime
pnpm -C apps run build:desktop
```
浏览器验证：打开实际 `/aggregate-api/` 页面，确认健康列、状态色、Tooltip 字段和详情入口可操作。

**Dependencies**
Slice 7（类型定义）；healthQuery 已存在（无新依赖）。

**Rollback**
删除新增 TableHead + TableCell；`HealthStateBadge` 可保留或删除。

---

## Slice 9: 健康详情面板

**Behavior**
`openHealthDetail(apiId)` 触发一个 Dialog/Sheet 组件，调用 `accountClient.getAggregateApiHealth(apiId)` 并展示：

**摘要区**
- 当前 state（HealthStateBadge）+ 更新时间
- 是否启用主动探测（config.enabled）、探测模型（config.probeModel）
- 监控来源（summary.observationSource）、最近观测时间/延迟/HTTP 状态
- 若 errorCategory 不为空：展示分类和 errorReason

**事件列表（最近 20 条）**
- 每行：trigger → outcome | stateBefore → stateAfter | httpStatus | reason | observedAt
- outcome 用颜色区分：success=绿，failure=红

**监控定义管理区（完整最小表单）**
- `accountClient.listAggregateApiMonitors(apiId)` 读取 0 或 1 条定义；空状态提供“添加监控”。
- 表单可创建、编辑、启停、删除，字段仅为 name、GET/POST、相对 path、静态 JSON body、认证模式、Header/prefix（仅 API-key 模式）、ResponseMapping、间隔和超时；提交前做 UX 校验，服务端仍为唯一安全边界。
- 提交/删除成功后失效 monitor query 与 `aggregate-api-health` query；Basic 模式不展示或保存 Header/prefix 控件。

**Code boundary**
- `apps/src/app/aggregate-api/page.tsx`：新增 Dialog state + `openHealthDetail`，复用现有 queryClient 失效模式
- 新建聚焦的 `apps/src/components/aggregate-api/AggregateApiHealthDetailPanel.tsx` 与最小表单，避免继续膨胀已很大的 page.tsx

**注意**：`AggregateApiHealthDetail` 类型（已有 `apps/src/types/api-key.ts:141-146`）和 `accountClient.getAggregateApiHealth`（已有 line 869-872）均已存在，无需新增。

**Test seam**
- 打开详情时 `getAggregateApiHealth` 与 `listAggregateApiMonitors` 被调用，panel 展示 summary.state 及归一化字段。
- 表单创建/启停/编辑/删除分别调用相应 client 方法并失效相关 query；Basic 模式不渲染 Header/prefix 输入。
- 新增 `apps/tests/aggregate-api-custom-monitor.spec.ts`，沿用现有 Aggregate API 页面 mock/fixture：覆盖详情打开、表单字段随认证模式变化、保存后列表刷新与健康列可见；不以源码文本断言替代用户流。

**RED**
测试 open detail → getAggregateApiHealth mock 被调用 → panel 未渲染 → 测试失败 → 红。

**GREEN**
Panel 实现后测试通过 → 绿。

**Validation**
```bash
pnpm -C apps run test:runtime
pnpm -C apps exec playwright test tests/aggregate-api-custom-monitor.spec.ts
pnpm -C apps run build:desktop
```
浏览器验证：
1. 打开 `/aggregate-api/`，点击某行“详情”。
2. Dialog 显示 summary、config、归一化字段、最近事件与定义表单。
3. 在页面创建 `GET /status`、`authMode=none`、60 秒间隔的定义；无需 DevTools。
4. 保存后定义刷新，健康列与详情事件在下一调度周期更新。

**Dependencies**
Slice 7（client 方法）、Slice 8（触发入口）。

**Rollback**
删除 Dialog state + Panel 组件；Slice 8 中的"详情"按钮 onClick 改为 no-op。

---

## 完整验证清单

在所有 slice GREEN 之后，执行以下端到端验证：

```bash
# Rust 全工作区回归（migration、storage、service 与调用方）
cargo test --workspace

# 前端运行时契约、静态导出构建
pnpm -C apps run test:runtime
pnpm -C apps exec playwright test tests/aggregate-api-custom-monitor.spec.ts
pnpm -C apps run build:desktop
```

浏览器与服务冒烟（按顺序）：
1. 启动实际 service + Web/desktop 应用，确认 migration 137 创建 `aggregate_api_custom_monitors`。
2. 在 `/aggregate-api/` 详情面板创建 `GET /status`、`authMode=none`、60 秒定义；确认未出现独立插件或 DevTools 配置步骤。
3. 等待调度，确认健康列、详情摘要和事件显示 `自定义监控`、HTTP、延迟、归一化原因。
4. 将 mock upstream 改为 401，等待下一周期；确认 cooldown 及网关实际请求被拒绝或改走既有回退路由，且 mock 未收到被阻断 API 的转发。
5. 禁用或删除定义，确认原生健康轮询在下一周期重新接管；恢复 upstream 后按既有 reset/recovery 语义验证恢复。

---

## 安全验证要点（critical profile 要求）

在 Slice 3、5、6 GREEN 之后，额外验证：

- API-key 和 Basic 两种成功认证请求的 service 日志、RPC 返回、`aggregate_api_health_events.reason` 均不含凭据或响应原文。
- 301/302 重定向不被跟随，`//host`、反斜杠、控制字符和 fragment path 均由 define RPC 拒绝。
- `scheduleIntervalSecs=59`、`timeoutMs=999/30001`、不匹配的 saved credential type、第二条同 API 定义均返回精确错误。
- Rhai `list_settings()` 和手动插件任务都不能读取 `aggregate_api_secrets`，也不能写健康状态。

---

## 未完成风险记录

以下风险在 MVP 不实现，但须在 implement 前确认不影响可交付范围：

1. **独立状态主机（未来扩展）**：MVP 仅同 origin relative path；若供应商状态页位于另一 host，必须先完成 URL allowlist、私网/重定向防护和凭据转发策略。
2. **Plugin Adapter（deferred）**：Rhai host capability token 设计未完成，留待后续；本 MVP 由 Rust Config Adapter 执行。
3. **`business_degraded` 不触发路由阻断**：当前设计 degraded 不设 cooldown，路由不阻断。若用户期望降级时也阻断，需要新增配置项，MVP 不实现。
