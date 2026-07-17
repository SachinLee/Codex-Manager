# Design

## 1. Architecture

Add a capability-aware planning boundary between normalized client requests and
candidate-specific provider adaptation:

```text
normalized immutable request
  -> RequestCapabilityIntent
  -> CapabilityResolver + candidate facts
  -> CandidatePlanSet
       native-compatible candidates
       safe-downgrade candidates
       incompatible candidates
  -> existing ordered/balanced routing within each phase
  -> candidate-specific immutable EffectiveRequest
  -> upstream
  -> classified result / observation / bounded retry
```

The capability layer decides request compatibility and transformations. Existing
routing still owns health, cooldown, quota, cost, order, and candidate transport.

## 2. Module boundaries

Create focused modules under `crates/service/src/gateway/capability/`:

- `keys.rs`: bounded capability keys, state, confidence, protocol and scope.
- `intent.rs`: extracts required/optional/provider-state requirements without
  reading prompt text.
- `resolver.rs`: loads matching overrides/observations and resolves precedence
  plus wildcard specificity.
- `planner.rs`: partitions candidates and emits immutable effective plans.
- `transforms.rs`: code-owned transform allowlist and safety preconditions.
- `classifier.rs`: structured HTTP/SSE capability error classifiers.
- `runtime.rs`: `off/observe/enforce` setting and built-in profiles.

Add small integration calls to the existing Aggregate API proxy path. Do not put
the new resolver or classifiers directly into the legacy proxy loop.

Use a focused service module for Aggregate API capability administration and a
focused frontend component/hook under the Aggregate API feature instead of
expanding `page.tsx` with new orchestration.

## 3. Capability model

### 3.1 Keys

Wire keys are stable dotted strings; Rust parsing maps known keys to a typed enum
and retains an `Unknown(String)` form for forward-compatible reads.

Initial keys:

- `responses.endpoint`
- `responses.streaming`
- `responses.websocket`
- `responses.state.previous_response_id`
- `responses.reasoning.encrypted_content`
- `responses.tool.function`
- `responses.tool.custom`
- `responses.tool.namespace`
- `responses.hosted_tool.image_generation`
- `responses.hosted_tool.web_search`

Unknown keys can be stored and displayed but cannot execute a transform until a
code-owned transform policy is implemented.

### 3.2 Scope and specificity

```text
CapabilityScope {
  source_kind,
  source_id,
  upstream_model_pattern,
  protocol,
  capability_key
}
```

`*` is allowed for model pattern and protocol. Match precedence is exact model +
exact protocol, exact model + wildcard protocol, wildcard model + exact
protocol, then both wildcard. Source identity and capability key are always
exact for persisted supplier facts.

### 3.3 Facts

```text
CapabilityFact {
  state: supported | unsupported | unknown,
  source: operator | runtime | probe | builtin,
  confidence: high | medium | low,
  evidence_code,
  observed_at,
  expires_at,
  occurrence_count
}
```

Resolution order is operator, unexpired runtime, unexpired probe, builtin, then
unknown. Within one layer, scope specificity wins before recency. Operator reset
deletes the override rather than writing an `unknown` override.

## 4. Persistence

Add migration `116_gateway_capability_routing.sql` with three tables. Migration
`115` is already owned by the Grok 4.5 billing work in this shared worktree.

### 4.1 `gateway_capability_overrides`

- scope columns from `CapabilityScope`
- `state` (`supported` or `unsupported`)
- `created_at`, `updated_at`
- unique index across the complete scope

### 4.2 `gateway_capability_observations`

- scope columns
- `state`, `observation_source` (`runtime` or `probe`), `confidence`
- stable `evidence_code`; no raw upstream body
- `first_observed_at`, `last_observed_at`, `expires_at`
- `occurrence_count`
- unique key across scope, source, state and evidence code for upsert/coalescing
- indexes for effective resolution and expiry pruning

Default TTL policy:

