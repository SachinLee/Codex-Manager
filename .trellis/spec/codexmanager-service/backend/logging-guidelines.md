# Logging Guidelines

> How logging is done in this project.

---

## Overview

<!--
Document your project's logging conventions here.

Questions to answer:
- What logging library do you use?
- What are the log levels and when to use each?
- What should be logged?
- What should NOT be logged (PII, secrets)?
-->

(To be filled by the team)

---

## Log Levels

<!-- When to use each level: debug, info, warn, error -->

(To be filled by the team)

---

## Structured Logging

<!-- Log format, required fields -->

(To be filled by the team)

---

## What to Log

<!-- Important events to log -->

(To be filled by the team)

---

## What NOT to Log

<!-- Sensitive data, PII, secrets -->

(To be filled by the team)

## Scenario: Gateway Reasoning Guard Runtime Contract

### 1. Scope / Trigger
- Trigger: Gateway reasoning guard behavior spans runtime env config, app settings API, HTTP bridge response handling, upstream retry flow, request logs, and Prometheus metrics.
- Use this contract when changing `crates/service/src/gateway/observability/http_bridge/**`, `crates/service/src/gateway/upstream/proxy_pipeline/**`, `crates/service/src/gateway/observability/metrics.rs`, or app settings fields named `reasoningGuard*`.

### 2. Signatures
- App settings API fields:
  - `reasoningGuardEnabled: boolean`
  - `reasoningGuardTargets: number[]`
  - `reasoningGuardInterceptStreaming: boolean`
  - `reasoningGuardInterceptNonStreaming: boolean`
  - `reasoningGuardRetryAttempts: number`
  - `reasoningGuardBypassAfterConsecutive: number`
- Runtime env keys:
  - `CODEXMANAGER_REASONING_GUARD_ENABLED`
  - `CODEXMANAGER_REASONING_GUARD_TARGETS`
  - `CODEXMANAGER_REASONING_GUARD_INTERCEPT_STREAMING`
  - `CODEXMANAGER_REASONING_GUARD_INTERCEPT_NON_STREAMING`
  - `CODEXMANAGER_REASONING_GUARD_RETRY_ATTEMPTS`
  - `CODEXMANAGER_REASONING_GUARD_BYPASS_AFTER_CONSECUTIVE`
- Prometheus metrics:
  - `codexmanager_gateway_reasoning_guard_matches_total{mode="stream|non_stream"}`
  - `codexmanager_gateway_reasoning_guard_blocks_total{mode="stream|non_stream"}`
  - `codexmanager_gateway_reasoning_guard_internal_retries_total{mode="stream|non_stream"}`

### 3. Contracts
- Default targets are `[516, 1034, 1552]`; invalid, duplicate, or non-positive target values are ignored, and an empty normalized list falls back to defaults.
- `reasoningGuardRetryAttempts = 0` means no internal retry; a positive value retries the same candidate before synthesizing a guard 502.
- When `reasoningGuardEnabled` is true, streaming and non-streaming intercepts must not both be false.
- A reasoning guard match is account-neutral: it must not mark an account unavailable, set provider failure state, or record normal failover unless a separate non-guard upstream error occurs.
- Internal retry actions must return through `RetrySameCandidate`, reacquire the same account inflight guard, and retry the same account/candidate rather than moving to the next candidate.
- Gateway tests that mutate `reasoningGuard*` env or runtime settings must initialize the full guard config under `test_env_guard`, including enabled state, targets, both intercept switches, retry attempts, and consecutive-bypass threshold. Runtime setters mirror values back into env vars, so partial per-test setup can leak state into later server startups.

### 4. Validation & Error Matrix
- Enabled + both intercepts false -> reject app settings patch.
- Missing persisted setting -> use runtime default.
- Match + intercept disabled -> observe only, count match, deliver upstream response.
- Match + retry budget remains -> count match and internal retry, retry same candidate, do not block.
- Match + retry budget exhausted -> count match and block, synthesize 502 with `codexmanager_reasoning_guard`.
- Non-matching reasoning tokens -> reset consecutive guard state and pass through normally.
- Test only sets one guard env key after a previous test disabled the guard -> later startup may inherit stale runtime/env state; set the full guard env matrix for every reasoning guard gateway test.

### 5. Good/Base/Bad Cases
- Good: first upstream response has `reasoning_tokens=1034`, retry budget is `1`, second same-account response is clean; client receives the second response, metrics increment match and internal retry only.
- Base: retry attempts are `0`; a matching response returns 502 and does not call the next candidate.
- Bad: a guard match enters generic gateway error follow-up before the internal retry decision; this can mark the account unavailable or trigger ordinary failover.

### 6. Tests Required
- Runtime/app settings tests must cover default fields, persisted snapshot round-trip, normalized targets, and rejecting enabled guard with both intercept modes disabled.
- Gateway tests must cover non-stream block, stream strict buffering without leaked delta, observe-only/disabled behavior, consecutive bypass, configurable non-516 targets, and same-candidate internal retry.
- Gateway reasoning guard tests should use a shared helper for complete guard env initialization so default parallel execution does not depend on test order.
- Metrics tests should assert match, block, and internal retry counters via `/metrics` or the narrowest available metrics API.

### 7. Wrong vs Correct
#### Wrong
```rust
let follow_up = context.apply_gateway_error_follow_up(account_id, error, has_more_candidates);
if bridge.reasoning_guard_action == Some(ReasoningGuardBridgeAction::InternalRetry) {
    return RetrySameCandidate { request };
}
```

#### Correct
```rust
let should_retry_same_candidate =
    bridge.reasoning_guard_action == Some(ReasoningGuardBridgeAction::InternalRetry);
let follow_up = if should_retry_same_candidate {
    None
} else {
    final_error
        .as_deref()
        .map(|error| context.apply_gateway_error_follow_up(account_id, error, has_more_candidates))
};
```

#### Wrong
```rust
let _guard_enabled = EnvGuard::set("CODEXMANAGER_REASONING_GUARD_ENABLED", "0");
// A later test only sets retry attempts and assumes the guard returns to default enabled.
let _guard_retry = EnvGuard::set("CODEXMANAGER_REASONING_GUARD_RETRY_ATTEMPTS", "0");
```

#### Correct
```rust
let _guard_env = reasoning_guard_test_env(
    true,  // enabled
    0,     // retry attempts
    0,     // bypass after consecutive
);
```
