# Grok / xAI Request Billing

## Scenario: Provider-reported Grok costs with token-price fallback

### 1. Scope / Trigger

- Applies when a gateway response for an xAI model provides `usage.cost_in_usd_ticks`, or when Grok 4.5 must be priced from usage tokens because the upstream/aggregate protocol omitted that field.

### 2. Signatures

- SQLite `model_price_rules.long_context_threshold_inclusive INTEGER NOT NULL DEFAULT 0`.
- SQLite `request_pricing_snapshots`: `cost_source`, `provider_cost_usd_ticks`, `provider_cost_usd`, `local_estimated_cost_usd`, and `pricing_variance_usd`.
- Request-log RPC fields: `pricingCostSource`, `providerCostUsd`, `localEstimatedCostUsd`, `pricingVarianceUsd`, and `longContextThresholdInclusive`.

### 3. Contracts

- Parse only non-negative integer `usage.cost_in_usd_ticks`; the terminal usage snapshot replaces a streaming running value.
- Convert ticks once with `ticks / 10_000_000_000.0`. Provider-reported cost is the charged base amount; local estimation remains audit-only.
- Use local official xAI rules for `grok-4.5`, `grok-4.5-latest`, and `grok-build-latest` if provider cost is absent. The 200K threshold is inclusive; old rules remain exclusive by default.
- Apply an Aggregate API multiplier only to the selected effective charge, never to raw provider/local audit columns.

### 4. Validation & Error Matrix

- Missing/negative/non-integer provider cost -> ignore it and use local pricing.
- Valid provider ticks -> `cost_source = provider_reported` and persist the pre-multiplier USD value.
- No provider ticks but an official price matches -> `cost_source = local_estimate`.
- No matching price -> retain the existing `missing` status; do not synthesize a successful zero-price rule.

### 5. Good / Base / Bad Cases

- Good: a final `cost_in_usd_ticks = 37_756_000` records `$0.0037756` and a local variance.
- Base: an Aggregate API response that strips xAI cost still produces a Grok 4.5 token estimate.
- Bad: summing every streamed cost snapshot, or treating a negative tick count as a credit.

### 6. Tests Required

- Parser tests for Chat/Responses usage, nested `response.usage`, invalid values, and terminal overwrite behavior.
- Pricing tests for aliases, Standard/Priority prices, `199999`/`200000`, and legacy strict thresholds.
- Request-log integration tests asserting final charge, raw audit fields, and multiplier behavior.

### 7. Wrong vs Correct

```rust
// Wrong: stream fragments are cumulative snapshots, not line items.
total_ticks += chunk.usage.cost_in_usd_ticks.unwrap_or(0);

// Correct: the terminal snapshot replaces the running value.
if let Some(ticks) = chunk.usage.cost_in_usd_ticks {
    usage.provider_cost_usd_ticks = Some(ticks);
}
```
