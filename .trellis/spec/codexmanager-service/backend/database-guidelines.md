# Database Guidelines

> Database patterns and conventions for this project.

---

## Overview

<!--
Document your project's database conventions here.

Questions to answer:
- What ORM/query library do you use?
- How are migrations managed?
- What are the naming conventions for tables/columns?
- How do you handle transactions?
-->

(To be filled by the team)

---

## Query Patterns

<!-- How should queries be written? Batch operations? -->

(To be filled by the team)

---

## Migrations

<!-- How to create and run migrations -->

(To be filled by the team)

---

## Naming Conventions

<!-- Table names, column names, index names -->

(To be filled by the team)

---

## Common Mistakes

<!-- Database-related mistakes your team has made -->

(To be filled by the team)

## Scenario: Reasoning Guard Event Observability

### 1. Scope / Trigger
- Trigger: Gateway Reasoning Guard observability spans storage, request-log RPC, gateway retry flow, and aggregate API UI statistics.
- Use a dedicated event table for intermediate Guard actions. Keep `request_logs` as the final client request summary, especially when an internal retry later succeeds.

### 2. Signatures
- DB table: `gateway_reasoning_guard_events`
  - `trace_id TEXT`
  - `request_log_id INTEGER`
  - `mode TEXT NOT NULL` where values are `stream` or `non_stream`
  - `action TEXT NOT NULL` where values include `observe_only`, `internal_retry`, `recovered`, `block`, `bypass_after_consecutive`
  - `target_token INTEGER`
  - `source_kind TEXT` such as `openai_account` or `aggregate_api`
  - `source_id TEXT`
  - `supplier_name TEXT`
  - `upstream_model TEXT`
  - `request_path TEXT`
  - `attempt_index INTEGER NOT NULL DEFAULT 0`
  - `final_status_code INTEGER`
  - `created_at INTEGER NOT NULL`
- RPC method: `requestlog/aggregate_api_reasoning_guard`
- RPC method: `requestlog/aggregate_api_daily_usage`
- Web command: `service_requestlog_aggregate_api_reasoning_guard`
- Web command: `service_requestlog_aggregate_api_daily_usage`

### 3. Contracts
- Internal retry events must not create ordinary request-log rows for the blocked intermediate response.
- A later successful retry may write a `recovered` event with the same `trace_id`; the final `request_logs` row records the successful response usage.
- Aggregate API stats are grouped by `source_kind = 'aggregate_api'` and `source_id = aggregateApiId`.
- Billable usage summaries add only `action = 'internal_retry'` Guard usage. Do not bill `block`, `observe_only`, `recovered`, or `bypass_after_consecutive` events.
- `requestlog/aggregate_api_daily_usage` returns base usage plus `guardRetryTotalTokens`, `guardRetryEstimatedCostUsd`, `billableTotalTokens`, and `billableEstimatedCostUsd`.
- Request-log filtered summaries use `trace_id` to join Guard internal retry usage back to the final request row; aggregate API daily summaries use `source_kind = 'aggregate_api'` and `source_id`.
- The RPC response uses camelCase fields: `totalRequestCount`, `eventCount`, `affectedRequestCount`, `matchRate`, `internalRetryCount`, `internalRetryRequestCount`, `retryRecoveryCount`, `retryRecoveryRate`, `blockCount`, `blockedRequestCount`, `blockRate`, `observeOnlyCount`, `bypassAfterConsecutiveCount`, `lastTargetToken`, `lastEventAt`.

### 4. Validation & Error Matrix
- Invalid RPC params -> return an RPC error with `invalid requestlog/aggregate_api_reasoning_guard params`.
- Storage open failure -> return `open storage failed`.
- Summary query failure -> return `summarize aggregate api reasoning guard failed: ...`.
- No request logs in range -> rates are `0.0`, not division errors.
- Latest event with `target_token IS NULL` -> do not overwrite `lastTargetToken`; use the latest non-null token event.
- Legacy databases without `gateway_reasoning_guard_events` -> summary queries must create/ensure the table before joining it.

### 5. Good/Base/Bad Cases
- Good: one aggregate API has two requests, one `internal_retry`, one `recovered`, and one `block`; `matchRate = affectedRequestCount / totalRequestCount`, `retryRecoveryRate = retryRecoveryCount / internalRetryRequestCount`, and `blockRate = blockedRequestCount / totalRequestCount`.
- Good: daily usage with base cost `$0.19` and an internal retry cost `$0.04` reports `estimatedCostUsd = 0.19`, `guardRetryEstimatedCostUsd = 0.04`, and `billableEstimatedCostUsd = 0.23`.
- Base: no Guard events for an aggregate API; UI shows no hits and rates are zero.
- Bad: writing `internal_retry` to `request_logs` as a final failed request; this inflates request counts and hides the eventual successful retry.
- Bad: summing a `block` event into billable usage; blocked intermediate responses are observability data, not additional chargeable successful work.

### 6. Tests Required
- Storage/RPC test inserts request logs and `gateway_reasoning_guard_events`, then asserts camelCase output and rate calculations.
- Storage test for aggregate API daily usage inserts one `internal_retry` and one `block` event, then asserts only `internal_retry` contributes to `guardRetry*` and `billable*` fields.
- Request-log filtered summary test inserts an internal retry event with the final request `trace_id` and asserts filtered total tokens/cost include the retry.
- Gateway integration test asserts `ReasoningGuardBridgeAction::InternalRetry` retries the same candidate, does not fail over, does not mark the account unavailable, and only logs the final successful request.
- UI normalization/build test must cover missing fields and clamp rate fields to `0..=1`.

### 7. Wrong vs Correct

#### Wrong
```rust
// Wrong: the intermediate synthetic 502 is treated as the final client result.
context.log_final_result_with_model(..., 502, usage, Some("reasoning_tokens=1034"), ...);
```

#### Correct
```rust
// Correct: record the intermediate Guard action separately and preserve the final request log.
context.record_reasoning_guard_event(..., ReasoningGuardBridgeAction::InternalRetry, Some(1034), ...);
return Ok(FinalizeUpstreamResponseOutcome::RetrySameCandidate { request });
```

#### Wrong
```sql
-- Wrong: all Guard events are treated as billable retry usage.
SELECT SUM(estimated_cost_usd)
FROM gateway_reasoning_guard_events
WHERE source_kind = 'aggregate_api';
```

#### Correct
```sql
-- Correct: only internal retry attempts add billable retry usage.
SELECT SUM(estimated_cost_usd)
FROM gateway_reasoning_guard_events
WHERE source_kind = 'aggregate_api'
  AND action = 'internal_retry';
```
