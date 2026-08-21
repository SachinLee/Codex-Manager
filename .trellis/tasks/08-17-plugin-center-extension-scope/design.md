# Design: 上游自定义状态监控（MVP）

## 1. 背景与当前行为

### 现有健康监控能力

`crates/service/src/aggregate_api_health.rs` 已有完整的健康状态机：
- 状态枚举：`unknown` / `healthy` / `degraded` / `unhealthy` / `cooldown` / `recovering`
- 核心写入入口：`record_observation_with_storage(storage, api_id, model, protocol, trigger, ok, status, latency_ms, reason)`
- 路由阻断：`is_routing_blocked_with_storage()` 检查 cooldown/unhealthy 并参与网关路由
- 健康探测调度：`ensure_aggregate_api_health_polling()` 以 `scheduled_probe` trigger 调用现有 LLM 探测
- 持久化：`aggregate_api_health_states` + `aggregate_api_health_events` 表，已有 RPC `aggregateApi/health/*`

### 当前插件运行时限制（AC-001~003 已由研究文档确认）

Rhai 运行时注册的宿主函数仅有：`log`、`get_setting`/`list_settings`（需 `settings:read`）、`http_get`/`http_post`（需 `network`，无 Header 支持）、`cleanup_banned_accounts`/`cleanup_unavailable_free_accounts`（需 `accounts:cleanup`）。

**插件不能**：读取 `aggregate_api_secrets` 表、写入 `aggregate_api_health_states`、调用任何 RPC、注册路由或 UI 组件。因此插件不适合作为生产级上游状态监控的凭据持有者或健康状态写入者。

### 当前 UI 缺口

`/aggregate-api/page.tsx` 每 15 秒查询 `aggregateApi/health/list`，`healthByApiId` map 已构建，但 `state`、`latencyMs`、`httpStatus`、`errorCategory`、`errorReason`、`observationSource` 字段未在主表格或详情面板完整呈现。详情 RPC `aggregateApi/health/get` 已有事件列表能力但未挂接 UI。

---

## 2. 方案概述

**MVP 选择：内置 Config Adapter，不扩展 Rhai 权限。**

新增 `aggregate_api_custom_monitor` 模块，作为 `aggregate_api_health` 的新观测源。执行流程：

```
CustomMonitorDefinition (SQLite config)
        ↓ 调度器定时触发
CustomMonitorAdapter (Rust, crates/service)
        ↓ 读取 AggregateApiSecretConfig 注入凭据
        ↓ HTTP 请求 + ResponseMapping 解析
        ↓ 归一化 MonitorOutcome
record_observation_with_category()         ← 新增的 thin wrapper
        ↓
aggregate_api_health_states / events       ← 现有表，无改动
        ↓
aggregateApi/health/list + get RPC         ← 现有 RPC，无改动
        ↓
/aggregate-api/ 前端（增加健康列 + 详情面板）
```

插件保持原有职责：脚本管理、任务调度、原始日志。不添加新 Rhai 宿主函数。

---

## 3. 最小深模块接口

### 3.1 公开接口（`crates/service/src/aggregate_api_custom_monitor.rs`）

```
// 执行单条监控定义，结果写入 aggregate_api_health
pub(crate) fn execute_custom_monitor(
    storage: &Storage,
    definition: &CustomMonitorDefinition,
) -> MonitorExecutionResult

// 调度器入口，仅启动一次后台线程
pub(crate) fn ensure_custom_monitor_polling()

// RPC helpers (由 rpc_dispatch/aggregate_api.rs 调用)
pub(crate) fn define_custom_monitor(...)  -> Result<CustomMonitorDefinition, String>
pub(crate) fn list_custom_monitors(api_id: &str) -> Result<Vec<CustomMonitorDefinition>, String>
pub(crate) fn delete_custom_monitor(id: &str) -> Result<(), String>
```

`execute_custom_monitor` 隐藏 HTTP 执行、Header 注入、凭据绑定、ResponseMapping 解析和错误分类；调用方只需提供定义，结果自动写入现有健康状态机，响应原文不保留。

