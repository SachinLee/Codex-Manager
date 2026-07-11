# 优化新模型费用计算设计

## 1. Design Summary

本任务不把问题视为“补三行模型价格”，而是修复一个跨层计费契约缺口：GPT-5.6 新增了独立 cache-write 价格，但 CodexManager 目前只建模 input、cached input、output，导致 cache-write token 在采集、归一化、持久化、计价、钱包重估和 UI 配置中全部丢失。

推荐采用 `sub2api` 的 typed usage / typed cost breakdown 思路，但保留 CodexManager 现有 `ModelPriceRule` resolver，使用官方 seed + 用户 override，不引入第二套价格引擎或远程 dynamic pricing。

## 2. Layer Boundaries

- `crates/service/src/gateway/observability/`
  - 负责把不同响应协议的 raw usage 归一化为单一 usage contract。
  - Responses、Chat Completions、SSE、non-stream JSON、Responses WebSocket 必须共享解析规则，避免字段漂移。
- `crates/service/src/quota/model_pricing.rs`
  - 负责 price rule 解析、长上下文档位选择、token 分类 clamp 和 cost breakdown。
  - 这里是唯一费用公式，不允许 request log 或 wallet 再维护第二份公式。
- `crates/core/src/storage/`
  - 负责 price rule 新字段、request token stats、rollup 与 reasoning-guard retry usage 的持久化。
- `crates/core/src/rpc/` 与 `crates/service/src/quota/read.rs`
  - 负责向前端暴露 optional 的 cache-write/long-context price fields 和 cache-write usage。
- `apps/src/`
  - 负责自定义价格编辑、类型兼容和 request usage 的可观测展示。
- `crates/service/src/auth/app_manager.rs`
  - 负责 wallet re-rating；只能调用统一 estimator，必须传入完整 usage 与 effective service tier。

## 3. Normalized Usage Contract

新增或提取一个 focused typed contract，建议命名 `NormalizedUsage`，核心字段为：

```text
input_tokens                 // normalized total input，包含 read/write 子集
cached_input_tokens          // cache read，input_tokens 的子集
cache_write_input_tokens     // generic cache write，input_tokens 的子集
output_tokens
total_tokens
reasoning_output_tokens
```

不直接采用 `sub2api` 的 `cache_creation_input_tokens` 作为跨 provider 通用名称，因为 OpenAI 官方字段和语义是 `cache_write_tokens`，Anthropic 还存在 5m/1h TTL 分类。

### 3.1 Provider normalization rules

| Source | Raw input semantics | Read source | Write source | Normalized total input |
| --- | --- | --- | --- | --- |
| OpenAI Responses | `input_tokens` 已包含 read/write | `input_tokens_details.cached_tokens` | `input_tokens_details.cache_write_tokens` | raw `input_tokens` |
| OpenAI Chat Completions | `prompt_tokens` 已包含 read/write | `prompt_tokens_details.cached_tokens` | `prompt_tokens_details.cache_write_tokens` | raw `prompt_tokens` |
| Anthropic native | `input_tokens` 是 ordinary input；cache read/creation 独立 | `cache_read_input_tokens` | `cache_creation_input_tokens` | ordinary + read + creation |
| Gemini native | prompt total 与 cached content 的关系由 adapter 明确处理 | provider cached field | 没有明确 write 字段时为 0 | provider total；禁止按名称猜测 write |

Anthropic 的 `cache_write_5m/1h` price fields 保留。第一阶段新增 generic aggregate 字段和 OpenAI 计价能力，不拿 `cache_write_5m_price_per_1m` 代替 OpenAI generic write price；TTL-aware Anthropic 分项可在同一 typed contract 上后续扩展。

### 3.2 Merge and streaming semantics

