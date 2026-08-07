# 既有工作与重叠风险研究

## 范围与证据等级

本文件只审阅与“余额为零时临时排除聚合 API 路由候选”直接相关的 Trellis 任务、研究产物、当前源码和近期提交。未修改产品代码，未执行构建、测试、lint 或项目级验证命令。

- **[已实现事实]**：当前源码或提交内容直接证明的行为；提交只证明代码进入该提交，不等同于本次研究实际运行验证。
- **[任务/设计提议]**：PRD、design、implement 或 research 中写明，但不能仅凭这些文件宣称已经实现的内容。
- **[约束/风险]**：应带入本任务总方案的兼容边界，不是实现建议。

所有四个历史任务的 `task.json` 当前仍为 `"status": "in_progress"`、`completedAt: null`、`commit: null`：

- `.trellis/tasks/07-15-aggregate-api-cooldown-ui/task.json:6,14,18`
- `.trellis/tasks/07-28-fix-aggregate-api-usage-refresh/task.json:6,14,18`
- `.trellis/tasks/07-31-upstream-api-capacity-recovery/task.json:6,14,18`
- `.trellis/tasks/08-03-aggregate-api-upstream-health/task.json:6,14,18`

因此，下面会把能由源码/提交复核的结果与任务文本中的计划严格分开；不能把这些任务的工作流状态视为正式完成证明。`docs/solutions/` 在本次定向检查中不存在，故无该目录下的方案产物可引用。

## 本任务已确认的基线

- **[任务事实]** `.trellis/tasks/08-06-disable-zero-balance-aggregate-api/prd.md` 的 R1/R2、Scope Boundaries 与 Open Questions 明确限定：只有系统成功取得并判定为零的余额才触发；无查询能力、缺值或查询失败不得触发；管理员可单独解除；人工启停和失败冷却不得被覆盖。该文档本身是当前规划输入，不证明功能已实现。
- **[已实现事实]** `crates/service/src/aggregate_api.rs:2558-2634` 的 `refresh_aggregate_api_balance` 已返回 `AggregateApiBalanceRefreshResult`。成功解析时会保存 `snapshot` 并以 `snapshot.is_valid` 决定 `ok`；查询错误会保存失败结果且返回 `balance: None`。该函数中没有改变路由候选或冷却状态的调用。
- **[已实现事实]** 可解析余额的语义可由 `crates/service/src/aggregate_api.rs:1008-1029` 与 `1110-1135` 交叉确认：`is_valid` 与数值 `remaining` 分开，且有效响应缺少 `remaining` 会报错。由此可知，数值零、缺失余额和查询失败已经是可区分的结果。

### 应保留的直接约束

1. 零余额规则的触发证据必须是一次成功、有效且具有数值 `remaining` 的余额快照；`remaining` 缺失、`is_valid == false`、刷新返回 `ok == false` 或调用报错都不能被推断为零余额。前半句是当前任务 PRD 的要求；后半句由上述现有返回模型支持。
2. 手动解除仅应撤销零余额临时状态；其结果只能是在**其他既有路由条件均允许**时重回候选，不能同时改变配置 `status`、失败冷却或健康状态。
3. 服务重启、刷新成功为非零以及过期余额的恢复语义目前仍是 `.trellis/tasks/08-06-disable-zero-balance-aggregate-api/prd.md` 的 Open Question；不得把健康模块的持久化策略当成该问题已经决定。

## 07-15：聚合 API 冷却 UI

### 来源与已落实行为