### 3.2 现有接口扩展（`aggregate_api_health.rs`）

新增纯扩展函数，不修改现有函数签名，不破坏任何已有调用方：

```rust
// 仅在 custom_monitor 模块内调用；bypass failure_category() 推导
pub(crate) fn record_observation_with_category(
    storage: &Storage,
    api_id: &str,
    trigger: &str,
    ok: bool,
    status: Option<i64>,
    latency_ms: Option<i64>,
    reason: Option<&str>,
    error_category: Option<&str>,   // 显式分类，优先于 failure_category()
)
```

同时在 `trigger_label()` 中追加：`"custom_monitor" => "自定义监控"`

---

## 4. 数据契约

### 4.1 CustomMonitorDefinition — 新增 SQLite 表（migration 137）

```sql
CREATE TABLE IF NOT EXISTS aggregate_api_custom_monitors (
    id                      TEXT PRIMARY KEY,
    aggregate_api_id        TEXT NOT NULL REFERENCES aggregate_apis(id) ON DELETE CASCADE,
    name                    TEXT NOT NULL,
    enabled                 INTEGER NOT NULL DEFAULT 1,
    request_method          TEXT NOT NULL DEFAULT 'GET',   -- 'GET' | 'POST'
    request_path            TEXT NOT NULL,                  -- 相对路径，拼接到 AggregateApi.url
    request_body_template   TEXT,                           -- 静态 JSON body，仅 POST 有效
    auth_mode               TEXT NOT NULL DEFAULT 'none',  -- 'none' | 'aggregate_api_key' | 'aggregate_api_basic'
    auth_header             TEXT,                           -- e.g. 'Authorization' | 'x-api-key'
    auth_header_prefix      TEXT,                           -- e.g. 'Bearer ' | ''
    response_mapping_json   TEXT NOT NULL DEFAULT '{}',    -- ResponseMapping JSON
    schedule_interval_secs  INTEGER NOT NULL DEFAULT 300,  -- 最小 60
    timeout_ms              INTEGER NOT NULL DEFAULT 10000, -- 最大 30000
    next_run_at             INTEGER,
    created_at              INTEGER NOT NULL,
    updated_at              INTEGER NOT NULL,
    UNIQUE(aggregate_api_id)
);
CREATE INDEX idx_custom_monitors_due ON aggregate_api_custom_monitors(enabled, next_run_at);
```

**约束**：
- 每个 `aggregate_api_id` 仅允许一条定义；启用时由该定义独占全局健康状态，避免多条监控互相覆盖。
- `schedule_interval_secs` 最小值 60（服务端强制，不依赖前端校验）；持久化的 `next_run_at` 保证调度频率不会超过该值。
- `timeout_ms` 取值范围 1000–30000；请求体为静态 JSON，不支持任何变量替换。
- 创建时由服务端生成 UUID；更新时必须携带已有 `id` 且 `aggregate_api_id` 不可变。
- `request_path` 必须以单个 `/` 开头，拒绝 `//`、反斜杠、控制字符和 fragment；以字符串拼接保留已保存 Aggregate API 的 origin，不使用 `Url::join`。

### 4.2 ResponseMapping DSL

存储为 `response_mapping_json` 字段的 JSON：

```json
{
  "status_field_path": "data.status",
  "healthy_values": ["ok", "healthy", "active", "up"],
  "degraded_values": ["degraded", "partial", "slow"],
  "unhealthy_values": ["error", "down", "unavailable", "critical"]
}
```

字段说明：
- `status_field_path`：点分隔路径（如 `"status"` 或 `"data.health.state"`），`null` 表示仅依赖 HTTP 状态码
- `healthy_values` / `degraded_values` / `unhealthy_values`：字符串或数字匹配列表，大小写不敏感
- 匹配优先级：unhealthy > degraded > healthy；若值匹配任意降级列表，ok=false
- 路径不存在或 JSON 解析失败 → `parse_error` 分类，不等同于业务不健康

### 4.3 MonitorObservation（归一化结果，不持久化，只传递给 `record_observation_with_category`）