- terminal usage snapshot 继续覆盖之前 snapshot，而不是把同一请求多个 terminal event 相加。
- `merge_usage`、`usage_has_signal`、SSE collectors 和 WebSocket terminal parser 都加入 cache-write 字段。
- protocol conversion 必须保留 token details；从 Responses 转 Chat Completions 时写入 `prompt_tokens_details.cache_write_tokens`，反向转换写入 `input_tokens_details.cache_write_tokens`。
- adapter 不得同时保留 raw creation token 又把它重复叠加到 total input。

## 4. Pricing Model

### 4.1 Schema additions

`ModelPriceRule` 和 `PriceSeed` 新增：

```text
cache_write_price_per_1m
long_context_cache_write_price_per_1m
```

保留：

```text
cache_write_5m_price_per_1m
cache_write_1h_price_per_1m
```

字段语义：

- generic cache-write price 是 OpenAI GPT-5.6 family 使用的直接单价。
- long-context generic write price 仅在 `input_tokens > threshold` 且规则明确配置时使用。
- 对旧规则的 `NULL` generic write price，兼容回退到 ordinary input price。
- 对已知需要独立 write price、但 matched rule 缺失该字段且实际 write tokens > 0 的请求，仍可用 input-price fallback 生成估算，但 `price_status` 标记为 `partial` 并记录 warning，不能静默声称完整准确。

### 4.2 Token partition and formula

所有 token 先转为非负整数，按剩余空间 clamp：

```text
total_input = max(input_tokens, 0)
read        = min(max(cached_input_tokens, 0), total_input)
write       = min(max(cache_write_input_tokens, 0), total_input - read)
plain       = total_input - read - write
output      = max(output_tokens, 0)
```

费用分项：

```text
plain_cost  = plain  * input_price  / 1_000_000
read_cost   = read   * cached_price / 1_000_000
write_cost  = write  * write_price  / 1_000_000
output_cost = output * output_price / 1_000_000
total_cost  = plain_cost + read_cost + write_cost + output_cost
```

长上下文档位使用归一化前后都不改变的 `total_input` 判断：

```text
use_long_context = total_input > long_context_threshold_tokens
```

禁止使用 `plain`、`total_input - cached` 或 `>= threshold` 选择档位。

### 4.3 Typed cost breakdown

借鉴 `sub2api`，内部 `CostEstimate` 建议扩展为：

```text
provider
price_status          // ok | partial | missing
plain_input_cost_usd
cached_input_cost_usd
cache_write_cost_usd
output_cost_usd
total_cost_usd
matched_pattern
price_source
match_quality         // exact | family | fallback
```

第一阶段持久化仍以 `estimated_cost_usd` 总额为兼容主字段，同时持久化 cache-write token。分项 cost 可先用于测试、日志和诊断，不强制新增全部数据库 cost columns，避免无必要的 schema 膨胀。

### 4.4 Model matching safety

实账验证发现，当前 `starts_with` 会让 `gpt-5.6-sol` 静默匹配 `gpt-5` official seed。需要把 custom prefix 语义与 official model-family 语义分开：

- custom rule 的 `match_type = prefix` 保持现有行为，避免破坏用户显式配置。
- official seed 使用 model-aware family boundary：
  - `gpt-5` 可以匹配模型本身及其日期/version suffix，例如 `gpt-5-YYYY-MM-DD`。
  - `gpt-5` 不得匹配新的 dotted minor family `gpt-5.6`。
  - `gpt-5.6` 可以作为 alias/family pattern 匹配 `gpt-5.6-sol/terra/luna`，specific variant 仍由最长 pattern 优先。
- resolver 返回 `matched_pattern`、`price_source` 和 `match_quality`。
- 只有 exact/specific family 规则可以返回 `price_status = ok`；宽泛或兼容 fallback 返回 `partial`，完全无价格返回 `missing`。
- 日志和指标必须能聚合 `partial/missing/fallback`，未来新增模型时不能等到账单异常才发现。

## 5. Official Seed Design

### 5.1 Standard seeds

新增四个 pattern：