- **提交引用**：`86870f0b75ad31b6a214ccf00c65d30627e2ce32`（2026-07-17，`feat: add capability routing, cooldown UI, Grok billing, and model usage stats`）。提交统计显示其同时新增了该任务产物、冷却模块、RPC/Tauri/Web transport、页面、hook 和冷却测试。
- **[已实现事实]** `crates/service/src/gateway/routing/aggregate_api_cooldown.rs:11-14,17-51` 的 `AggregateApiCooldownKey` 用 `(api_id, upstream_model)` 区分状态；阈值为 5 次失败，冷却为 5 分钟，并且只在内存中维护。
- **[已实现事实]** `record_aggregate_api_failure`（同文件 `:199-244`）累积失败并在达到阈值后创建冷却；`clear_aggregate_api_cooldown`（`:246-255`）只清除该模型；`clear_aggregate_api_cooldowns`（`:257-265`）清除某 API 全部模型状态并删除关联 policy action。
- **[已实现事实]** 当前实际候选入口为 `crates/service/src/gateway/upstream/protocol/aggregate_api.rs:1406-1427`：对每个候选调用 `gateway_is_aggregate_api_in_cooldown`，命中后才从候选列表中滤出；候选全被滤除时在 `:1428-1459` 以 503 结束。这一过滤发生在随后日消费额过滤（`:1461-1512`）之前。
- **[已实现事实]** `crates/service/src/gateway/mod.rs:1300-1310` 的 `gateway_reset_aggregate_api_runtime_status` 复用“清除某 API 所有模型冷却”的语义。服务层 `reset_aggregate_api_runtime_status` 在 `crates/service/src/aggregate_api.rs:1964-1976` 先校验 API 存在；RPC 是 `aggregateApi/runtimeStatus/list` / `reset`（`crates/service/src/rpc_dispatch/aggregate_api.rs:55-61`）。
- **[已实现事实]** 既有冷却回归契约位于 `crates/service/src/gateway/routing/tests/aggregate_api_cooldown_tests.rs`：覆盖“五次后进入”、成功清除、reset 同时清除 policy action、遗忘陈旧失败、模型隔离及成功仅清除同模型（各 `fn` 位于 `:4-154`）。本次未运行这些测试。
- **[已实现事实]** 桌面与 service-mode Web 使用既有同一 RPC 名称：Tauri wrapper 为 `apps/src-tauri/src/commands/aggregate_api.rs:20-36`，Web command map 为 `apps/src/lib/api/transport-web-commands/aggregate-api.ts:5-7`，RPC dispatch 为上述 Rust 文件；这正是当前任务要求复用的 typed transport 链。
- **[已实现事实]** `apps/src/app/aggregate-api/page.tsx:214-219,456-465,965-1017,1241-1255` 使用独立 runtime-status 查询、倒计时、确认对话框、成功/失败 toast 和 `resetAggregateApiRuntimeStatus`。同页的配置开关仍使用 `api.status`（`:1146-1149`），因此目前 UI 已有“运行时路由状态”与“人工启用”分列的事实基础。

### 仅在任务文本中出现的决策

- `.trellis/tasks/07-15-aggregate-api-cooldown-ui/prd.md` R1–R3 与 `.trellis/tasks/07-15-aggregate-api-cooldown-ui/design.md` 描述了运行时快照字段、确认文案、约 2 秒轮询及布局；这些文本不能单独作为实现证明。
- `.trellis/tasks/07-15-aggregate-api-cooldown-ui/research/aggregate-api-table-ui-analysis.md` 推荐把“路由状态”放在 Guard 与“启用”之间、固定约 176px，并将原“状态”改为“启用”。这是可复用的视觉/术语决策，不是零余额功能已具备的证据。

### 与本需求的关系、重叠和保留约束

- **可复用决策**：单 API、确认后手动解除、明确成功/失败反馈、桌面/service-mode 共用 typed transport、独立运行态而非伪装为持久配置。
- **禁止语义合并**：零余额不是“连续上游请求失败”，也不是模型级失败冷却；不得调用现有 `runtimeStatus/reset` 作为零余额解除的同义词，否则会清除所有模型的真实失败冷却和 policy action。
- **UI 风险**：当前候选过滤和页面文案都把 `gateway_is_aggregate_api_in_cooldown` 的结果称作“冷却”。零余额若复用此布尔路径，会被错误地显示为冷却中；总方案需保留独立的可观察原因。

## 07-28：聚合 API 今日用量刷新

### 来源与已落实行为