```rust
struct MonitorOutcome {
    ok: bool,
    http_status: Option<i64>,
    latency_ms: Option<i64>,
    error_category: Option<&'static str>,  // 见 §5 分类表
    reason: Option<String>,                // 脱敏、长度受限的短原因
}
```

响应原文不写入日志、SQLite 或前端；详情只显示现有健康模型已承载的归一化字段。

---

## 5. 凭据绑定模型

**原则：凭据只在 Rust 运行时内流转，不进入任何可被插件读取的上下文。**

执行流程：
1. `auth_mode = "aggregate_api_key"` 时，`execute_custom_monitor()` 读取 `AggregateApiSecretConfig`；仅接受已保存 `auth_type=apikey`，将 `secret_value` 作为配置 Header 的值。Header 默认 `Authorization: Bearer <secret>`；`auth_header` 须经 `HeaderName` 校验，prefix 拒绝 CR/LF。
2. `auth_mode = "aggregate_api_basic"` 时，仅接受 `auth_type=userpass`，在 Rust 内解析已保存的 username/password 并构造 Basic 认证；前端和插件都不接收该 JSON 或其字段。
3. `auth_mode = "none"` 不读取秘密。认证模式与已保存凭据类型不匹配时，产生不含凭据的 `auth` 失败观测。
4. 独立 blocking client 禁止重定向；所有错误原因截断并脱敏，不保留响应原文。

**禁止路径（由模块内部边界保证，不依赖外部配置）**：
- secret_value 和 userpass 字段不写入任何 log、health event 或 RPC 返回。
- 插件 Rhai 上下文不接受任何包含 aggregate_api_id binding 的宿主函数。
- 认证 Header、请求体和 URL 查询参数均不得包含模板变量或引用凭据。

---

## 6. 错误分类表（AC-004）

| 条件 | error_category | ok | 触发 cooldown |
|---|---|---|---|
| TCP/DNS 连接失败 | `unreachable` | false | 累积到阈值 |
| 请求超时 | `timeout` | false | 累积到阈值 |
| HTTP 401 / 403 | `auth` | false | 立即 30min cooldown |
| HTTP 429 | `rate_limited` | false | 立即 5min cooldown |
| HTTP 5xx | `server_error` | false | 累积到阈值 |
| Response JSON 解析失败 / 路径缺失 | `parse_error` | false | 累积到阈值 |
| `unhealthy_values` 匹配 | `business_unhealthy` | false | 累积到阈值 |
| `degraded_values` 匹配 | `business_degraded` | false | 不触发 cooldown（等同 degraded 状态）|
| HTTP 2xx + healthy_values 匹配（或无 mapping） | — | true | — |
| HTTP 2xx + 无 mapping + 无值匹配 | — | true | — |

注意：`record_observation_with_category()` 接收显式 `error_category`，不走现有 `failure_category()` 推导。`aggregate_api_health.rs` 中 `health_state_blocks_routing()` 已对 `cooldown_until > now` 做检查，`business_degraded` 类别不会设置 `cooldown_until`，因此不阻断路由，与现有降级语义一致。

---

## 7. 观测写入流程（AC-006）

```
execute_custom_monitor()
  → MonitorOutcome { ok, http_status, latency_ms, error_category, reason }
  → record_observation_with_category(
        storage,
        api_id     = definition.aggregate_api_id,
        trigger    = "custom_monitor",               // trigger_label → "自定义监控"
        ok, status, latency_ms, reason,
        error_category,
    )
  → aggregate_api_health_states upsert              // 写入已有表
  → aggregate_api_health_events insert              // 写入已有表（不含响应原文）
  → is_routing_blocked_with_storage() 自动生效     // 现有路由阻断语义，无需改动
```

**不变式**：启用的自定义监控独占 `(api_id, model=NULL, protocol=NULL)` 全局状态；自动内置健康轮询必须跳过这些 API。禁用或删除定义后，内置轮询恢复接管。手动探测在启用自定义监控时拒绝执行，避免临时覆盖该状态。

---

## 8. 调度器集成

