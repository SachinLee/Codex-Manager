# 技术设计

## 1. 边界与总体架构

新增 `aggregate_api_health` 领域模块，放在 `crates/service/src/aggregate_api_health.rs`，负责探测调度、观测归一化、状态机和路由治理；不把逻辑继续堆入已有的大型 `aggregate_api.rs`。

```text
gateway attempt outcome ─┐
                         ├─> HealthObservation ─> classifier ─> state reducer
scheduled/manual probe ──┘                              │
                                                        ├─> SQLite current state + event history
                                                        ├─> in-memory route gate / cooldown
                                                        └─> RPC summary -> desktop/web UI
```

职责边界：

- `crates/core/`：迁移、SQLite storage structs/query、RPC DTO；不执行网络请求。
- `crates/service/`：复用 `aggregate_api.rs` 的 provider probe 和 `gateway::upstream_client_for_aggregate_url`，执行调度与路由 gate。
- `apps/src-tauri/`：仅新增集中式 RPC command wrapper，不写检测逻辑。
- `apps/src/lib/api/`：typed wrapper、web command mapping、normalizer。
- `apps/src/app/aggregate-api/` 与 `apps/src/components/aggregate-api/`：状态徽章、详情历史、配置和手动动作。

## 2. 数据模型与迁移

新增 migration `117_aggregate_api_health.sql`（实际编号以实现时最新 migration 为准）：

### `aggregate_api_health_configs`

- `aggregate_api_id TEXT PRIMARY KEY REFERENCES aggregate_apis(id) ON DELETE CASCADE`
- `enabled INTEGER NOT NULL DEFAULT 0`
- `probe_interval_secs INTEGER NOT NULL DEFAULT 900`（最小 60，默认 900）
- `probe_timeout_ms INTEGER NOT NULL DEFAULT 30000`（范围 1000..60000）
- `probe_model TEXT NULL`（空值表示从 enabled model catalog 选择代表模型）
- `last_scheduled_at INTEGER NULL`, `next_probe_at INTEGER NULL`, `updated_at INTEGER NOT NULL`
- 每个现有 source 用 `INSERT ... SELECT` 创建默认关闭配置；source 删除级联清理。

### `aggregate_api_health_states`

以 `(aggregate_api_id, upstream_model, protocol)` 为主键；source 级状态用空 `upstream_model` 和空 `protocol` 表示。

- `state TEXT NOT NULL CHECK (state IN ('unknown','healthy','degraded','unhealthy','cooldown','recovering'))`
- `consecutive_failures INTEGER NOT NULL DEFAULT 0`, `consecutive_successes INTEGER NOT NULL DEFAULT 0`
- `failure_threshold INTEGER NOT NULL DEFAULT 5`
- `cooldown_until INTEGER NULL`, `half_open_at INTEGER NULL`
- `last_observed_at INTEGER NULL`, `last_probe_at INTEGER NULL`, `last_success_at INTEGER NULL`, `last_failure_at INTEGER NULL`
- `last_latency_ms INTEGER NULL`, `last_http_status INTEGER NULL`
- `last_error_category TEXT NULL`, `last_error_reason TEXT NULL`, `last_observation_source TEXT NULL`
- `updated_at INTEGER NOT NULL`

### `aggregate_api_health_events`

保留可解释历史，不保存 secret/header/full body：

- `id INTEGER PRIMARY KEY`, `aggregate_api_id TEXT NOT NULL REFERENCES aggregate_apis(id) ON DELETE CASCADE`
- `upstream_model TEXT NULL`, `protocol TEXT NULL`
- `trigger TEXT CHECK (trigger IN ('passive','scheduled_probe','manual_probe','half_open'))`
- `outcome TEXT CHECK (outcome IN ('success','failure','ignored'))`
- `state_before TEXT`, `state_after TEXT`, `error_category TEXT`, `http_status INTEGER`, `latency_ms INTEGER`
- `reason TEXT`（最多 500 bytes，经过 sanitize/truncate）、`observed_at INTEGER`, `cooldown_until INTEGER NULL`
- 索引 `(aggregate_api_id, observed_at DESC)`、`(aggregate_api_id, upstream_model, observed_at DESC)`。
- 每个 source 默认最多保留最近 500 条或 30 天记录，维护任务分批裁剪。

### 与旧字段/状态的兼容

