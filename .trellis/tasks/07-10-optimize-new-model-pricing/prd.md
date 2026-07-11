# 优化新模型费用计算

## Goal

修正并增强 CodexManager 对新模型的费用估算能力，重点覆盖 `gpt-5.6` alias、`gpt-5.6-sol`、`gpt-5.6-terra`、`gpt-5.6-luna`，确保短上下文、长上下文、cached input、cache writes、output 与 `standard/priority` billing mode 能按官方价格口径计算，并保持历史模型、自定义价格规则、请求日志和钱包扣费链路向后兼容。

## Background

- 用户提供的官方价格截图显示，GPT-5.6 family 首次在当前 OpenAI flagship model 价格表中单列 `Cache writes`。
- OpenAI 官方 Prompt Caching 文档确认：GPT-5.6 及后续 family 的 cache writes 按 uncached input 的 `1.25x` 计费；之前的模型没有额外 cache-write fee。
- OpenAI 官方 usage 字段为：
  - Responses API：`usage.input_tokens_details.cached_tokens` 与 `usage.input_tokens_details.cache_write_tokens`
  - Chat Completions API：`usage.prompt_tokens_details.cached_tokens` 与 `usage.prompt_tokens_details.cache_write_tokens`
- OpenAI 官方长上下文规则是原始 total input tokens **大于** `272K` 后，对整次请求使用更高的 input/output 档位；当前实现使用 `>= 272000`，存在边界错误。
- 当前内置价格种子版本为 `2026-05-11`，没有任何 GPT-5.6 specific rule（`crates/service/src/quota/model_pricing.rs:4`、`:47`）；由于 resolver 使用 `starts_with`，GPT-5.6 会静默误匹配宽泛的 `gpt-5` seed，并以旧价格返回 `price_status = ok`，而不是可靠地暴露 missing。
- 当前 usage、request log、SQLite token stats、rollup、RPC/UI 与 wallet re-rating 链路都没有 generic cache-write token 字段（`crates/service/src/gateway/observability/http_bridge/aggregate/output_text.rs:14`、`:195`；`crates/service/src/gateway/observability/request_log.rs:3`、`:665`）。
- 当前数据库规则虽预留 Anthropic 风格的 `cache_write_5m/1h` 字段，但 `ModelPriceMatch`、rule resolver 与费用公式完全忽略 cache writes（`crates/core/src/storage/mod.rs:855`；`crates/service/src/quota/model_pricing.rs:29`、`:615`、`:751`）。
- 本机 `D:\my-works\sub2api` 的 typed usage、typed cost breakdown、分项持久化与 resolver 分层值得借鉴；但其 GPT-5.6 三款模型价格相同、缺少 cache-write 单价，OpenAI usage 路径和公式还存在漏采集、双计风险，不能直接复制。
- 2026-07-10 对 `input` 上游 404 条请求进行只读实账核算：CodexManager 当前存储 `$10.8651305`；按新官方价格但暂不含 cache writes 重算为 `$41.120590`；上游为 `$42.208600`。基础价格修正后仅剩 `$1.088010`（2.58%）差额，符合未采集 cache-write premium 的量级。
- 2026-07-11 对本机请求日志进行只读审计：88 条 GPT-5.6 Standard 请求的 normalized input 超过 `272K`。其中仅 9 条已存费用与当前长上下文公式一致，另 79 条虽属于长上下文请求，但仍保留历史不同价格。当前日志没有 context band、matched rule 或 cost breakdown，不能仅凭 `estimated_cost_usd` 证明实际采用了哪档价格（详见 `research/request-log-long-context-audit-2026-07-11.md`）。

## Requirements

