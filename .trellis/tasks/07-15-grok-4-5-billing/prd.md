# Grok 4.5 请求计费

## Goal

让 CodexManager 对 `grok-4.5` 请求生成非零、可审计且尽量贴近 xAI 实际账单的费用记录。直接 xAI 响应应优先采用官方返回的实际请求费用；当聚合供应商或协议转换没有保留实际费用字段时，系统使用官方 token 价格进行本地估算，并明确标识估算完整性。

## Background

- 当前内置价格规则没有任何 `grok-*` model seed。Aggregate API 模型发现会为未知模型自动创建 enabled exact 零价占位规则；本机运行库中的 `grok-4.5` 正匹配 `aggregate_api_sync` 规则，其 input/cached/output 费率全为 `0.0`，因此 pricing snapshot 虽显示 `price_status = ok`，总费用仍为 `0.0`（`crates/service/src/apikey/apikey_models.rs:1026-1077`、`crates/service/src/quota/model_pricing.rs:1023-1077`）。没有该占位规则时，`resolve_model_price` 才会返回 missing，并由 request log 的兼容逻辑落为 `0.0`（`crates/service/src/gateway/observability/request_log.rs:308-318`）。
- xAI 官方 Grok 4.5 标准价格为：prompt `<200K` 时 input `$2.00`、cached input `$0.50`、output/reasoning `$6.00`；prompt `>=200K` 时分别为 `$4.00`、`$1.00`、`$12.00`，单位均为 USD / 1M tokens。
- xAI Priority Processing 对所有 token 类型收取标准价格的 `2x`，仅当响应确认 `service_tier = "priority"` 时生效。
- xAI 每个 inference response 的 `usage.cost_in_usd_ticks` 表示该请求的实际扣费，`1 USD = 10,000,000,000 ticks`；该值已包含 prompt caching、reasoning 和 server-side tool invocation 成本。
- xAI Chat Completions 与 Responses 分别通过 `usage.prompt_tokens_details.cached_tokens` 和 `usage.input_tokens_details.cached_tokens` 暴露缓存命中；当前共享 parser 已覆盖这两个字段。
- 当前 `UpstreamResponseUsage`、请求计价快照与前端请求日志没有 provider-reported cost 字段，无法区分实际费用与本地估算（`crates/service/src/gateway/observability/http_bridge/aggregate/output_text.rs:15`、`crates/core/src/storage/mod.rs:522`）。
- 当前通用长上下文判断使用 `input_tokens > threshold`；xAI Grok 4.5 的官方边界是 `>= 200K prompt tokens`，不能通过伪造 `199999` 阈值绕过数据模型缺口。
- 官方证据与实现影响记录在 `research/xai-grok-4-5-pricing-2026-07-15.md`。

## Requirements

- R1. 以 xAI 官方 model、pricing、cost tracking 与 prompt caching 文档作为 Grok 4.5 价格和 usage 语义来源。
- R2. 覆盖 `grok-4.5`、`grok-4.5-latest` 与当前指向 Grok 4.5 的 `grok-build-latest`；alias 变更后不得依赖旧本地价格掩盖 provider-reported cost。
- R3. 内置价格规则新增 provider `xai`，支持 Standard 与 Priority 两种 billing mode。
- R4. Standard 本地估算必须实现 `<200K` 的 `$2.00 / $0.50 / $6.00` 和 `>=200K` 的 `$4.00 / $1.00 / $12.00`；Priority 在响应确认后对相应档位应用 `2x`。
- R5. 价格规则必须能显式表示 inclusive 长上下文边界；旧规则默认保持 exclusive `>` 语义，不能破坏 GPT-5.6 的 `>272K` 合同。
- R6. Chat Completions 与 Responses 的流式、非流式路径必须采集并传播 `usage.cost_in_usd_ticks`；流式只采用终态 usage，不累加 running snapshot。
- R7. `cost_in_usd_ticks` 只接受非负、可表示的整数；缺失或非法值不得产生负费用或溢出，必须回退到本地估算。
- R8. xAI 本地兜底计价必须覆盖 cached input、completion output 与 reasoning output。reasoning 不得漏算，也不得在已包含于 output total 时重复计费。
- R9. 最终费用选择顺序为 `cost_in_usd_ticks`、兼容的 provider cost 字段、本地价格估算。provider-reported cost 与本地 estimate 必须同时保留用于审计。
- R10. request pricing snapshot 必须记录 `cost_source`、原始 provider cost、local estimate、最终费用与二者偏差；旧历史记录继续按 nullable/default 值读取，不回填、不改账。
- R11. `estimated_cost_usd` 作为兼容字段继续存在；新请求写入最终有效费用，同时由 `cost_source` 说明它是 provider actual 还是 local estimate。
- R12. Aggregate API 的 `cost_multiplier` 只能应用一次。快照保留 multiplier 前 provider cost，并将 multiplier 后的最终费用用于现有请求日志与默认 wallet charging。
- R13. 存在显式 `billing_model_slug` 时保留现有 wallet re-rating 业务规则；没有显式 re-rating 时优先采用 provider-reported cost。
- R14. server-side tools 或 usage violation 等非纯 token 成本由 provider-reported cost 覆盖；若实际费用缺失且已知使用了工具，本地 token estimate 必须标记为 `partial`，不得声明为完整账单。
- R15. 请求日志 UI 必须展示“官方实际费用”或“本地估算”来源，并继续展示 context band、billing mode、matched rule 与 price status。
- R16. 新增存储/RPC 字段必须 additive、nullable/defaulted，兼容旧数据库、旧请求日志与 Web/Desktop 两种运行方式。
- R17. Grok 计费改动不得改变现有 OpenAI、Anthropic、Gemini 价格匹配、cache-write、长上下文和 wallet re-rating 结果。
- R18. 本任务只交付迁移代码与应用逻辑，不直接重算用户历史日志、钱包余额或第三方账单。