- `gpt-5.6-sol`
- `gpt-5.6-terra`
- `gpt-5.6-luna`
- `gpt-5.6` alias，价格与 sol 相同

具体价格使用 `prd.md` 的官方矩阵。`gpt-5.6` prefix 仍可匹配 alias/versioned slug，specific variant 通过最长 pattern 优先匹配。

### 5.2 Priority seeds

新增同样四个 pattern，使用官方 Priority 单档价格。官方 Priority 页面没有 long-context 列，因此 Priority seed 的 long-context fields 保持 `None`，不得用 Standard 倍数自行生成。

### 5.3 Seed lifecycle

当前 `official-{version}-{pattern}` 会让旧版本长期 enabled。改为：

1. 在同一存储事务中 upsert 当前完整 official seed set。
2. 验证当前版本 seed 数量完整。
3. 禁用 `source = official_seed AND seed_version != current` 的旧记录。
4. 不修改 `source = custom` 的用户规则。
5. 提交后 invalidation enabled-rule cache。

resolver 同时增加 deterministic tie-breaker（例如 `updated_at DESC, id ASC`），但正常情况下旧 official seed 已不再参与匹配。

### 5.4 Empirical baseline

2026-07-10 `input` 上游窗口提供上线前人工基线：

- legacy stored cost：`$10.8651305`
- v2 base cost without cache writes：`$41.120590`
- upstream cost：`$42.208600`
- remaining delta：`$1.088010` / `2.577697%`

该基线证明 model matching/official price 是主要误差，cache-write 是剩余精度问题。上线验证必须分别观察两类修复，不能只比较最终 total。

## 6. Persistence and Compatibility

建议新增下一号 SQLite migration，至少添加：

- `model_price_rules.cache_write_price_per_1m`
- `model_price_rules.long_context_cache_write_price_per_1m`
- `request_token_stats.cache_write_input_tokens`
- `request_token_daily_rollups.cache_write_input_tokens`
- `request_token_stat_hourly_rollups.cache_write_input_tokens`
- reasoning-guard event usage 对应的 cache-write token 字段（其 retry cost 也必须使用统一 estimator）

兼容规则：

- 新 token columns 对旧行使用 `NULL/0`。
- 不回填或重算旧 `estimated_cost_usd`，因为历史响应已无法可靠恢复 write 分类。
- 旧 RPC payload 缺少新增字段时正常反序列化。
- 旧 custom rule 的 generic write price 为 `NULL` 时按 input price fallback；UI 明确提示该语义。
- `request_logs` 旧 token columns 继续保持兼容，不再复制一份新 token 数据；权威 token usage 仍放在 `request_token_stats`。

## 7. RPC and Frontend

`ModelPriceRuleEntry` / `ModelPriceRuleUpsertInput` 与 TypeScript 对应类型新增 optional：

- `cacheWritePricePer1m`
- `longContextThresholdTokens`
- `longContextInputPricePer1m`
- `longContextCachedInputPricePer1m`
- `longContextCacheWritePricePer1m`
- `longContextOutputPricePer1m`

UI 设计：

- 基础区从三列扩展为 input / cached / cache write / output。
- 增加折叠的“长上下文价格”区，只有设置 threshold 时启用长上下文四类价格。
- 数值允许 `0`，不允许负数、NaN、Infinity；threshold 必须为正整数或空。
- Request log/detail 与 usage summary 增加 optional cache-write token 行；旧后端缺字段时按 0 展示。

## 8. Wallet and Billing Model Re-rating

`raw_usage_json` 新增 `cacheWriteInputTokens`，保留现有 camelCase 兼容结构。

`estimate_billing_model_cost_usd` 必须：

1. 读取 camelCase、snake_case 和官方 token-details 嵌套路径中的 cache write。
2. 将当前请求的 effective `service_tier` 传入统一 estimator。
3. 使用相同 clamp、长上下文与 price-status 规则。
4. 不在 wallet 层复制 token 公式。

