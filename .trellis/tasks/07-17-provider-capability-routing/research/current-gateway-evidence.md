# Current gateway evidence

## Reproduction

- Session `019f6e8d-2929-7333-9de9-3640ebeb4268` produced Grok request-log
  rows `42423` and `42424`, both 502.
- Both rows attempted Aggregate APIs in the order `input`, `esfaery-grok`,
  `codexforme-grok` and ended with
  `permission_error: Image generation is not enabled for this group`.
- The next GPT request (`42425`) succeeded through `input`.
- A separate controlled GPT-to-Grok CLI replay succeeded when its Grok body had
  no image-generation tool, previous response ID, or encrypted reasoning state.

## Existing capability diagnostics

- `crates/service/src/aggregate_api.rs:46` defines diagnostics as an immediate
  list of named probe results with status/reason/risk/recommended mode.
- `crates/service/src/aggregate_api.rs:2394` probes models, Responses, compact,
  WebSocket (not tested by default), and hosted image generation.
- `crates/service/src/aggregate_api.rs:2508` returns the result directly; it does
  not persist facts or feed runtime request planning.
- The existing Aggregate API page already exposes the diagnostic action, making
  it the correct UI location for effective capability state and overrides.

## Existing request and routing behavior

- `crates/service/src/gateway/request/official_responses_http.rs:319` normalizes
  `tool_choice` and `:355` optionally injects hosted image generation globally.
  It does not remove an incoming tool based on the selected supplier.
- `crates/service/src/gateway/upstream/protocol/aggregate_api.rs:1121` iterates
  candidates, while `:1264` only applies retry body and model override before
  building the upstream body. There is no candidate-specific capability plan.
- The proxy loop records only attempted IDs in the final request log. The first
  two supplier error causes from the reproduced Grok requests are not retained.
- `crates/service/src/gateway/upstream/protocol/aggregate_api.rs:1627` records
  ordinary candidate failure without a capability error class, so a capability
  mismatch can affect source-wide cooldown.
- `crates/service/src/gateway/routing/aggregate_api_cooldown.rs:11` uses five
  consecutive failures and a five-minute source-wide cooldown.

## Reusable project patterns

- `gateway_reasoning_guard_events` keeps intermediate retry events separate from
  the final `request_logs` row.
- `crates/service/src/gateway/observability/reasoning_guard_events.rs` uses a
  bounded async queue in production and synchronous writes in tests. Capability
  attempt events should reuse this operational pattern.
- The current request-log retention default is 14 days through
  `CODEXMANAGER_REQUEST_LOG_RETENTION_DAYS`.
- Route evidence and system policy actions already distinguish transport,
  capacity, rate-limit, and capability categories, but capability evidence does
  not yet drive a request-contract resolver.

## Design implications

1. Keep diagnostics as a producer of facts, not the runtime resolver itself.
2. Put request inspection, resolution, planning, classification, and transforms
   in focused gateway capability modules.
3. Integrate with the legacy proxy loop through small typed hooks.
4. Store operator overrides, expiring observations, and redacted attempt events
   separately because they have different ownership and retention semantics.
5. Use structural signatures and stable transform/error codes; never persist raw
   request or response bodies.