## Acceptance Criteria

- [ ] `grok-4.5`、`grok-4.5-latest` 和 `grok-build-latest` 均匹配 provider `xai` 的官方价格规则。
- [ ] 旧数据库中已存在的 `aggregate_api_sync` Grok 零价 exact 占位规则不会遮蔽新版 official seed；升级后无需用户删库或手工改价即可产生非零费用。
- [ ] Standard 请求在 `199999` prompt tokens 使用短档，在 `200000` prompt tokens 使用长档；GPT-5.6 的 exact-threshold 行为保持不变。
- [ ] Priority 只在 effective response tier 为 `priority` 时使用 `2x`，default/auto 回落请求按 Standard 计费。
- [ ] Chat Completions 与 Responses 的 non-stream JSON 和最终 SSE usage 都能采集 `cost_in_usd_ticks`。
- [ ] `cost_in_usd_ticks = 37,756,000` 精确换算为 `$0.0037756`（展示可按 UI 精度四舍五入）。
- [ ] 流式多个 running cost snapshot 不相加，最终快照决定 provider actual cost。
- [ ] 非法、负数或缺失 provider cost 自动回退本地估算并保留明确状态。
- [ ] xAI cached input 按短/长档 `$0.50/$1.00` 计费，普通 input、cached input 互斥且不双算。
- [ ] xAI reasoning token 在本地兜底中按 output 费率计费；含 `total_tokens` 与不含 `total_tokens` 的 payload 均无漏算或双算。
- [ ] 有 provider actual 时最终请求费用以 actual 为基准；local estimate 与 variance 仍持久化可审计。
- [ ] 有 server-side tool 且 provider actual 缺失时 price status 为 `partial`；有 actual 时工具费无需本地重复推导。
- [ ] request pricing snapshot 和 RPC/UI 能区分 `provider_reported` 与 `local_estimate`。
- [ ] Aggregate API multiplier 与 wallet charging 仅应用一次；显式 billing-model re-rating 保持原行为。
- [ ] 历史无新增字段的日志可以正常查询，费用不会自动重算或覆盖。
- [ ] 模型目录/自定义价格 UI 能正确显示 Grok provider、两档价格和 inclusive threshold 语义。
- [ ] 相关 Rust 单元/集成测试、RPC 序列化测试、前端 build/runtime tests 通过，现有 GPT-5.6 计价回归无变化。

## Out of Scope

- Grok 4.3、4.20、grok-build-0.1、Imagine、Voice 等其他 xAI 模型的完整价格导入。
- Batch API 计价；官方当前明确 `grok-4.5` 不支持 Batch API。
- 在 provider actual 缺失时，根据最终文本反推未知 server-side tool 类型或调用次数。
- 自动同步 xAI 远程价格表或在运行时覆盖用户自定义规则。
- 对历史 Grok `$0` 日志执行推测性 backfill、钱包追扣或第三方账单对账。
- 直接修改用户 SQLite 数据库。