- high-confidence negative runtime: seven days
- positive runtime/probe: 24 hours
- low-confidence or generic errors: not persisted as capability observations

### 4.3 `gateway_upstream_attempt_events`

- `trace_id`, `request_log_id`, `attempt_index`, `phase`
- source identity, supplier name, upstream model, protocol/path
- `contract_signature`, `capability_decisions_json`, `transform_codes_json`
- `error_class`, `error_code`, `http_status`, `duration_ms`, `outcome`
- `delivery_started`, `created_at`

The JSON fields contain only bounded codes/states, not request values. Production
writes use a bounded async queue with synchronous fallback, following the
reasoning-guard event pattern. Tests write synchronously. Pruning follows request
log retention.

## 5. Request intent

`RequestCapabilityIntent` contains required, optional and provider-state
capabilities plus a redacted structural signature.

Required intent is derived from:

- endpoint semantics;
- an explicit tool object/type in `tool_choice`;
- a Manager-private `x-codexmanager-required-capabilities` header or equivalent
  internal metadata, validated against known keys and removed upstream.

With `tool_choice=auto`, tools in the catalog are optional. Prompt text and tool
arguments are never inspected.

The structural signature is built from top-level field names, input item type
counts, role counts, tool type counts, streaming flag, and boolean presence of
provider-state fields. It never hashes or stores content values.

## 6. Effective plan and safe transforms

Each candidate receives a fresh plan from the immutable normalized body:

```text
CandidatePlan {
  phase: native | downgrade | incompatible,
  decisions: Vec<CapabilityDecision>,
  transforms: Vec<TransformCode>,
  effective_body: Bytes,
  incompatibility: Option<ClassifiedCapabilityError>
}
```

Initial transforms:

### `drop_optional_image_generation`

Allowed only when image generation is present in `tools` and is not forced by
endpoint, `tool_choice`, or private required-capability declaration. Remove only
the matching tool definition. If a tool-choice reference would become dangling,
the transform is not safe.

### `drop_previous_response_id_for_stateless_replay`

Allowed only when the request contains a complete replayable input transcript as
defined by a tested predicate. If the previous response ID is the sole context,
the capability is required and the candidate is incompatible.

### `drop_encrypted_reasoning_for_stateless_replay`

Remove `reasoning.encrypted_content` from `include`, reasoning items, and nested
encrypted fields only when the same stateless-replay predicate proves visible
conversation state remains. Otherwise the candidate is incompatible.

Every transform returns a new body and a bounded transform code. Transform
composition always starts from the original body.

## 7. Two-phase routing

For every routable candidate, resolve facts and produce one of:

- native: required features supported or admissible unknown; no semantic
  downgrade needed;
- downgrade: only optional features need allowlisted safe transforms;
- incompatible: a required feature is unsupported or a required transform is
  unsafe.

Run native candidates first, then downgrade candidates. Existing route strategy
is applied independently inside each list. Incompatible candidates emit a skip
event/evidence but are not sent upstream.

In `observe` mode the planner records the projected phase/transform but leaves
candidate order and body unchanged. In `off` mode it performs no planning or
events. `enforce` is the default.

## 8. Failure classification and recovery

Parse structured JSON and SSE failure envelopes before applying bounded text
matching. Classifiers return stable codes, for example:

- `capability.image_generation_not_enabled`
- `capability.previous_response_id_not_supported`
- `transport.timeout`
- `auth.rejected`
- `rate_limit.upstream`
- `capacity.model`
- `policy.content`
- `invalid_request`
- `upstream.unknown`

The initial image classifier requires the permission-error type plus the exact
group-entitlement message family. Generic status codes and `upstream failed`
never create capability facts.

When an optional capability is rejected with high confidence:

1. Persist/coalesce the scoped negative observation.
2. Ensure no client bytes were delivered.
3. Ensure the candidate downgrade budget is unused and deadline remains.
4. Rebuild from the immutable original body with the corresponding transform.
5. Retry the same candidate once.
6. Record both attempt events; do not record supplier health failure for the
   capability rejection.