- 保留 `aggregate_apis.status` 作为人工 active/disabled。
- 保留 `last_test_*`；手动检测和主动 probe 都更新它，以兼容现有列表与旧客户端；新 UI 优先读取 health summary。
- 现有内存 `AggregateApiCooldownState` 迁移为 health reducer 的快速 route gate：启动时加载未过期 persisted states，状态变更以事务写 SQLite 后再更新内存；旧 RPC 字段继续返回，增加可选 health 字段。

## 3. 统一观测与状态机

```rust
struct HealthObservation {
    api_id: String,
    model: Option<String>,
    protocol: Option<String>,
    trigger: ObservationTrigger,
    outcome: ObservationOutcome,
    category: ErrorCategory,
    http_status: Option<u16>,
    retry_after_secs: Option<i64>,
    latency_ms: Option<i64>,
    reason: Option<String>, // sanitized
    observed_at: i64,
}
```

错误分类和治理范围：

| 类别 | 典型信号 | scope | 治理 |
| --- | --- | --- | --- |
| `request_scoped` | 输入校验、客户端取消、内容/参数错误 | none | 写 `ignored`，不影响健康 |
| `auth` | 401/403、明确 token/key rejected | source | 单次确认后 30 分钟 cooldown |
| `model_not_supported` | 404 或 provider 明确不支持模型 | model | 12 小时模型 cooldown |
| `rate_limited` | 429 | model/source | 优先 Retry-After，否则指数退避，最大 30 分钟 |
| `transient` | timeout、DNS/TLS、408、500/502/503/504 | model | 连续 5 次才进入 5 分钟 cooldown |
| `other_upstream` | 其他上游错误 | model | 连续 5 次才进入 cooldown，并降低置信度 |

状态 reducer 规则：

1. `unknown -> healthy`：首次成功；`unknown -> degraded`：成功但延迟超过阈值（默认 6 秒）。
2. 成功：清除对应 scope 的失败计数与过期 cooldown；若 source scope 无 active model 阻塞则恢复 `healthy/degraded`。
3. 单次 transient/other：状态变为 `degraded` 或保持原状态，不摘除；第 5 次连续失败进入 `cooldown`。
4. auth：source scope 进入 `cooldown`，阻止该 source 全部候选；人工 `status` 仍为 active。
5. model_not_supported/rate_limited：只阻止对应 model；source 仍可服务其它 model。
6. cooldown 到期：状态变为 `recovering`，只允许一个 half-open probe/真实请求；成功恢复，失败按错误分类重新计算 cooldown。
7. `unhealthy` 用于确定性失败或健康摘要中无可用 scope；路由 gate 对 `cooldown/unhealthy` fail-closed，对 `unknown/degraded` fail-open。
8. reducer 写 event + state 使用同一 SQLite transaction；内存 route gate 更新失败时只记录日志，不阻塞请求处理。

## 4. 探测器与调度

- 将现有 `test_aggregate_api_connection` 拆为可复用 `probe_aggregate_api(api_id, model, trigger)`，保留 Codex/Claude/Gemini provider adapter 和现有 client/header 构造。
- 主动探测上线前必须给所有支持的 probe body 设置输出上限：Claude/Gemini 使用 `max_tokens/maxOutputTokens=1`，Codex Responses/Chat Completions 使用 `max_output_tokens/max_tokens=1`（必要时 provider-specific fallback）；不能仅依赖读取首个流式 chunk 控制账单。
- 代表模型选择顺序：配置 `probe_model` -> 当前 enabled model catalog 的最高优先级 route -> 已有 `model_override` -> 返回 `unknown/no_model`，不发空模型请求。
- 健康 source 默认不主动探测；配置启用后，healthy 目标每 900 秒、degraded 每 300 秒调度一次。调度延迟加 0..10% jitter，避免同一时刻打满供应商。
- 增加成本保险丝：单 source 每日主动 probe 默认上限 288 次（相当于 degraded 持续 24 小时的上限），达到上限后暂停普通 probe，仅保留人工 probe/到期 half-open；每日按本地日期滚动重置。
- cooldown/half-open 不等待普通周期：`next_probe_at` 到期后优先发一个 half-open probe；同一 `(api, model, protocol)` single-flight。
- worker pool 默认 2，单 probe 总超时受 `probe_timeout_ms` 和全局硬上限约束；响应体最多读取 64 KiB；不在 scheduler 线程执行网络 I/O。
- 复用已有 background task 启动方式，在 `lifecycle/startup.rs` 增加 `ensure_aggregate_api_health_polling()`；动态设置变化通过现有 settings signal 让 loop 重新计算 delay。
- 仅扫描 `status=active` 且 `health_config.enabled=1` 的 source；配置关闭立即取消待执行 probe，但不清除被动状态。
- 失败退避遵循现有 dynamic polling helper；单个 source 失败不拖慢其它 source，worker panic 必须隔离并释放 single-flight 标记。