- R1. 以 OpenAI 官方价格页和 Prompt Caching 文档为唯一价格与字段语义来源，覆盖 `gpt-5.6` alias、`gpt-5.6-sol/terra/luna` 的 `standard/priority` 规则。
- R2. 建立单一 normalized usage contract，使 `input_tokens` 始终表示包含 cache read/write 的 total input，cached input 与 cache writes 是其分类子集，消除不同协议的输入 token 语义歧义。
- R3. cache-write usage 必须贯穿 Responses、Chat Completions、流式 SSE、非流式 JSON、Responses WebSocket、request log、SQLite token stats、rollup、RPC、前端类型和 wallet raw usage/re-rating。
- R4. 计价必须按分类互斥公式计算，并对负数、read/write 超过 total input 等异常上游数据做 clamp，禁止漏算或双重计费。
- R5. `ModelPriceRule` 增加 generic `cache_write_price_per_1m` 与 `long_context_cache_write_price_per_1m`；现有 `cache_write_5m/1h` 字段继续保留，不能用 5m 字段冒充 OpenAI generic cache write。
- R6. 长上下文档位必须根据原始 normalized total input 的 `> threshold` 判断，而不是根据 ordinary input 或 `>= threshold` 判断。
- R7. 自定义价格 RPC/UI 必须能够配置 generic cache-write 和完整长上下文价格；新增字段必须 optional/defaulted，兼容旧客户端和旧数据库内容。
- R8. model resolver 必须区分 exact/family/fallback match。新增 minor family（例如 `gpt-5.6`）不得静默继承宽泛 `gpt-5` official seed 并返回 `ok`；custom prefix 语义保持兼容。
- R9. 官方 seed 升级必须替换或禁用旧版本 official seeds，避免相同 pattern/priority 的新旧价格同时 enabled；用户自定义规则不得被覆盖。
- R10. wallet 的 `billing_model_slug` 重估必须复用同一个 usage contract、完整 cache-write tokens 与 effective `service_tier`，不能产生第二套费用口径。
- R11. 删除 `request_log.rs` 中已废弃的第二份硬编码价格表与旧 `270_000` 阈值，实现单一价格源。
- R12. 参考 `sub2api` 采用 typed token/cost breakdown 和可测试的 resolver 边界，但不引入其远程 dynamic pricing、不导入其价格 JSON，也不复用其 OpenAI 双计公式。
- R13. 方案经用户批准后方可进入实施；迁移只以代码文件形式交付，不在本任务中直接对用户数据库执行。
- R14. 请求计价必须生成并持久化可审计的 pricing snapshot，至少包含 billing mode、context band、threshold、matched rule/pattern/source、match quality、price status、四类费用分项、总费用及相对短上下文基线的 uplift；请求日志不得只靠当前规则和 token 在前端反推历史计价结果。
- R15. 请求日志 UI 必须能标识并筛选 `short / long / single_tier / legacy_candidate / unknown`，展示长上下文总费用与 uplift。历史无 snapshot 的记录只能标记为“长上下文候选 / 原计价规则未知”，不能自动重算或覆盖原费用。
- R16. 自动 context compact 必须提供持久化控制开关并默认关闭。关闭时，CodexManager 在 `/v1/models` 中隐藏模型的 `auto_compact_token_limit`，避免 Codex 客户端自动发起 compact；模型目录中保存的原始阈值不得被删除或覆盖，显式 `/v1/responses/compact` 请求仍保持兼容透传。开关读取、环境变量覆盖和运行时切换必须 fail-safe，不能影响普通 `/v1/responses` 请求。

## Official Price Matrix

单位：USD / 1M tokens。

### Standard

| Model | Short input | Short cached | Short write | Short output | Long input | Long cached | Long write | Long output |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `gpt-5.6` / `gpt-5.6-sol` | 5.00 | 0.50 | 6.25 | 30.00 | 10.00 | 1.00 | 12.50 | 45.00 |
| `gpt-5.6-terra` | 2.50 | 0.25 | 3.125 | 15.00 | 5.00 | 0.50 | 6.25 | 22.50 |
| `gpt-5.6-luna` | 1.00 | 0.10 | 1.25 | 6.00 | 2.00 | 0.20 | 2.50 | 9.00 |

### Priority

OpenAI Priority 视图只公布单档价格，没有单独的 long-context 列；实现不得自行推算未公布的 Priority long-context 价格。