If safe downgrade is unavailable, continue failover. Partial delivery terminates
recovery and returns the existing stream failure behavior.

## 9. Health, cooldown and evidence

Introduce a typed failure disposition used by the Aggregate API loop:

```text
FailureDisposition {
  class,
  retry_same_candidate,
  failover,
  affect_source_health,
  capability_observation
}
```

Only transport/auth/rate/capacity classes reach existing source health logic.
Capability, policy and client-invalid-request classes do not increment the
source-wide consecutive-failure counter. Route evidence uses stable sanitized
reasons and links to attempt events by trace ID.

## 10. Runtime settings

Add persisted app setting and environment override:

- API field: `capabilityRoutingMode`
- env: `CODEXMANAGER_CAPABILITY_ROUTING_MODE`
- accepted values: `off`, `observe`, `enforce`
- default: `enforce`

The runtime setter validates values, updates the in-memory atomic/cache, and
mirrors existing app-setting patterns. Mode changes do not require restart.

## 11. RPC and UI

Add Aggregate API RPC methods with synchronized desktop and Web mappings:

- `aggregateApi/capabilities/get`
- `aggregateApi/capabilities/setOverride`
- `aggregateApi/capabilities/resetOverride`
- `aggregateApi/capabilities/clearObservation`
- `aggregateApi/capabilities/listRecentAttempts`

Responses use camelCase and never expose secrets or raw upstream payloads.

Add a focused Capability panel to the existing Aggregate API details/diagnostic
surface. Use a dedicated hook/client wrapper and component. The panel displays
effective state, source/confidence/scope/expiry, recent evidence, override
controls, learned reset, and global mode. No new route or navigation item.

## 12. Metrics

Add bounded metrics such as:

- plans by `mode` and `phase`
- attempts by `phase`, `outcome`, and bounded `error_class`
- downgrade retries by bounded `transform_code` and outcome
- capability observations by known capability family and state

Do not label metrics by source ID, supplier name, model string, trace ID, or
unknown capability key.

## 13. Compatibility, rollout and rollback

- Migration is additive; empty fact tables preserve existing records as unknown.
- Default enforce changes behavior only through tested allowlisted transforms.
- `observe` allows production comparison before or during rollout.
- `off` immediately restores legacy request/order behavior while retaining data.
- Schema rollback is unnecessary for operational rollback; tables can remain.
- Unknown keys survive RPC/storage round trips but cannot trigger transforms.

## 14. Test matrix

### Unit

- scope specificity and precedence
- TTL expiry and observation coalescing
- request intent for endpoint, forced tool, auto tool, private declaration
- every transform's safe/unsafe preconditions and immutability
- exact classifier positives and near-match negatives
- failure disposition and cooldown neutrality

### Gateway integration

- streaming/non-streaming optional image rejection then same-candidate recovery
- required image rejection without downgrade
- native phase before downgrade phase with order preserved per phase
- candidate bodies isolated across failover
- generic 502 creates no capability fact
- no retry after delivery starts
- `off/observe/enforce` behavior
- reasoning-guard and capability retry budgets remain independent

### Storage/RPC

- migration on existing DB
- override/observation CRUD, wildcard lookup, expiry pruning
- attempt-event redaction and 14-day pruning
- desktop/RPC/Web command parity and camelCase compatibility

### Frontend

- normalization of missing/unknown fields
- override/reset interactions and errors
- runtime transport tests
- static desktop build

## 15. Main risks

- Silent semantic weakening: prevented by machine-readable required intent and
  transform preconditions.
- Retry after partial SSE delivery: prevented by delivery-start guard.
- False learning from vague errors: prevented by exact classifiers.
- Routing churn from stale facts: bounded by TTL and operator override/reset.
- Hot-path SQLite latency: bounded async event queue; small indexed fact reads
  may be cached with invalidation on update/expiry.
- Legacy-file growth: new logic stays in focused modules with narrow integration
  hooks.