新调度器：`ensure_custom_monitor_polling()`，与 `ensure_aggregate_api_health_polling()` 对等，在 `lifecycle/startup.rs` 中同时启动。

- 每 30 秒扫描 `enabled=1` 且 `next_run_at <= now` 的定义，按 `next_run_at` 升序每轮最多执行两条，与既有轮询吞吐保持一致。
- 每次执行后持久化 `next_run_at = now + schedule_interval_secs`；无需额外内存计数器，最小间隔已将单定义理论上限固定为每日 1440 次。
- 执行超时由 `timeout_ms` 控制；blocking 请求只占用专用调度线程，不阻塞 RPC/网关请求线程。
- `ensure_aggregate_api_health_polling()` 在列出可探测配置后排除已启用自定义监控的 API；`ensure_custom_monitor_polling()` 仅执行自定义定义。
- `next_run_at` 是定义表字段，不复用健康配置的 `next_probe_at`。

---

## 9. RPC 契约（新增 3 条）

```
aggregateApi/monitor/define    params: { id?, aggregateApiId, name, enabled, requestMethod,
                                         requestPath, requestBodyTemplate?, authMode,
                                         authHeader?, authHeaderPrefix?,
                                         responseMappingJson, scheduleIntervalSecs, timeoutMs }
                               result: CustomMonitorDefinitionResult (完整字段，不含 secret_value)

aggregateApi/monitor/list      params: { id: aggregateApiId }
                               result: { items: CustomMonitorDefinitionResult[] }

aggregateApi/monitor/delete    params: { monitorId }
                               result: { ok: true }
```

`aggregateApi/health/list`、`aggregateApi/health/get` 无变化；custom_monitor 写入的结果通过相同路径呈现，observationSource = "自定义监控"。

---

## 10. 前端数据流（AC-007）

```
/aggregate-api/page.tsx
  healthQuery (已有，每 15 秒)
    → accountClient.listAggregateApiHealth()
    → aggregateApi/health/list RPC
    → healthByApiId: Map<string, AggregateApiHealthSummary>
    
  表格行新增"健康状态"列（TableCell）：
    health = healthByApiId.get(api.id)
    → <HealthStateBadge state={health?.state} />
       + 最近观测时间 formatTsFromSeconds(health?.lastObservedAt)
       + latencyMs / httpStatus / errorReason（Tooltip 展示）
       + 来源标识（health?.observationSource）
       onClick → openHealthDetail(api.id)
  
  健康详情面板（Dialog/Sheet）：
    accountClient.getAggregateApiHealth(apiId)
    → aggregateApi/health/get RPC（已有）
    → AggregateApiHealthDetail { summary, config, states, events }
    → 展示：当前 summary、config（是否启用主动探测）、归一化字段和最近 N 条 events
    → events 行：trigger / outcome / stateBefore→stateAfter / httpStatus / reason / observedAt
    → 自定义监控定义：读取、创建/更新、启停和删除；表单只暴露方法、相对路径、静态 JSON 请求体、认证模式、映射与调度参数
```

**不新增路由**，不修改 `/plugins/` 页面逻辑。监控定义管理附加在 `/aggregate-api/` 详情面板；该表单是 Config Adapter 的唯一配置入口，不能要求用户通过 DevTools 或插件日志完成配置。

---

## 11. 安全与隐私（AC-005）

| 风险 | 对策 |
|---|---|
| 插件读取 aggregate_api_secrets | 不新增 Rhai 宿主函数；secrets 表不在 `list_app_settings_map()` 路径内 |
| 凭据泄露到日志或结果 | `execute_custom_monitor` 不打印秘密或响应原文；RPC/health event 仅返回脱敏短原因 |
| 认证 Header 注入 | HeaderName 校验、拒绝 CR/LF，Basic 模式不接受自定义 Header/prefix |
| 自定义监控错误写入健康状态 | service 内部校验 MonitorOutcome，拒绝 `state = "unhealthy"` 直接写入（须经过状态机计数阈值） |
| SSRF 或凭据经重定向外泄 | request_path 保持已保存 URL 的 origin，拒绝路径逃逸；HTTP client 禁止重定向 |
| 探测计费暴涨 | 每 API 仅一条定义、最小 60s 间隔、持久化 `next_run_at` |