- **提交引用**：`53943702e0ba11610e42905d85ba1dd5e558cef4`（2026-08-01，`feat: improve gateway usage billing and recovery`）。提交统计显示该任务的 artifacts、`apps/tests/aggregate-api-usage-refresh.spec.ts`、聚合页、`request_token_stats`、`requestlog/aggregate_api_daily_usage` 和 gateway 相关文件均进入同一提交。
- **[已实现事实]** `crates/service/src/usage/refresh/batch.rs:115-186` 在 polling cycle 同时调用用量刷新和聚合 API 余额刷新；余额轮询只挑选 `balance_query_enabled && status == active` 的 API。单个余额刷新失败只记录 warning/计数，当前循环没有在这里修改路由资格。
- **[已实现事实]** 聚合页手动“刷新余额”只会对余额查询已开启的 API 调用 `accountClient.refreshAggregateApiBalance`，然后失效 `aggregate-apis` 查询：`apps/src/app/aggregate-api/page.tsx:339-342,468-509,581-595,928-934`。同一页的余额呈现来源是持久化的 `lastBalanceJson`，由 `parseBalanceSnapshot` 解析（`:136-168`）。
- **[已实现事实]** 日用量的 Service RPC 是 `crates/service/src/requestlog/requestlog_aggregate_api_daily_usage.rs:9-38` 的 `read_aggregate_api_daily_usage_stats`；页面对应的 Playwright 用例在 `apps/tests/aggregate-api-usage-refresh.spec.ts:163-272`，只模拟 `requestlog/aggregate_api_daily_usage` 的变更与 keep-alive 页面恢复。它不是余额刷新或零余额候选资格的回归用例。
- **[历史调查事实，不可当作当前运行态]** `.trellis/tasks/07-28-fix-aggregate-api-usage-refresh/research/refresh-logic.md` 记录了 2026-07-28/29 的运行数据库与 RPC 对照，并把当时的问题定位为 TanStack Query 生命周期和后来发现的 `aggregate_api_id` 写入/汇总回退。这是当时证据，不证明今天的余额数据或刷新状态。

### 仅在任务文本中出现的决策

- `.trellis/tasks/07-28-fix-aggregate-api-usage-refresh/prd.md`、`design.md`、`implement.md` 的中心是“今日 Token/费用”刷新，并在 Out of Scope 中排除了供应商余额查询改动。
- 其中 `design.md` 的“右上角刷新余额仍只代表供应商余额”是该任务的范围决策；不能反向推导出余额刷新会影响路由。

### 与本需求的关系、重叠和保留约束

- **可复用事实**：余额刷新已有手动入口、后台轮询入口和返回模型；零余额状态应消费它们已经产生的结果，而不将“用量刷新”混进余额/路由语义。
- **冲突风险**：不能把余额不可得、invalid snapshot 或轮询失败当成零；这会违反当前任务 PRD，也会把 07-28 所保留的旧余额/刷新失败语义错误转为路由阻塞。
- **人工恢复约束**：刷新余额和解除零余额是不同动作。现有“刷新余额”会重新查询并更新配置数据；本任务要求的解除是管理员显式撤销临时路由排除。两者不能因为共用页面按钮而被宣称等价。

## 07-31：上游 API 容量错误自动恢复

### 来源与已落实行为

- **提交引用**：任务 artifacts 随 `53943702e0ba11610e42905d85ba1dd5e558cef4` 进入仓库；更早的通用前序提交为 `fdeae6cf1e4512c9f71b819c0f0559ae254bda4d`（2026-07-02，`feat: add upstream capacity same-candidate retry`）。后者的提交统计涉及通用 candidate executor/response finalizer，并非该 07-31 任务目录。
- **[已实现事实]** `53943702` 的统计列出 `crates/service/src/gateway/upstream/protocol/aggregate_api.rs`、其测试、`metrics.rs` 和 `aggregate_api_cooldown.rs` 的修改，因而能确认该时期确实提交了 Aggregate API 路径的容量恢复相关变更；不据此推断本次研究已运行其回归测试。
- **[已实现事实]** 当前 Aggregate API 候选循环在 `crates/service/src/gateway/upstream/protocol/aggregate_api.rs:1697-1805` 保留 immutable candidate body，并存在独立的 transport/capability retry budget。此事实与容量恢复“同候选、未向客户端可见前重放”的边界一致。

### 仅在任务文本中出现的决策

- `.trellis/tasks/07-31-upstream-api-capacity-recovery/prd.md` R1–R5 与 `design.md` 规定了精确容量错误、初次加两次同上游重放、不得切下一个候选、终态 503、不可在已向客户端交付输出后重放，以及“容量错误不创建 cooldown”。这些是已决定的任务设计；不能只靠设计文本证明每个验收场景均已验证。
- `.trellis/tasks/07-31-upstream-api-capacity-recovery/implement.md` 是实施清单，`check.jsonl`/`implement.jsonl` 未提供可作为本次结论的完整执行日志。

### 与本需求的关系、重叠和保留约束

- **语义分离**：容量是单次请求的专属恢复/终态策略，不是余额为零，也不应成为零余额状态的触发器或人工恢复目标。
- **候选顺序风险**：零余额过滤位于当前候选循环之前（`aggregate_api.rs:1406-1427`）；其作用只应是消除已知无余额的候选。对仍参与候选的容量错误，必须保留现有同候选重放、可见输出边界和“不静默模型切换”语义。
- **观测约束**：容量任务强调 trace/上游 ID/模型的排障而不记录 secret 或 body；零余额可观察性同样不能扩大敏感信息暴露面。

