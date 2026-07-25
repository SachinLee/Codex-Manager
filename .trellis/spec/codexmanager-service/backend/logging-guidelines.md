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

## Scenario: Request Log Session Identity

### 1. Scope / Trigger

- Trigger: changing Codex request headers, Responses request normalization, HTTP or WebSocket request-log finalization, or session-title projection.
- Applies to `gateway/request/incoming_headers.rs`, `gateway/local_validation/request.rs`, `http/responses_websocket.rs`, `gateway/upstream/protocol/aggregate_api.rs`, `gateway/upstream/proxy.rs`, and request-log tests.

### 2. Signatures

- Accepted session header aliases: `session-id`, `session_id`, and `x-session-id`.
- Trusted parent fallback: `x-codex-parent-thread-id`.
- Explicit request-body fields may appear at the top level, under `client_metadata`, or under `metadata`: `session_id`, `sessionId`, `codex_session_id`, `codexSessionId`, `thread_id`, `threadId`, `codex_thread_id`, and `codexThreadId`.
- Persisted field: `RequestLogTraceContext.session_id` -> `request_logs.session_id`.

### 3. Contracts

- Resolution precedence is `session header > parent-thread header > explicit body candidate > recognized prompt_cache_key`.
- HTTP, WebSocket, and Aggregate API finalizers must all copy the resolved session ID into `RequestLogTraceContext`; forwarding a header upstream is not equivalent to logging it. Aggregate route success/failure logs must not drop `session_id` / `conversation_anchor` via `..Default::default()`.
- Body-derived IDs are logging metadata only. They must not change routing, session affinity, or the upstream request body.
- A persisted candidate is limited to 256 bytes of printable ASCII.
- `prompt_cache_key` is accepted only when it is a Codex UUID-shaped thread ID or a non-empty `local:` ID. Route anchors beginning with `pck:v1:` and arbitrary cache keys are not session IDs.

### 4. Validation & Error Matrix

- Empty, control-character-containing, or longer-than-256-byte candidate -> ignore it for request-log session identity.
- Empty `local:` suffix -> ignore it.
- `pck:v1:` route anchor -> ignore it.
- Header and body disagree -> persist the header value.
- No valid identity source -> persist `NULL`; do not invent an ID from request order or timing.

### 5. Good / Base / Bad Cases

- Good: a Codex request with `x-session-id` stores that ID for both HTTP and WebSocket logs, allowing the UI to resolve the local thread title.
- Base: a compatible Responses client omits session headers but supplies `client_metadata.thread_id`; the bounded explicit ID is stored only in the log.
- Bad: the upstream receives `x-session-id`, but the WebSocket finalizer omits `RequestLogTraceContext.session_id`, leaving the UI with `-`.
- Bad: the Aggregate API path resolves a body/header session ID, then writes request logs with `..Default::default()` and drops `session_id`.
- Bad: treating every `prompt_cache_key` as a session ID, which can link unrelated requests or persist attacker-controlled labels.

### 6. Tests Required

- Unit tests must cover all three header aliases through both axum and tiny_http parsers.
- Resolver tests must prove header precedence over body metadata.
- Body parsing tests must cover explicit metadata, UUID and `local:` cache keys, route-anchor rejection, ordinary-key rejection, control characters, empty local IDs, and the 256-byte bound.
- HTTP request-log tests must write and read a log row using a body-derived candidate.
- WebSocket proxy tests must assert every finalized request-log row stores `x-session-id`.
- Aggregate API request-log contexts must include `session_id_for_log` / `conversation_anchor_for_log` on every write path.

### 7. Wrong vs Correct

#### Wrong

```rust
let request_log_session_id = incoming_headers.session_id().map(str::to_string);
// The WebSocket finalizer omits session_id even though the header was parsed.
```

#### Correct

