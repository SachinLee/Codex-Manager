# Model Catalog Long-Context Billing

## Scenario: Global long-context tier selection with immutable request audit

### 1. Scope / Trigger

- Applies when a model catalog contains a base tier (`min_input_tokens = 0`) and one or more long-context tiers, or when changing the global App Settings billing policy.
- This is cross-layer work: App Settings RPC, SQLite charge snapshots, service estimates, request-log audit data, and the settings UI must agree.

### 2. Signatures

- App Settings RPC: `appSettings/get` and `appSettings/set({ longContextBillingEnabled?: boolean })`.
- Persisted key: `gateway.long_context_billing_enabled`; an absent key means `true`.
- SQLite: `request_charge_snapshots.long_context_billing_enabled INTEGER NOT NULL DEFAULT 1`.
- Core input: `ChargeSnapshotInputV2.long_context_billing_enabled: Option<bool>`; `None` preserves the legacy enabled default.
- Catalog helpers: `select_model_price_tier_with_long_context_billing_v2` and `resolve_model_price_from_catalog_with_long_context_billing`.

### 3. Contracts

- With the setting enabled, select the highest tier where `min_input_tokens = 0` or `min_input_tokens < input_tokens`. Thresholds are strictly exceeded, so 272,000 tokens use the base tier and 272,001 may use the 272K tier.
- With the setting disabled, select only the base tier. Route selection never changes this billing policy.
- Persist the resolved boolean and selected tier in `request_charge_snapshots`. Request-log pricing snapshots must derive `context_band` and local estimate from that immutable charge snapshot, never from a later settings read.
- A valid `base_cost_override_microusd` is provider-reported and remains authoritative; the local policy must not recompute it.

### 4. Validation & Error Matrix

- Negative tokens, rates, or multipliers -> reject the charge snapshot.
- Unknown model without provider-reported cost -> return `model_price_missing` or the existing missing-model error.
- Unknown model with non-negative provider-reported cost -> create the charge snapshot using that cost and optional catalog metadata.
- Missing setting key or `ChargeSnapshotInputV2.long_context_billing_enabled = None` -> use enabled behavior for compatibility.

### 5. Good / Base / Bad Cases

- Good: `longContextBillingEnabled = false` with 300K input persists `tier_min_input_tokens = 0` and `context_band = single_tier`.
- Base: an old database without the setting key continues to choose the long tier only after its threshold is strictly exceeded.
- Bad: reading the current setting while writing the request-log estimate after a charge snapshot has been created; a concurrent settings update can make audit values disagree.

### 6. Tests Required

- App Settings RPC test: absent key returns `true`; setting `false` returns and persists `false`.
- Core storage tests: below, exactly at, and above a tier threshold; disabled policy always selects base; stored snapshot retains the resolved boolean.
- Service pricing test: log estimate with the snapshot policy uses base and long amounts correctly.
- Provider-cost regression: a provider override remains the charged base amount regardless of policy.

### 7. Wrong vs Correct

```rust
// Wrong: audit depends on a mutable global setting after the charge is written.
let estimate = estimate_cost_usd_for_log(storage, Some(model), input, cached, cache_write, output);

// Correct: audit reuses the policy captured with the immutable charge snapshot.
let estimate = estimate_cost_usd_for_log_with_long_context_billing(
    storage,
    Some(model),
    input,
    cached,
    cache_write,
    output,
    charge.long_context_billing_enabled,
);
```