## 08-03：聚合 API 上游健康检测

### 来源与已落实行为

- **主提交引用**：`dc86f9fa331473d5ddfeba4b9b9d90df467c7eaa`（2026-08-04，`feat: add aggregate api upstream health monitoring`）。提交统计显示了 health migration、Core storage、`aggregate_api_health.rs`、Gateway、RPC、Tauri、Web transport、类型与页面修改。
- **后续提交引用**：`debd3b72c4ea601ca5a8d6e55a2508e0068c6d9f`（2026-08-05，`fix: keep disabled health checks out of routing`），修改 health gate、gateway 和 Aggregate API protocol；`129e4f7d03efc70dc37501377922e9ccf3eabebc`（2026-08-05）修复选择的 probe model；`f4f6fe1d2403811e006e7a6e61497858a0aed2fc`（2026-08-05）新增 probe costs。它们证明 health 行为在任务文档提交后仍有修正。
- **[已实现事实]** `crates/core/migrations/130_aggregate_api_health.sql:1-55` 建立 health config/state/event 三表；config 默认 `enabled = 0`，state 值独立于人工启停，event 有 trigger/outcome 约束并级联随 aggregate API 删除。
- **[已实现事实]** `crates/service/src/aggregate_api_health.rs:152-280` 的 `record_observation_with_storage` 按错误类别写入持久化 health state/event；成功清除该 health scope 的失败/冷却，失败可变为 `degraded` 或 `cooldown`。这是 health 的独立状态机，不是余额状态机。
- **[已实现事实]** `is_routing_blocked_with_storage`（同文件 `:299-335`）明确规定：被动观测可以可见，但 persisted health state **仅在该 source 主动监控 enabled 时** 才能从路由中排除；状态缺失或 config 查错会返回不阻塞。这一 gate 的测试是同文件 `persisted_cooldown_only_blocks_when_proactive_monitoring_is_enabled`（`:796-867`），本次未运行。
- **[已实现事实]** `crates/service/src/gateway/mod.rs:1248-1259` 将 legacy 内存 failure cooldown 与 health route gate 以 OR 合并为 `gateway_is_aggregate_api_in_cooldown`；实际聚合候选过滤通过此函数运行。名称虽为 cooldown，但此时已包含 persisted health 排除。
- **[已实现事实]** health 人工 reset 的边界比名称更宽：`crates/service/src/aggregate_api_health.rs:648-663` 先 reset persisted health state，随后调用 `gateway_reset_aggregate_api_runtime_status`；后者会清除 legacy failure cooldown（`crates/service/src/gateway/mod.rs:1305-1310`）。
- **[已实现事实]** health RPC 已沿现有 typed 链完整存在：Rust dispatch `aggregateApi/health/list|get|config/update|probe|reset`（`crates/service/src/rpc_dispatch/aggregate_api.rs:62-87`），Tauri wrapper（`apps/src-tauri/src/commands/aggregate_api.rs:40-107`），Web map（`apps/src/lib/api/transport-web-commands/aggregate-api.ts:8-13`），页面查询/呈现（`apps/src/app/aggregate-api/page.tsx:228-237,418-465,1037-1079`）。

### 仅在任务文本中出现的决策

- `.trellis/tasks/08-03-aggregate-api-upstream-health/prd.md` 与 `design.md` 规划了完整的错误分类、主动 probe、half-open、状态展示和持久化恢复策略。其中设计文档的多数细节是目标设计；当前源码应是判断具体已实现范围的依据。
- `.trellis/tasks/08-03-aggregate-api-upstream-health/research/reference-analysis.md` 是新健康设计前的 2026-08-03 研究基线，引用了更早 commit `477ce51...`。它的外部项目比较与推荐模型可用于理解背景，但不应当作今日当前行为的来源。
- `.trellis/tasks/08-03-aggregate-api-upstream-health/research/probe-cost-estimate.md` 是主动探测成本估算，不是余额刷新成本或零余额恢复规则。

### 与本需求的关系、重叠和保留约束