这样平台 model 与 `billing_model_slug` 重估只会改变 matched price，不会改变 usage 分类。

Wallet status policy：

- `ok`：使用 v2 total 正常扣费。
- `partial`：使用 estimator 明确返回的兼容 fallback cost，但在 ledger raw usage 中保存 status、matched pattern 与 warning；不得展示为精确价格。
- `missing`：保持当前不自动构造未知价格的行为，不产生推测性扣费，同时记录高优先级告警。
- GPT-5.6 official seeds 完整时正常请求必须为 `ok`；上述降级只服务旧 custom rule、异常 usage 或未来未知模型。

## 9. Removal of Duplicate Pricing

删除 `crates/service/src/gateway/observability/request_log.rs` 中：

- `MODEL_PRICE_PER_1K_TOKENS`
- `resolve_model_price_per_1k`
- dead-code `estimate_cost_usd*`

所有 request log、reasoning guard、aggregate API 与 wallet 计价都收敛到 `quota::model_pricing`。

## 10. Rollout and Rollback

Rollout 顺序：

1. 先落 additive schema、typed usage contract 和 parser，暂不改变 wallet charge。
2. 启用 v2 shadow calculation，同时记录 legacy/v2 total、matched pattern、match quality 和 delta。
3. 使用 focused fixtures 和 `input` 上游样本确认 model/context/cache-write 分项。
4. 切换 request log、dashboard 与 rollup 到 v2。
5. wallet 作为最后一个切换点；只有 `ok` 结果进入正常扣费，`partial/missing` 记录告警并按明确的兼容策略处理。
6. 最后开放 RPC/UI 配置并清理 legacy estimator。

在 official seed 生效前，必须保证 estimator 和 usage parser 已支持 cache writes，否则会出现“价格存在但 token 永远为 0”的假修复。

Rollback：

- SQLite add-column migration 不回滚删除列；旧代码会忽略新增列。
- 如费用结果异常，可临时禁用新版本 official seed 并恢复旧 seed，但不能删除 custom rules。
- 不对历史费用做自动修复，避免不可逆钱包差额调整；如需补账必须另开审计任务。

## 11. Risks

- 不同 provider 的 raw `input_tokens` 语义不同，若 normalization 不在协议边界完成，公式仍会漏算或双计。
- streaming terminal event 可能重复出现，错误地累加 usage 会放大费用。
- custom rule 缺少 generic write 单价时只能得到 fallback/partial 估算，需要 UI 提示和日志告警。
- official seed 替换若不在事务中执行，启动中断可能留下不完整价格集。
- wallet 扣费是资金边界；partial/missing 状态必须可观测，不能默默按零费用通过。
- 若 v2 shadow 与 upstream 的差异持续超过 `3%` 或绝对值超过 `$0.10`（取更严格者），暂停 wallet 切换并检查 usage 分类、tier、长上下文和 multiplier。
- regional processing `10%` uplift、Batch/Flex 与未来 cache-write family 需要额外请求信号，不应塞进本次基础模型 seed。

## 12. Request-log Pricing Audit Snapshot

仅在 UI 使用 `input_tokens > threshold` 动态打标不够可靠：规则可能被修改，历史日志可能使用旧 seed，Priority 也没有独立 long-context 价格。推荐新增一张一对一、additive 的 `request_pricing_snapshots` 表，而不是继续扩张 `request_logs` 或把审计字段塞进 `request_token_stats`。

建议字段：

```text
request_log_id                  // PK/FK
billing_mode                   // standard | priority | ...
context_band                   // short | long | single_tier | unknown
long_context_threshold_tokens
matched_rule_id
matched_pattern
price_source
match_quality                  // exact | family | fallback
price_status                   // ok | partial | missing
plain_input_cost_usd
cached_input_cost_usd
cache_write_cost_usd
output_cost_usd
total_cost_usd
short_baseline_cost_usd
long_context_uplift_usd
created_at
```