## 5. 被动观测接入与路由

- 在 gateway attempt outcome 已确定 HTTP/status/category 后调用 `record_aggregate_api_health_observation`；不得在请求 body 尚未分类时猜测。
- 复用现有 `record_aggregate_api_failure/clear_aggregate_api_cooldown` 的调用点，改由 health reducer 产生相同 route gate 结果，避免两套阈值分叉。
- candidate selection 检查顺序：人工 disabled -> source health gate -> model health gate -> 现有 capability/cooldown/额度规则；健康组件不可用时对 unknown fail-open。
- source 级 cooldown 的 reason 汇总 auth/category；model 级 cooldown reason 包含 upstream model。UI 不展示 secret 或完整上游响应。
- 手动恢复只清除 runtime/persisted health cooldown 和失败计数，不改人工 status；下一次真实请求/主动 probe 仍会重新观测。

## 6. RPC/API 合约

新增并保持现有 underscore/camelCase 映射：

- `aggregateApi/health/list`：返回每个 source 的 `adminStatus`、`healthState`、`effectiveRouteState`、`lastObservedAt`、`lastProbeAt`、`latencyMs`、`errorCategory`、`consecutiveFailures`、`cooldownUntil`。
- `aggregateApi/health/get`：返回 source summary、按 model/protocol 的 states、配置和最近事件（默认 50，最大 200）。
- `aggregateApi/health/config/update`：`id`, `enabled`, `intervalSecs?`, `timeoutMs?`, `probeModel?`；服务端验证范围和 source 存在性。
- `aggregateApi/health/probe`：`id`, `model?`, `trigger=manual`；返回一次 `HealthProbeResult`，并更新旧 `last_test_*`。
- `aggregateApi/health/reset`：`id`, `scopeModel?`, `scopeProtocol?`；清理指定或全部 runtime/persisted cooldown。
- 现有 `aggregateApi/runtimeStatus/list/reset` 保持兼容，映射到 health summary/legacy runtime fields。

链路同步：Rust dispatch -> Tauri `commands/aggregate_api.rs` -> `transport-web-commands/aggregate-api.ts` -> `account-client.ts` -> `apps/src/types` normalizer。禁止新增 raw `fetch`。

## 7. UI 设计

- 聚合 API 列表增加一个主状态 badge：内部 `healthy` 显示“推荐使用”，`unknown` 显示“可用”，`degraded` 显示“需注意”，`unhealthy` 显示“不可用”，`cooldown/recovering` 显示“冷却中”；旁边显示人工 disabled、余额状态，三者分开。
- 详情区域显示代表模型、探测开关/频率、最近主动/被动来源、延迟、错误类别、连续失败、冷却截止和最近 3-5 条事件；文本原因可复制但已脱敏。
- 操作按钮：`立即检测`、`解除冷却`、`保存探测设置`；pending/失败/空数据状态完整处理。
- MVP 不增加通知设置或图表依赖；历史先以最近事件列表和 24 小时成功率/平均延迟摘要呈现。

## 8. 安全、可观测性与回滚

- 所有外部请求通过现有 `upstream_client_for_aggregate_url` 和 auth header helper；探测配置不得允许任意 URL/代理绕过既有边界。
- 错误 reason 做状态码保留、敏感 header/token 清洗、最大字节截断；禁止保存 response body。
- 增加 health probe attempts/success/failure、latency、state transition 日志/metrics，日志字段只用 source id/model/category。
- 迁移可前向应用；回滚先关闭 scheduler、停止写入，再保留未知新表（SQLite 不做破坏性 down migration）。旧客户端继续读取 `last_test_*` 和 runtime fields。
- 健康检测是辅助路由治理，任何 storage/worker 错误都 fail-open 到现有 gateway 行为并记录可观测错误。