- **核心冲突风险**：现有 `gateway_is_aggregate_api_in_cooldown` 已把两种原因压为一个候选过滤布尔值。若零余额复用该路径而没有独立原因/状态，页面和日志会把“余额为零”误称为冷却，也会让人工恢复误触 health/legacy cooldown。
- **配置独立性**：health 的 route blocking 受 `activeProbeEnabled` 控制；余额为零的规则受 `balanceQueryEnabled` 和成功余额结果控制。这两个开关的意义、生命周期和默认值不同，不能互相作为前置条件或替代。
- **人工恢复独立性**：不能复用 `aggregateApi/health/reset` 作为零余额解除，因为当前实现会联动清除 legacy runtime cooldown；这直接违反本任务“与失败冷却状态保持语义分离”的要求。
- **可复用决策**：沿用按 source/model 管理状态、将可读原因和人工配置分开、使用现有 Rust dispatch → Tauri → Web command map → account client 的 typed 链，且不向浏览器返回 secret/原始上游响应。

## 近期提交时间线与结论

| 日期 | 提交 | 可确认内容 | 对本任务的含义 |
| --- | --- | --- | --- |
| 2026-07-17 | `86870f0b` | 冷却 runtime status、复位 UI、transport 与测试进入提交 | 可复用手动解除交互和 typed 链；不能把零余额伪装成失败冷却。 |
| 2026-08-01 | `53943702` | 用量刷新任务 artifacts、余额/用量相关页与 Aggregate API gateway 恢复改动进入提交 | 余额刷新已有结果源；用量刷新不等于余额路由治理。 |
| 2026-08-04 | `dc86f9fa` | persisted health storage/RPC/UI 进入提交 | 已有第三种路由相关状态，必须独立于新状态。 |
| 2026-08-05 | `debd3b72` | disabled health checks 不再阻塞路由 | 不可破坏“观测可见”与“路由可阻塞”的独立开关语义。 |
| 2026-08-05 | `129e4f7d` / `f4f6fe1d` | probe model 修复与 probe cost 追踪 | 健康主动探测仍在演进；与余额查询/零余额无直接等价关系。 |

## 总方案必须保留的约束清单

1. **四类状态不可互相覆盖**：人工 `AggregateApi.status`、legacy failure cooldown、persisted health、零余额临时排除均需保持可区分原因与独立解除语义。前三者已由源码证明存在；第四者是本任务新增目标。
2. **候选资格是组合条件**：零余额解除只取消自身的过滤；若人工 disabled、legacy cooldown、health gate、日消费额限制、capability 路由或其他现有条件仍拒绝候选，则不能承诺重新路由。
3. **余额未知默认不参与新规则**：以成功且有效的数值零作为唯一触发信号；解析失败、无 `remaining`、invalid 或旧快照均不能成为“零”。
4. **余额刷新与人工解除不等价**：刷新是查询并写入余额结果；解除是管理员对临时路由状态的明确操作。刷新后的非零或重新零余额如何影响已解除状态，须在本任务总方案明确，而不是沿用健康 reset 的副作用。
5. **现有观察/错误语义不得扩张**：容量错误、普通失败、健康 probe 与余额查询都有不同分类与恢复路径；不能把余额规则接到 failure counter、capacity retry 或 health probe enabled 上。
6. **UI 必须避免误导**：现有“路由状态”可展示多种运行态，但文案必须能明确说明“因零余额临时排除”，不能显示为“冷却中”或“已停用”；人工“启用”仍表示配置状态。
7. **跨端链路不另起旁路**：桌面和 service-mode Web 都应走已经存在的 typed Rust RPC dispatch、Tauri command wrapper、Web command map 和 account client 链；避免 raw fetch 和浏览器持有 secret。
8. **可观察性与敏感信息边界**：仅暴露可解释的状态/时间/原因，不返回 API secret、鉴权参数、完整上游响应或请求体。这与 07-15、07-31 和 08-03 的既有边界一致。
9. **状态持久化/过期尚未决定**：当前 health 的 migration/persisted state 是现成先例，不是对零余额状态的既定决定。总方案必须显式决策服务重启、余额刷新延迟、状态过期和手动解除后的再次检查规则。
10. **验证范围需要分开**：既有用量刷新测试只覆盖统计显示，冷却测试只覆盖失败冷却；零余额路由排除、未知余额 fail-open、手动仅解除零余额、跨端 transport 及与其余 gates 共存都需要作为独立可观察契约，不能宣称由既有测试覆盖。

## 未证实项

- 未从现有源码发现“余额为零”的独立 runtime/persisted 路由状态或专用人工解除 RPC；这是本任务要规划的能力，不应描述为已经存在。
- 未执行任何提交中的测试、构建或运行态探测；所有“已实现事实”仅基于定向源码与 `git show --stat` 审阅。
- 现有 task metadata 未标记完成；提交包含实现文件不构成 Trellis 任务验收已完成的证明。