```rust
let request_log_session_id = resolve_request_log_session_id(
    &incoming_headers,
    [request_metadata.session_id_candidate.as_deref()],
);

RequestLogTraceContext {
    session_id: request_log_session_id.as_deref(),
    ..Default::default()
}
```

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
  - `codexmanager_gateway_upstream_capacity_internal_retries_total`
- Service event recorder:
  - `record_gateway_reasoning_guard_event(event: GatewayReasoningGuardEvent)`

### 3. Contracts
- Default targets are `[516, 1034, 1552]`; invalid, duplicate, or non-positive target values are ignored, and an empty normalized list falls back to defaults.
- `reasoningGuardRetryAttempts = 0` means no internal retry; a positive value retries the same candidate before synthesizing a guard 502.
- When `reasoningGuardEnabled` is true, streaming and non-streaming intercepts must not both be false.
- A reasoning guard match is account-neutral: it must not mark an account unavailable, set provider failure state, or record normal failover unless a separate non-guard upstream error occurs.
- Internal retry actions must return through `RetrySameCandidate`, reacquire the same account inflight guard, and retry the same account/candidate rather than moving to the next candidate.
- `ContinuationRecovery` is a safe semantic retry, not stream splicing. The matched/truncated stream round is untrusted; do not forward its SSE chunks, tool calls, output messages, lifecycle events, or encrypted reasoning items to the client.
- Continuation recovery requests must be rebuilt from the original/base request body for that candidate, not from the previously generated continuation body. This prevents accumulated commentary markers across repeated `516 -> 1034 -> clean` recovery chains.
- Continuation recovery requests must remove `previous_response_id`, force `stream=true`, remove `reasoning.encrypted_content` from `include`, replay only sanitized original `input`, and append exactly one `phase="commentary"` marker.
- Sanitized continuation replay must drop original input items with `type="reasoning"` and recursively remove every `encrypted_content` field before sending the retry upstream. Do not auto-add `reasoning.encrypted_content` just because continuation recovery is enabled.
- Upstream capacity retry uses the same `RetrySameCandidate` transport path with a distinct reason. It must not reuse the reasoning guard retry budget or metrics.
- Capacity matching is intentionally narrow: match only `Selected model is at capacity. Please try a different model.` plus project-generated `key=value` prefixes or debug suffixes; do not use generic substring matching for `capacity`.
- Gateway tests that mutate `reasoningGuard*` env or runtime settings must initialize the full guard config under `test_env_guard`, including enabled state, targets, both intercept switches, retry attempts, and consecutive-bypass threshold. Runtime setters mirror values back into env vars, so partial per-test setup can leak state into later server startups.
- Persist reasoning guard request-log events through `record_gateway_reasoning_guard_event`; do not call `storage.insert_gateway_reasoning_guard_event` directly from gateway request execution or HTTP bridge code.
- Request-log and billing rollups must treat `internal_retry` and `continuation_recovery` as retry-class guard actions. A new guard retry action must update the shared storage retry-action predicate before it can affect request-log badges, guard retry token totals, cost totals, or billable usage.
- Production event persistence must keep synchronous SQLite work out of the request hot path. Use a bounded async queue for normal writes and fall back to synchronous insertion only when the queue is full or disconnected, so events are not silently lost under pressure.
- Tests should keep reasoning guard event insertion synchronous. This avoids background threads opening storage after a test-specific DB path or env guard has been reset.

