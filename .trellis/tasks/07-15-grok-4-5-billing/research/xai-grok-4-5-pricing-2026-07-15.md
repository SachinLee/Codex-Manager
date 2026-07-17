# xAI Grok 4.5 Pricing Research

**Verified:** 2026-07-15  
**Confidence:** High  
**Scope:** Grok 4.5 text inference, prompt caching, priority processing, server-side tools, exact provider cost reporting

## Sources

1. [xAI Grok 4.5 model page](https://docs.x.ai/developers/models/grok-4.5)
2. [xAI pricing](https://docs.x.ai/developers/pricing)
3. [xAI cost tracking](https://docs.x.ai/developers/cost-tracking)
4. [xAI prompt caching usage and pricing](https://docs.x.ai/developers/advanced-api-usage/prompt-caching/usage-and-pricing)
5. [xAI Batch API](https://docs.x.ai/developers/advanced-api-usage/batch-api)

The model page was modified 2026-07-09 and the pricing page was modified 2026-07-03 according to their official metadata at research time.

## Official Price Matrix

All token prices are USD per 1M tokens.

| Billing mode | Prompt band | Input | Cached input | Output / reasoning |
| --- | --- | ---: | ---: | ---: |
| Standard | `< 200K` | 2.00 | 0.50 | 6.00 |
| Standard | `>= 200K` | 4.00 | 1.00 | 12.00 |
| Priority | `< 200K` | 4.00 | 1.00 | 12.00 |
| Priority | `>= 200K` | 8.00 | 2.00 | 24.00 |

Verified rules:

- Grok 4.5 context window is `500K`.
- The pricing table defines the bands as `< 200k prompt tokens` and `>= 200k prompt tokens`.
- When the threshold is reached, the higher price applies to all tokens in the request.
- Priority Processing is `2x` Standard for input, output, cached and reasoning tokens.
- Priority billing applies only when the response confirms `service_tier = "priority"`.
- The model aliases currently include `grok-4.5-latest` and `grok-build-latest`.
- Grok 4.5 currently has no Batch discount and Batch API documentation says the model is not supported for batch requests.

## Prompt Caching Contract

- Chat Completions: `usage.prompt_tokens_details.cached_tokens`.
- Responses API: `usage.input_tokens_details.cached_tokens`.
- Cached tokens are a subset of total prompt/input tokens, not additional tokens.
- xAI performs prompt caching automatically; conversation affinity can improve cache hit rate but is not required for accounting.

Local fallback formula:

```text
plain_input = max(input_tokens - cached_tokens, 0)
input_cost = plain_input * input_rate / 1_000_000
cached_cost = cached_tokens * cached_rate / 1_000_000
```

## Reasoning Contract

xAI pricing lists reasoning tokens as a standard billed token type. Official Chat Completions and Responses examples can expose reasoning separately from completion/output while `total_tokens` accounts for both.

For xAI-only local fallback:

```text
if total_tokens is valid and total_tokens >= input_tokens:
    billable_output = max(output_tokens, total_tokens - input_tokens)
else:
    billable_output = output_tokens + reasoning_tokens

output_cost = billable_output * output_rate / 1_000_000
```

This normalization must be provider-specific. OpenAI output semantics must not be changed because reasoning can already be a subset of `output_tokens` there.

## Provider-Reported Actual Cost

xAI states every inference response includes the exact per-request charge in:

```text
usage.cost_in_usd_ticks
```

Conversion:

```text
1 USD = 10,000,000,000 ticks
cost_usd = cost_in_usd_ticks / 10,000,000,000
```

Properties:

- The value is per request, not cumulative across conversation turns.
- It already includes prompt caching discounts.
- It includes all token costs and server-side tool invocation costs.
- For streaming REST/OpenAI SDK calls, usage/cost appears in the final usage chunk when `stream_options.include_usage = true`.
- xAI SDK running snapshots must not be summed; the final response is authoritative.

Implementation consequence: provider actual should be the final-cost source when present, while local pricing remains an audit/fallback calculation.

## Tool Pricing

| Tool | Official cost per 1K calls |
| --- | ---: |
| Web Search | $5.00 |
| X Search | $5.00 |
| Code Execution / Code Interpreter | $5.00 |
| File Attachments | $10.00 |
| Collections / File Search | $2.50 |

Provider actual cost already includes these charges. If provider actual is stripped by an intermediary, token-only local estimates cannot claim complete accuracy unless typed invocation counts are also available.

## Other Billable Cases

- Some Responses API guideline violations caught before generation can incur a fixed `$0.05` fee.
- File storage/download and collections storage are account-level resources and are not part of ordinary text request token estimation.

## Repository Findings

- `crates/service/src/quota/model_pricing.rs` has no `grok-*` official seed, and unknown model providers currently fall back to `openai`.
- `crates/service/src/gateway/observability/request_log.rs` converts a missing price result to `0.0`, which explains the observed zero-cost Grok logs.
- The shared usage parser already reads cached and reasoning tokens from OpenAI-compatible usage objects, but it does not retain `cost_in_usd_ticks` or `cost_in_nano_usd`.
- `RequestPricingSnapshot` stores local breakdown fields but has no cost-source, provider-actual, local-estimate, or variance fields.

## Runtime Diagnosis (2026-07-16)

The repackaged desktop process was running `apps/src-tauri/target/release/CodexManager.exe`, built at `2026-07-16 10:49:54`. Repackaging reproduced the existing pricing behavior because the Grok billing task is still in `planning` and the release source still contains neither Grok official seeds nor provider-cost capture.

Read-only inspection of `C:/Users/shuan/AppData/Roaming/com.codexmanager.desktop/codexmanager.db` found a more immediate runtime path than the generic missing-rule fallback:

- Recent successful Grok request logs (`request_logs.id` 38706, 38709, 38711) contain non-zero input, cached input, output, reasoning, and total tokens.
- Each pricing snapshot says `price_status = ok`, `matched_pattern = grok-4.5`, `price_source = aggregate_api_sync`, but every cost component and `total_cost_usd` is `0.0`.
- The matched row is `agg-sync-ag_e61927004eee-grok-4.5`, an enabled exact Standard rule with provider `openai` and input/cached/output prices all set to `0.0`.
- `ensure_model_price_rules_for_aggregate_api` intentionally creates unknown discovered models with zero prices (`crates/service/src/apikey/apikey_models.rs:1026-1077`). Because Grok is absent from `PRICE_SEEDS`, `infer_provider` also falls back to `openai` (`crates/service/src/quota/model_pricing.rs:624-635`).
- The estimator treats any structurally valid zero-price rule as `price_status = ok` and computes a real `Some(0.0)` (`crates/service/src/quota/model_pricing.rs:1023-1077`), so the later `unwrap_or(0.0)` is not the direct cause for these specific rows.

For request 38711, the token-only Standard short-context fallback would be non-zero even without provider actual cost:

```text
plain input = 192,742 - 191,616 = 1,126
local estimate = 1,126 * $2.00 / 1M
               + 191,616 * $0.50 / 1M
               + 550 * $6.00 / 1M
               = $0.10136
```

The upstream source is an Aggregate API. Therefore provider `usage.cost_in_usd_ticks` may be stripped by the intermediary, making the local Grok seed a required fallback rather than an optional diagnostic. A future official Grok seed with the existing official priority (`~10,000`) will outrank the aggregate placeholder priority (`-10`), and future aggregate sync will skip creating the placeholder once `resolve_model_price` recognizes Grok. Provider-cost parsing remains necessary for direct xAI responses and intermediaries that preserve the field.

## Implementation Consequences

1. Prefer provider-reported actual cost over local token estimation.
2. Keep local official seeds for OpenAI-compatible aggregators that remove xAI-specific usage fields.
3. Persist the raw integer ticks for audit and only convert to floating-point USD at the accounting boundary.
4. Keep local estimate alongside actual cost to detect upstream or seed drift.
5. Add an inclusive long-context comparator instead of encoding `199999` as a fake threshold.
6. Preserve raw completion/reasoning analytics, but make xAI fallback billing include all billable output tokens.
7. Mark tool-bearing fallback estimates `partial` when exact provider cost is unavailable.

## Gaps and Assumptions

- No remote price synchronization is planned; prices are a dated official seed.
- Alias targets can change in the future. Provider-reported actual cost limits the accounting risk, but official seeds still require versioned updates.
- Aggregate providers may alter or omit xAI-specific fields. The fallback path must remain functional and visibly identified as an estimate.