---

## 12. 兼容性与迁移

- **无 breaking change**：`record_observation_with_storage()` 签名不变；新增 `record_observation_with_category()` 为纯扩展。
- **无前端健康结果类型变更**：`AggregateApiHealthSummary`、`AggregateApiHealthEvent` 字段足以展示归一化结果；新增定义管理类型仅用于 Config Adapter。
- **数据库迁移**：migration 137 仅新增表和唯一约束，不修改现有表。删除 `aggregate_api` 时 `ON DELETE CASCADE` 自动清理。
- **自动探测所有权明确**：启用的定义令内置轮询跳过对应 API；禁用/删除后恢复内置轮询，不产生两个自动来源竞争同一全局状态。

---

## 13. 备选方案及取舍

### A. Plugin Adapter（Rhai 执行解析逻辑）

研究结论：当前 Rhai 缺少 Header 支持、无凭据绑定接口、无写健康状态能力。补充这些能力需要：新增 `monitor:write` 宿主桥接 + 凭据令牌机制 + 沙箱隔离审计。安全风险与工程量均超过 MVP 范围。

**取舍**：将 Plugin Adapter 推迟到有明确需求且完成宿主能力令牌设计之后。本轮使用 Rust 内置 Config Adapter，插件仅保留日志/任务管理职责。

### B. 前端逐供应商 JSON 解析

直接在前端对不同供应商的原始状态 JSON 做 switch-case 解析。问题：
- 供应商 schema 变更需前端更新
- 状态显示与路由阻断语义分裂（前端红色徽章 ≠ service cooldown）
- 凭据必须传到前端才能请求需认证的接口

**取舍**：已明确拒绝，见 PRD AC-004 Must not。

### C. 在现有插件任务输出上附加健康写入桥接

由 service 监听插件任务完成事件，解析返回 JSON，判断是否符合 MonitorObservation 格式并写入健康状态。问题：输出格式无强约束，service 需要猜测插件意图；与 AC-004 "前端不为每个供应商维护私有 JSON 解析器"的精神冲突。

**取舍**：不采用。保持插件输出只作为原始日志。

---

## 14. 发布与回滚

**发布**：migration 137 在服务启动时自动应用（现有 `apply_sql_migration` 机制）；新调度器随 `lifecycle/startup.rs` 启动；前端健康列在 `/aggregate-api/` 页面增量呈现。

**回滚**：
- 前端回滚：恢复 `/aggregate-api/page.tsx` 到无健康列版本，不影响健康状态数据
- 后端回滚：停用 `ensure_custom_monitor_polling()` 启动调用，不影响现有 `record_observation_with_storage` 调用链
- 数据回滚：`DROP TABLE aggregate_api_custom_monitors` 后需要写反向 migration（无 data loss，因为 health_states/events 不区分来源，数据仍然有效）
- 灰度策略：`ensure_custom_monitor_polling()` 可以在不启动的情况下保持数据模型，待验证后再开启

---

## 15. 未决技术风险

| 风险 | 等级 | 说明 |
|---|---|---|
| SSRF via request_path | 低 | request_path 经过单斜杠、字符与 fragment 校验，以字符串拼接保持 AggregateApi.url 的 origin，且 client 禁止重定向；独立 URL 仍不在 MVP 范围 |
| ResponseMapping 路径解析边界 | 低 | 点分隔路径实现时需要处理数组索引、null 中间节点；MVP 不支持 JSONPath 表达式，仅支持简单点分隔 |
| 自定义监控遇到需要 `business_degraded` 不触发路由阻断的用户期望 | 低 | 当前设计 degraded 不设 cooldown_until 不阻断路由；用户若期望降级时也阻断需要额外配置项，MVP 不实现 |
| `trigger_label` 是私有函数，返回 `&'static str` | 低 | 新增 `"custom_monitor" => "自定义监控"` 需要在 `aggregate_api_health.rs` 内修改，是文件内改动，不影响接口 |