### 4. Validation & Error Matrix
- Enabled + both intercepts false -> reject app settings patch.
- Missing persisted setting -> use runtime default.
- Match + intercept disabled -> observe only, count match, deliver upstream response.
- Match + retry budget remains -> count match and internal retry, retry same candidate, do not block.
- Match + continuation recovery selected on a Responses stream -> discard the matched round body, build a sanitized continuation request from the base request, and only deliver the final clean upstream stream.
- Repeated continuation matches -> each retry body contains the sanitized base input plus exactly one commentary marker; no prior marker, matched-round reasoning item, or matched-round output is replayed.
- Match + retry budget exhausted -> count match and block, synthesize 502 with `codexmanager_reasoning_guard`.
- Non-matching reasoning tokens -> reset consecutive guard state and pass through normally.
- Test only sets one guard env key after a previous test disabled the guard -> later startup may inherit stale runtime/env state; set the full guard env matrix for every reasoning guard gateway test.
- Capacity message match + capacity retry budget remains -> count capacity internal retry, retry the same candidate, and skip ordinary gateway error follow-up/failover.
- Capacity message match + capacity retry budget exhausted -> return the upstream capacity error to the client without ordinary failover follow-up.
- Event queue accepts a reasoning guard event in production -> return to the request flow immediately; a background worker writes the event to SQLite.
- Event queue is full or disconnected -> synchronously insert the event and log only if storage is unavailable or insertion fails.
- Test records a reasoning guard event -> insert synchronously in the same call so assertions do not race a background writer.

### 5. Good/Base/Bad Cases
- Good: first upstream response has `reasoning_tokens=1034`, retry budget is `1`, second same-account response is clean; client receives the second response, metrics increment match and internal retry only.
- Good: a `516 -> 1034 -> 128` Responses stream recovery sends two continuation requests, each rebuilt from the sanitized base input with one commentary marker, and the client only receives the final clean stream lifecycle and output.
- Good: bridge and proxy pipeline call `record_gateway_reasoning_guard_event`, keeping production request handling non-blocking while preserving event durability on queue backpressure.
- Base: retry attempts are `0`; a matching response returns 502 and does not call the next candidate.
- Bad: a guard match enters generic gateway error follow-up before the internal retry decision; this can mark the account unavailable or trigger ordinary failover.
- Bad: continuation recovery folds the matched stream chunks into the final response or replays matched-round encrypted reasoning items; this mixes response identities and leaks untrusted tentative output.
- Bad: bridge code opens storage and inserts the reasoning guard event directly before returning; this can add SQLite latency to every matching request and makes tests race background storage state.

### 6. Tests Required
- Runtime/app settings tests must cover default fields, persisted snapshot round-trip, normalized targets, and rejecting enabled guard with both intercept modes disabled.
- Gateway tests must cover non-stream block, stream strict buffering without leaked delta, observe-only/disabled behavior, consecutive bypass, configurable non-516 targets, and same-candidate internal retry.
- Continuation recovery gateway tests must assert that matched stream deltas are not delivered, `previous_response_id` is absent from the continuation request, `reasoning.encrypted_content` is not requested automatically, original reasoning items are not replayed, nested `encrypted_content` fields are stripped, and repeated recovery attempts do not accumulate multiple commentary markers.
- Gateway reasoning guard tests should use a shared helper for complete guard env initialization so default parallel execution does not depend on test order.
- Metrics tests should assert match, block, and internal retry counters via `/metrics` or the narrowest available metrics API.
- Capacity retry tests should assert same-account retry and `codexmanager_gateway_upstream_capacity_internal_retries_total` without depending on unrelated async usage refresh side effects.
- Event persistence tests should assert through request-log/gateway-log surfaces after the request completes; do not depend on production background worker timing.
- Continuation recovery tests should assert both clean replay behavior and request-log/billing projections, including guard retry count plus retry token/cost rollups.
- Unit tests that directly exercise event recording should run against the test-only synchronous path.

### 7. Wrong vs Correct
#### Wrong
```rust
let folded = fold_continuation_chunks(matched_round_chunks, final_round_body);
return deliver_to_client(folded);
```

#### Correct
```rust
let continuation_body =
    build_continuation_recovery_body(base_attempt_body, marker_text);
return retry_same_candidate_with_body(continuation_body);
```

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
if let Some(storage) = crate::storage_helpers::open_storage() {
    let _ = storage.insert_gateway_reasoning_guard_event(&event);
}
```

#### Correct
```rust
record_gateway_reasoning_guard_event(event);
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