`CostEstimate` 扩展为 typed breakdown，并由同一次 estimator 调用同时驱动 `request_token_stats.estimated_cost_usd`、wallet 与 pricing snapshot，禁止调用方各自重算。

### 12.1 Context-band semantics

- `long`：matched rule 明确配置 long threshold/price，billing mode 支持独立长上下文档，且 normalized total input `> threshold`。
- `short`：matched rule 支持长上下文档，但本次未越过阈值；或规则本身只有普通 short 档。
- `single_tier`：当前 billing mode 只有单档官方价格，例如 GPT-5.6 Priority。即便 input 超过 `272K`，也不能展示为“使用 long price”。
- `unknown`：缺少 model、usage 或可靠匹配规则。
- `legacy_candidate` 不写入 snapshot；它是读取历史无 snapshot 日志时的兼容展示态。只有当前规则明确支持 long、effective tier 为 Standard、input 越阈值时才可推断，并必须显示“原计价规则未知”。

### 12.2 Uplift semantics

`long_context_uplift_usd = applied_total_cost - short_baseline_cost`。短基线使用同一 matched rule 的 short 四分类价格和同一 token partition 计算，结果 clamp 到 `>= 0`。该字段回答“因为进入长上下文档额外增加了多少”，而 `total_cost_usd` 回答“这次长上下文请求总共多少钱”。

Priority/single-tier、short、partial/missing 时 uplift 为 `NULL`，避免制造不存在或不可靠的比较。

### 12.3 Query and UI

- `RequestLogListParams` 增加独立 `pricing_band_filter`，不要复用 HTTP `status_filter`。
- list/summary SQL left join snapshot；新日志直接使用 snapshot，旧日志按上述规则生成只读 `legacy_candidate/unknown` 展示态。
- `RequestLogSummary` 增加 context band、threshold、matched rule、price status 与 cost breakdown。
- filter summary 增加 `long_context_count`、`long_context_cost_usd`、`long_context_uplift_usd`、`legacy_candidate_count`。
- 日志费用单元格显示 `长上下文` badge；详情展示 total、uplift、plain/cached/write/output 分项和 matched rule。历史候选使用警告色 badge 和“不代表历史已按长上下文计价”的 tooltip。

### 12.4 Compatibility and migration

- 由于 migration `113` 已可能在开发数据库执行，新增 snapshot 使用下一号 migration，不回改已应用的 `113`。
- 不 backfill pricing snapshot，不覆盖历史 `estimated_cost_usd`，不调整 wallet ledger。
- snapshot 写入失败不得导致主请求失败，但必须记录结构化告警；费用与 wallet 的资金边界仍以 estimator 结果为准。

## 13. Automatic Compact Safety Switch

CodexManager 当前已存储模型的 `auto_compact_token_limit`，Codex 客户端据此决定何时调用 `/v1/responses/compact`。本阶段不在网关中拦截普通 `/v1/responses` 并自行替换历史，因为该行为需要完整复现 Codex 的 replacement-history 协议，失败会直接扩大正常请求风险。

新增 `autoCompactEnabled` 设置，运行时环境变量为 `CODEXMANAGER_AUTO_COMPACT_ENABLED`，默认 `false`：

- 关闭：仅在本地 `/v1/models` 输出投影中将 `auto_compact_token_limit` 置空。
- 开启：原样发布模型目录保存的阈值。
- 持久化模型目录内容不变，切换开关不产生数据丢失。
- 显式 `/v1/responses/compact` 继续透传，便于手动 compact 和兼容已有客户端。
- 普通 `/v1/responses` 链路不读取 compact 结果、不等待额外上游调用，开关或 compact 失败不会阻断正常流程。

该设计属于 fail-safe 控制：缺少设置时按关闭处理。未来如果实现真正的网关主动 compact，需要独立任务验证 compact response 到 replacement history 的完整协议，并实现超时、结果校验、原请求 fail-open 回退和独立可观测性。
