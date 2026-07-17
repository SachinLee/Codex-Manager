# Grok 4.5 请求计费设计

## 1. Design Summary

采用双来源计费模型：xAI `usage.cost_in_usd_ticks` 是实际扣费的权威来源；现有 `ModelPriceRule` 引擎加入 Grok 4.5 official seeds，作为聚合供应商剥离 xAI 扩展字段、异常响应或离线重估时的兜底。两种来源同时生成审计数据，但最终费用只选择一个基础来源，再统一应用现有 aggregate multiplier 和 wallet 规则。

本设计扩展现有计费链路，不创建第二套 Grok 专用日志或钱包系统。

## 2. Current Failure Path

```text
model = grok-4.5 discovered from Aggregate API
  -> built-in seeds: no match
  -> aggregate model sync creates enabled exact rule at $0 / $0 / $0
  -> resolver matches aggregate_api_sync placeholder
  -> CostEstimate { price_status: "ok", cost_usd: Some(0.0) }
  -> request log / rollup / wallet observe $0

model = grok-4.5 without the aggregate placeholder
  -> enabled price rules: no match
  -> built-in seeds: no match
  -> CostEstimate::missing()
  -> request_log cost_usd.unwrap_or(0.0)
  -> request log / rollup / wallet observe $0
```

同时，generic parser 丢弃 `cost_in_usd_ticks`，因此即使直连 xAI 已返回实际费用，现有日志也无法使用。

## 3. Data Flow

```text
xAI upstream response / final SSE usage
  -> shared usage parser
     - token usage
     - reasoning usage
     - provider cost ticks
     - effective service tier
  -> local Grok estimate (always attempted for audit)
  -> select base cost
     - valid provider ticks: provider_reported
     - otherwise: local_estimate
  -> apply aggregate cost multiplier once
  -> request log + pricing snapshot + token stats
  -> wallet charge / rollups / RPC
  -> request log UI
```

## 4. Usage Contract

### 4.1 Internal fields

Extend `UpstreamResponseUsage` and `RequestLogUsage` with:

```text
provider_cost_usd_ticks: Option<i64>
provider_cost_nano_usd: Option<i64>   // compatibility fallback only
```

Keep existing token fields unchanged for cross-provider analytics:

```text
input_tokens
cached_input_tokens
cache_write_input_tokens
output_tokens
total_tokens
reasoning_output_tokens
```

### 4.2 Parsing and merge semantics

- Parse cost from the same `usage` objects already handled by `parse_usage_from_json`.
- Inspect both top-level `usage` and nested `response.usage` through the existing merge path.
- Reject negative values and values that cannot fit the persisted integer type.
- `merge_usage` uses last non-null replacement semantics for provider cost, matching terminal usage snapshots.
- Streaming readers must not sum running `cost_in_usd_ticks`; the final usage event replaces previous values.
- `usage_has_signal` includes provider cost so cost-only terminal usage is not discarded.
- Protocol conversion may forward the upstream field when compatible, but logging must capture it before conversion so forwarding behavior cannot affect accounting.

## 5. Pricing Rule Design

### 5.1 Provider and seeds

Add `XAI_PRICE_SOURCE = "https://docs.x.ai/developers/pricing"` and Standard/Priority seeds for:

- `grok-4.5`
- `grok-4.5-latest`
- `grok-build-latest`

All seeds use provider `xai`. Explicit alias seeds avoid relying on `grok-4.5` prefix matching for the unrelated `grok-build-latest` slug. Official seed replacement continues to use the existing versioned transaction, so an updated seed version disables older official seeds without touching custom rules.

### 5.2 Inclusive threshold

Add a persisted field:

```text
long_context_threshold_inclusive BOOLEAN NOT NULL DEFAULT 0
```

Selection becomes:

```text
inclusive = true  -> input_tokens >= threshold
inclusive = false -> input_tokens > threshold
```

Existing rules default to `false`; Grok seeds set `true` with threshold `200_000`. The snapshot keeps the threshold and also exposes the comparator/inclusive flag so historical decisions remain auditable.

### 5.3 Cached input

Reuse the normalized input partition:

```text
total_input = max(input_tokens, 0)
cached      = clamp(cached_input_tokens, 0, total_input)
plain       = total_input - cached - clamped_cache_write
```

Grok has no separate cache-write price. If unexpected cache-write usage is present, use ordinary input price as a compatible fallback and set `price_status = partial`.

### 5.4 xAI reasoning normalization

Do not globally change `output_tokens`, because OpenAI and other providers may report reasoning as a subset of output. Extend the estimator to accept `total_tokens` and `reasoning_output_tokens` and normalize only when the matched provider is `xai`:

```text
if total_tokens >= input_tokens:
    billable_output = max(output_tokens, total_tokens - input_tokens)
else:
    billable_output = output_tokens + max(reasoning_output_tokens, 0)
```

This preserves raw log fields while ensuring the local xAI fallback charges completion plus reasoning exactly once. The resulting combined amount is priced with the effective output rate for the chosen short/long and Standard/Priority band.

## 6. Actual Cost Selection

### 6.1 Conversion

```text
provider_cost_usd = provider_cost_usd_ticks / 10_000_000_000
```

Use ticks first. If ticks are absent and a valid compatibility nano-USD value exists:

```text
provider_cost_usd = provider_cost_nano_usd / 1_000_000_000
```

Raw integer values are persisted. Conversion happens once; no intermediate decimal rounding is applied.

### 6.2 Cost source

