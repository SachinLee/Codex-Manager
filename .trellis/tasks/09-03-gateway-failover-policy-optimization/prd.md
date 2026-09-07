# 优化网关上游错误分类与故障切换策略

## Goal
统一 Aggregate API 与账号池上游失败的决策边界，避免不可恢复的请求级 4xx 消耗同候选重试预算，同时保持真正的候选级故障可以 failover，并修正容量恢复的既有终态契约。用户价值是减少无效上游请求、降低故障窗口延迟，并让 502/503 的来源和重试行为可解释。

## Background and confirmed facts
- `aggregate_api.rs` 当前为普通 Aggregate API 非成功响应设置通用传输重试预算；需要用回归测试确认 400、401、403、404、429 和 5xx 的精确请求次数及候选推进路径。
- Aggregate API 已有独立的选定模型容量错误识别、最多两次同候选重放、jitter backoff、截止时间检查和容量指标；服务规范要求容量错误不进入通用重试、不冷却来源、不切换后续 Aggregate API，耗尽后返回 503。
- `route_exhaustion_terminal_status` 将路由耗尽类 404/503 归一化为 502；本任务不改变该兼容性行为。
- 账号池使用 `analyze_gateway_error` / `failover_policy` 决定 failover、账号不可用和 cooldown；Aggregate API 与账号池不应通过复制字符串判断建立第二套不一致的通用策略。
- 当前工作区已有其他 WIP 改动，实施时必须在现有改动上工作，不能回滚或覆盖无关修改。
- 会话级 Aggregate API 亲和性、候选尝试结构化摘要和相关存储由 `09-03-request-log-cache-failover-optimization` 管理；本任务不重复实现这些功能，但可消费其最终的尝试摘要契约。

## Requirements

### FR-1: Typed and shared upstream failure decision
建立一个由状态码、已识别的容量/能力/挑战语义、响应是否已交付和是否存在后续候选共同决定的失败决策边界。决策必须明确区分：同候选内部重试、推进到下一个候选、终止当前请求、以及仅更新健康/观测状态。不得通过泛化的 `status.is_success()` 分支让所有非成功响应共享同一预算。

### FR-2: Request-level 4xx does not consume retry budget
普通请求级 400/422（包括请求体或协议无法由切换候选修复的错误）不得在同一候选上消耗通用传输重试预算，也不得因无意义的候选轮转而重复发送相同请求；最终状态码继续沿用当前状态码映射。已有明确的 ChatGPT 特殊兼容重试、能力降级重试和 413 大请求体终态必须保持独立且优先级不变。

### FR-3: Candidate-level failures fail over without same-candidate waste
候选特有的 401/403、404/405/501、非容量 429、挑战/临时不可用等错误，若当前已有候选且请求尚未向客户端交付语义内容，应按既有健康、冷却、能力过滤和候选顺序推进到下一个允许候选，不重复消耗同候选的通用预算。HTTP 502 但错误体/错误码明确为 `rate_limit_exceeded` 的情况也属于候选级限流，必须按 429 语义 failover，而不能仅按外层 502 归为 `Other` 后直接返回。没有后续候选时，使用当前终态归一化逻辑。5xx、连接错误和可恢复超时是否保留现有同候选传输重试次数，必须由回归测试锁定并在设计中说明，不得因本任务隐式改变。

### FR-4: Capacity recovery remains an independent policy
精确匹配的选定模型容量错误继续使用独立容量预算：初始请求后最多两次同候选重放；不消耗通用传输重试，不触发普通候选 failover，不进入普通来源 cooldown。客户端已开始收到可交付内容后不得重放。预算耗尽返回规范要求的 503 容量错误；等待超过请求截止时间返回现有超时终态。近似文本不得误分类为容量错误。

### FR-5: Preserve public status and routing compatibility
不改变 `ordered`/`balanced` 的基础排序、不改变显式 Aggregate API 的语义、不改变账号池与 Aggregate API 的路由入口，不改变 `route_exhaustion_terminal_status` 的 404/503 到 502 兼容映射。任何状态码变化只能来自已有容量契约修正或现有超时/终态分支的明确规范化，且必须有测试证明。

### FR-6: Observable, bounded decisions
每次失败决策至少能在现有 trace/request attempt 观测中区分候选 ID、候选位置、HTTP 状态或传输类别、决策（retry same / failover / terminal）及是否为容量/能力/已交付后的特殊路径。不得记录 prompt、Authorization、密钥、完整上游 URL 查询参数或完整敏感错误正文。若 `09-03` 的结构化尝试摘要已落地，优先复用其类型和字段。

## Out of scope
- Aggregate API 会话亲和性、绑定存储、TTL、候选重排和新的请求日志数据库列；这些属于 `09-03-request-log-cache-failover-optimization`。
- 跨供应商或跨账号 prompt cache 共享。
- 修改 Aggregate API active/sort/凭据、自动下线 `sort=105` 或提升 `sort=400`；这些是运维动作。
- 新增等待型普通重试、修改全局路由排序、改变客户端错误码兼容契约。
- 新增公开 RPC、前端设置或 Prometheus 维度；已有容量指标和 trace 字段只做必要的决策验证。

## Acceptance Criteria
- [ ] Aggregate API 的 400/422、401/403、404/405/501、429 非容量错误分别有回归测试，测试断言同候选请求次数、候选推进/终止结果和最终状态码。
- [ ] 普通 4xx 不会错误消耗 `AGGREGATE_API_RETRY_ATTEMPTS_PER_CHANNEL`；已有明确特殊重试仍通过独立测试。
- [ ] 5xx、连接错误、可恢复超时的实际预算和 failover 行为与基线一致，或在测试与设计中明确记录经过批准的变化。
- [ ] 容量错误覆盖初始请求加两次重放、耗尽后 503、截止时间已过不等待、来源不 cooldown、不推进后续候选，以及已交付内容后不重放。
- [ ] 近似容量错误、能力错误和普通 502 不会被误归为容量错误；能力路径继续健康中立并使用独立预算。
- [ ] 空候选、候选耗尽和混合路径继续使用当前 `route_exhaustion_terminal_status` 兼容映射。
- [ ] 观测证据能区分 retry same、failover、terminal 和 skipped，并通过测试证明不泄漏 prompt、凭据或完整敏感 URL。
- [ ] 受影响的 Aggregate API、账号池/上游流程和 core 相关测试通过；至少运行 `cargo test -p codexmanager-service`，并按实际改动运行最窄的 core 测试。

## Key decisions
- 本任务优先修复可由代码确定的预算与分类错误，不把所有 4xx 粗暴视为终止，也不把所有 5xx 粗暴视为立即 failover。
- 容量错误严格遵循现有服务规范：同候选恢复、独立预算、耗尽 503、禁止普通 failover。
- 最终状态码归一化保持不变；状态码兼容性另行评估。
- 本任务不依赖 `09-03-request-log-cache-failover-optimization` 的 `aggregate_api_bindings` 表或亲和性逻辑。
- 可复用已稳定的 `request_logs.attempted_aggregate_api_ids_json` 字段记录候选尝试链；该字段在 migration 039 中已添加，早于亲和性表结构。
- 不读写 `aggregate_api_bindings` 表，避免与 09-03 的会话级亲和性逻辑冲突。

## Blocking open questions
无。业务优先级和状态码范围已确定；普通 5xx/transport 的保留行为由基线测试与设计阶段锁定，不构成需要用户决策的产品问题。

## Planning status
需求阶段完成，下一步生成 `design.md` 和 `implement.md`，然后提交最终计划摘要，等待用户明确批准后才可启动实现。