| Model | Input | Cached | Cache write | Output |
| --- | ---: | ---: | ---: | ---: |
| `gpt-5.6` / `gpt-5.6-sol` | 10.00 | 1.00 | 12.50 | 60.00 |
| `gpt-5.6-terra` | 5.00 | 0.50 | 6.25 | 30.00 |
| `gpt-5.6-luna` | 2.00 | 0.20 | 2.50 | 12.00 |

## Acceptance Criteria

- [ ] 根因清单中的每项问题都有严重度、代码位置、触发条件和影响范围。
- [ ] CodexManager 与 `sub2api` 的差异矩阵明确区分“可借鉴结构”与“不可复制实现/数据”。
- [ ] `gpt-5.6` alias 与 `gpt-5.6-sol/terra/luna` 的 Standard/Priority 价格、长上下文边界和 cache-write 单价有回归测试。
- [ ] `gpt-5.6-*` 不会命中宽泛 `gpt-5` official seed；未知未来 minor family 的 fallback 必须为 `partial/missing` 并携带 matched pattern，而不是静默 `ok`。
- [ ] 费用公式满足 `plain + read + write = total_input`，异常数据被 clamp 且不会产生负成本或双计。
- [ ] Responses、Chat Completions、SSE、non-stream JSON 与 Responses WebSocket 均能采集 `cache_write_tokens`。
- [ ] Anthropic/Gemini adapter 明确 total-input 与 cache read/write 的归一化边界，不把不同 provider 的字段名或 TTL 语义直接混用。
- [ ] `request_token_stats`、daily/hourly rollup、request log RPC、wallet raw usage 与 billing-model re-rating 保留 cache-write tokens。
- [ ] 自定义价格 RPC/UI 可配置 generic cache-write 与完整 long-context 字段，旧 payload 仍能反序列化并按兼容规则工作。
- [ ] 新 official seed 版本启用后，旧 official seed 不再参与匹配；custom rules 保持 enabled 且优先级不变。
- [ ] 旧历史日志不被虚构 backfill，也不被自动重算；新增字段对旧行按 `NULL/0` 兼容。
- [ ] 使用 2026-07-10 `input` 上游聚合数据作为人工验收基线：不含 cache writes 的重算值为 `$41.120590`；完整 usage 可用后，与上游 `$42.208600` 的差异应只剩约定 rounding tolerance。
- [ ] 新产生的请求日志可明确回答是否实际使用长上下文档、命中的价格规则、各费用分项及相对短上下文的额外费用。
- [ ] 日志列表支持按 context band 筛选；筛选摘要返回长上下文请求数、长上下文总费用和 uplift 总额。
- [ ] 设置页提供“自动上下文压缩”开关，默认关闭；关闭后 `/v1/models` 不发布 `auto_compact_token_limit`，重新开启后恢复模型目录原阈值。
- [ ] 自动 compact 开关只控制客户端自动触发，不拦截显式 compact endpoint，也不修改普通请求；配置缺失或读取失败时按关闭处理。
- [ ] 历史无 pricing snapshot 的 `>272K` GPT-5.6 Standard 日志显示为 `legacy_candidate`，保留原 `estimated_cost_usd`，不伪造 applied rule、不自动重算钱包。
- [ ] `prd.md`、`design.md`、`implement.md` 完成并经用户审阅后，才允许执行 `task.py start`。

## Out of Scope

- 不在本任务中直接对用户数据库执行数据库迁移。
- 不从 `sub2api` 同步 dynamic pricing，不依赖第三方价格源自动覆盖官方 seed。
- 不为官方未公布的 Priority long-context 价格做倍数推导。
- 不回填无法从历史响应恢复的 cache-write token 分类。
- 不在本任务中新增 Batch/Flex 计价或 regional processing `10%` uplift；后者需要独立的请求区域信号与产品决策。

## Planning State

- Trellis Phase：`in_progress`
- 用户已批准实施；官方价格、alias、usage 字段、cache-write 语义与 `>272K` 边界均已核对。
- 实施期间不直接改写用户历史费用或钱包余额。