Extend the in-memory estimate/result with:

```text
cost_source                 // provider_reported | local_estimate
provider_cost_usd_ticks
provider_cost_usd
local_estimated_cost_usd
pricing_variance_usd        // provider raw cost - local raw estimate
```

Selection:

```text
base_cost = valid provider cost ?? local estimate
effective_cost = base_cost * validated cost_multiplier
```

The variance compares pre-multiplier values. The multiplier is never applied to the stored raw provider cost or local audit estimate.

### 6.3 Tool requests

Provider-reported cost remains authoritative because it includes server-side tool invocations and internal agentic decodes. When exact cost is absent:

- If no tool signal exists, return the token estimate with the normal price status.
- If a server-side tool signal exists but typed invocation costs cannot be reconstructed, retain token estimate but force `price_status = partial`.
- Do not guess tool type or invocation count from response text.

## 7. Storage Design

Use the next available additive migration number at implementation time; do not assume `115` remains free while other active tasks are editing the repository.

### 7.1 `model_price_rules`

Add:

```text
long_context_threshold_inclusive INTEGER NOT NULL DEFAULT 0
```

Update table creation fallback, row mapper, insert/upsert SQL, official seed construction, RPC price-rule payload and custom rule UI. Existing rows preserve strict `>` behavior.

### 7.2 `request_pricing_snapshots`

Add nullable/additive audit columns:

```text
long_context_threshold_inclusive INTEGER
cost_source TEXT
provider_cost_usd_ticks INTEGER
provider_cost_usd REAL
local_estimated_cost_usd REAL
pricing_variance_usd REAL
```

`total_cost_usd` stores the final effective cost after multiplier for compatibility with existing log presentation. `provider_cost_usd` and `local_estimated_cost_usd` remain pre-multiplier values.

The legacy `request_logs.estimated_cost_usd` and `request_token_stats.estimated_cost_usd` continue to store the final effective cost so current summaries do not need a breaking rewrite.

### 7.3 Historical compatibility

- Old snapshots read new fields as `NULL`.
- No migration recalculates historical Grok prices.
- No wallet balance is modified by storage initialization.
- Table ensure helpers mirror migration columns for legacy databases.

## 8. Wallet and Retry Semantics

- No explicit `billing_model_slug`: use the final selected request cost, preferring provider actual.
- Explicit `billing_model_slug`: preserve current local re-rating behavior because the product intentionally charges against another model profile.
- Apply model-group multiplier and aggregate API multiplier exactly once along the existing charge path.
- Reasoning Guard/internal retry attempts keep using the existing event accounting. When an intermediate Grok response provides actual cost, its selected cost must flow into the retry event rather than silently reverting to a zero/missing estimate.
- A successful final request and billable internal retries remain separate accounting entries, consistent with current Guard observability rules.

## 9. RPC and UI

Extend request-log read models and TypeScript types with camelCase fields:

```text
pricingCostSource
providerCostUsd
localEstimatedCostUsd
pricingVarianceUsd
longContextThresholdInclusive
```

Raw ticks remain persisted for precision; if exposed through RPC, serialize them as a decimal string to avoid JavaScript integer precision loss.

Request log UI behavior:

- Badge: `官方实际费用` for `provider_reported`.
- Badge: `本地估算` for `local_estimate`.
- Continue showing `short / long / single_tier`, billing mode, matched rule and `ok / partial / missing`.
- Show local-vs-provider variance in diagnostic detail, not as a second charged amount.
- Historical rows with no cost source retain the existing estimated display and are not relabeled as provider actual.

The existing model catalog modal continues to own custom price editing; add the inclusive-threshold control next to the threshold field instead of creating a Grok-specific settings page.

## 10. Error and Logging Policy

- Invalid provider cost is not a request failure; log a structured warning with trace/model/source and fall back to local pricing.
- Do not log response bodies, API keys, prompts or other secrets.
- A missing Grok official seed or missing cost source should be visible through `price_status`/metrics, not silently converted to a successful zero price.
- Storage insertion failure follows the existing request-log error handling and must not crash response delivery.

## 11. Compatibility and Rollout

1. Add migrations and types first; old rows remain readable.
2. Add parsing and local seeds behind existing code paths; no runtime feature flag is required because fallback behavior is additive.
3. Switch final cost selection only after parser and snapshot tests pass.
4. Add RPC/UI fields as optional to tolerate mixed desktop/service versions.
5. Compare provider actual versus local estimate in tests and, when available, local request samples before treating the seed as production-ready.

Rollback is code-level: revert final cost selection to local estimate while leaving nullable audit columns in place. Additive SQLite columns are not destructively removed.

## 12. Key Risks

| Risk | Likelihood | Mitigation |
| --- | --- | --- |
| Running stream ticks are summed | Medium | Last-value replacement and final-chunk regression tests |
| Reasoning double counted | Medium | xAI-only normalization using `total - input`, provider regression tests |
| 200K boundary off by one | High without schema change | Persist inclusive comparator and test 199999/200000 |
| Tool requests undercharged by fallback | Medium | Prefer actual cost; mark tool-bearing fallback `partial` |
| Multiplier applied twice | Medium | Store raw and effective costs separately; wallet/request-log integration tests |
| JS loses tick precision | Medium | Persist i64; expose string or USD only |
| Alias later targets a new model | Medium | Versioned seeds plus actual/local variance diagnostics |
| Active tasks claim the next migration number | High | Allocate migration number immediately before implementation |
