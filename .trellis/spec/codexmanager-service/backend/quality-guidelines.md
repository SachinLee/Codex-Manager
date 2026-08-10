# Quality Guidelines

> Code quality standards for backend development.

---

## Overview

<!--
Document your project's quality standards here.

Questions to answer:
- What patterns are forbidden?
- What linting rules do you enforce?
- What are your testing requirements?
- What code review standards apply?
-->

(To be filled by the team)

---

## Forbidden Patterns

<!-- Patterns that should never be used and why -->

(To be filled by the team)

---

## Required Patterns

<!-- Patterns that must always be used -->

(To be filled by the team)

---

## Testing Requirements

<!-- What level of testing is expected -->

(To be filled by the team)

---

## Code Review Checklist

<!-- What reviewers should check -->

(To be filled by the team)

## Scenario: Aggregate Capability Diagnostics, Route Evidence, and Image Semantics

### 1. Scope / Trigger
- Trigger: Changes that add Aggregate API capability diagnostics, gateway route evidence, system policy-action summaries, or hosted image generation semantic validation.
- Applies to `crates/service/src/aggregate_api.rs`, `crates/service/src/rpc_dispatch/aggregate_api.rs`, `crates/service/src/gateway/routing/**`, `crates/service/src/gateway/observability/http_bridge/**`, `crates/service/src/requestlog/**`, Tauri aggregate API commands, and frontend typed API wrappers.

### 2. Signatures
- RPC method: `aggregateApi/diagnoseCapabilities`
  - Params: `{ id | apiId: string, liveSmoke?: boolean }`
  - Result: `{ id, providerType, diagnosedAt, latencyMs, nonMutating, liveSmoke, probes[] }`
- Tauri command: `service_aggregate_api_diagnose_capabilities(addr?: string, id: string, live_smoke?: boolean)`.
- Frontend wrapper: `accountClient.diagnoseAggregateApiCapabilities(apiId, { liveSmoke?: boolean })`.
- Request log summary additive fields: `routeEvidence: RouteEvidenceSummary[]`, `policyActions: GatewayPolicyActionSummary[]`.
- Hosted image generation semantic error class: `image_generation_missing_result`.

### 3. Contracts
- Capability diagnostic probes must not update Aggregate API `last_test_*`, balance, cooldown, routing, or account availability state.
- Default diagnostics must be conservative: WebSocket and hosted image generation live smoke are `not_tested` unless explicitly opted in.
- Probe statuses are limited to `supported`, `unsupported`, `unknown`, and `not_tested`.
- System policy actions are read-model summaries owned by `system`; first implementation supports only `kind = "cooldown"`.
- Manual user configuration and explicit enable/disable state always outrank system policy-action summaries.
- Hosted image generation success requires at least one `output` item with `type = "image_generation_call"` and a non-empty `result`.
- Do not log or persist API secrets, bearer tokens, ChatGPT tokens, or raw image base64 in diagnostics, evidence, or semantic error payloads.

### 4. Validation & Error Matrix
- Missing Aggregate API id -> return `aggregate api id required`.
- Unknown Aggregate API id -> return `aggregate api not found`.
- Missing secret -> return `aggregate api secret not found`.
- Probe HTTP 2xx -> `supported`.
- Probe HTTP 400/422 -> `supported` when the endpoint exists but rejects the minimal body.
- Probe HTTP 404/405/501 -> `unsupported`.
- Probe HTTP 401/403 or transport failure -> `unknown`.
- Hosted image generation HTTP 2xx with no valid `image_generation_call.result` -> gateway semantic failure with `image_generation_missing_result`.

### 5. Good/Base/Bad Cases
- Good: Aggregate API diagnostics return structured probe evidence and leave `last_test_status` unchanged.
- Good: A cooldown mark creates a system-owned temporary policy action with `createdAt`, `expiresAt`, `remainingSecs`, and source evidence.
- Base: Hosted image generation live smoke remains `not_tested` in the default UI because it may consume quota.
- Bad: A failed diagnostic marks an Aggregate API unavailable, updates balance, enters cooldown, or disables an account.
- Bad: An image response with HTTP 200 and empty `data` is returned as success.

### 6. Tests Required
- Diagnostic classification tests for supported, unsupported, and unknown HTTP statuses.
- Policy-action lifecycle tests that assert owner, target, source evidence, and expiration behavior.
- Request-log projection tests when evidence or policy-action fields change shape.
- Hosted image generation semantic tests for valid result, missing output, missing/empty result, and invalid JSON.
- Frontend build/type-check after changing typed API wrappers or request-log payload types.

### 7. Wrong vs Correct
#### Wrong
```rust
let result = test_aggregate_api_connection(api_id)?;
storage.update_aggregate_api_test_result(api_id, result.ok, result.status_code, result.message.as_deref())?;
```

#### Correct
```rust
let diagnostics = diagnose_aggregate_api_capabilities(api_id, false)?;
// Return diagnostics directly; do not mutate Aggregate API health or routing state.
```

#### Wrong
```rust
let response = build_images_api_response(&value, ImagesResponseFormat::B64Json);
return respond_json_bytes(request, StatusCode(200), headers, response);
```

#### Correct
```rust
if hosted_image_generation_semantic_error(&value).is_some() {
    return respond_json_bytes(request, StatusCode(502), headers, image_generation_semantic_error_body(message));
}
```

## Scenario: Provider Capability-Aware Aggregate Routing

### 1. Scope / Trigger

- Trigger: changing Aggregate API capability discovery, request compatibility,
  safe downgrade, learned observations, routing mode, or capability management.
- Applies across core storage, `gateway/capability/**`, the Aggregate API proxy,
  app settings, RPC/Tauri/Web command surfaces, and the typed frontend client.

### 2. Signatures

- Private request header:
  `x-codexmanager-required-capabilities: responses.hosted_tool.image_generation`.
- Runtime setting: `gateway.capability_routing_mode`; environment override:
  `CODEXMANAGER_CAPABILITY_ROUTING_MODE`; values: `off | observe | enforce`.
- RPC methods:
  - `aggregateApi/capabilities/get` with `{ id | apiId }`.
  - `aggregateApi/capabilities/setOverride` with `{ id | apiId,
    upstreamModelPattern?, protocol?, capabilityKey?, state }`.
  - `aggregateApi/capabilities/resetOverride` and
    `aggregateApi/capabilities/clearObservation` with the same scope fields.
  - `aggregateApi/capabilities/listRecentAttempts` with `{ id | apiId, limit? }`.
  - `aggregateApi/capabilities/setMode` with `{ mode }`.
- Migration `116_gateway_capability_routing.sql` owns
  `gateway_capability_overrides`, `gateway_capability_observations`, and
  `gateway_upstream_attempt_events`.

### 3. Contracts

- Resolution precedence is `operator override > unexpired runtime observation >
  unexpired probe observation > builtin > unknown`; scope specificity wins
  before recency inside one source layer.
- High-confidence runtime negative observations expire after seven days;
  positive runtime/probe observations expire after 24 hours. Generic errors do
  not create capability facts.
- Every candidate plan and retry body is rebuilt from the immutable original
  request. Candidate-local transforms must never leak into another candidate.
- Native candidates run before safe-downgrade candidates; existing ordering is
  stable inside each phase. Incompatible candidates are skipped.
- `tools` with `tool_choice = auto` is optional. An explicitly selected tool or
  a validated private required-capability header is required. The initial
  allowlisted transform only removes an optional `image_generation` tool and
  emits `drop_optional_image_generation`.
- Capability rejection is health-neutral. It may consume one capability retry
  for the same candidate, independently of transport and reasoning-guard retry
  budgets, only before client delivery starts.
- `off` preserves legacy routing and emits no capability attempt events;
  `observe` records projected plans without reordering or rewriting;
  `enforce` applies the plan and is the default.
- The private required-capabilities header is consumed by Manager, accepts only
  known capability keys, and must never be forwarded upstream.
- Attempt events persist only structural signatures, bounded codes, timing, and
  routing metadata. Never persist prompt/body values, secrets, tool arguments,
  encrypted reasoning content, or image bytes.
- RPC implementation, Tauri command registry, Web command map, typed frontend
  wrapper, and UI must remain synchronized whenever a command changes.

### 4. Validation & Error Matrix

- Empty Aggregate API id -> `aggregate api id required`.
- Unknown Aggregate API id -> `aggregate api not found`.
- Unknown capability key in management scope or private header -> 400 /
  `unsupported capability key`.
- Override state outside `auto | supported | unsupported` -> validation error;
  `auto` deletes the override.
- Routing mode outside `off | observe | enforce` -> validation error; an invalid
  environment value falls back to `enforce`.
- Structured `permission_error` plus the exact image group-entitlement message
  -> `capability.image_generation_not_enabled`, negative observation, no source
  cooldown, and at most one safe same-candidate downgrade retry.
- Generic HTTP 502 or vague upstream text -> ordinary upstream failure and no
  learned capability fact.
- Hosted-image diagnostic HTTP 400/422 without positive semantic evidence ->
  `unknown`; it must not persist a supported observation.
- Capability rejection after delivery begins -> no retry or failover.

### 5. Good / Base / Bad Cases

- Good: Grok rejects optional image generation before delivery; Manager learns
  the scoped negative fact, retries once without that tool, returns the answer,
  and keeps one final `request_logs` row.
- Good: an operator `supported` override beats a stale runtime negative fact.
- Base: an unknown candidate remains admissible and receives the original body.
- Bad: treating every 400/422 probe or every 502 response as capability evidence.
- Bad: mutating a shared body and accidentally removing tools for later
  candidates, or incrementing source cooldown for a feature entitlement error.

### 6. Tests Required

- Core storage: override CRUD, observation coalescing/TTL/clear/prune, attempt
  retention, and migration initialization.
- Capability unit tests: scope precedence, intent requiredness, exact classifier
  positives and near-match negatives, mode semantics, immutable transform, and
  structural-signature redaction.
- Gateway integration: first request contains optional image generation, exact
  entitlement rejection is health-neutral, the same supplier is retried once
  from the original body without the tool, both attempt phases are recorded,
  and only one final request log exists.
- Header tests: known declarations promote optional intent; unknown declarations
  return 400; the private header is absent from upstream headers.
- Probe tests: generic hosted-image 400/422 remains unknown and writes no
  positive observation.
- Cross-layer tests: RPC validation plus Tauri/Web/frontend command-name and
  payload normalization coverage; run the frontend production build.

### 7. Wrong vs Correct

#### Wrong

```rust
let effective_body = drop_image_tool(&mut shared_body);
mark_source_failure(api_id, response.status());
```

#### Correct

```rust
let plan = plan_candidate(&immutable_original_body, candidate, facts, mode)?;
if rejection.is_exact_capability_failure() && !delivery_started {
    persist_health_neutral_observation(&rejection)?;
    retry_once(plan_from_original_with_safe_transform)?;
}
```

## Scenario: Automatic Context Compaction Advertisement

### Scope / Trigger

- Applies when changing `auto_compact_token_limit`, `/v1/models`, `/v1/responses/compact`, or the `autoCompactEnabled` app setting.

### Contract

- `autoCompactEnabled` defaults to `false` and is persisted as `gateway.auto_compact_enabled`.
- `CODEXMANAGER_AUTO_COMPACT_ENABLED` is the runtime environment override.
- Disabling automatic compact removes `auto_compact_token_limit` only from the local `/v1/models` response projection. It must not delete or overwrite the stored model-catalog value.
- Enabling the setting restores the stored threshold in subsequent model-list responses.
- The switch controls Codex client auto-triggering only. Explicit `/v1/responses/compact` requests remain available, and normal `/v1/responses` requests must not depend on compact success.
- Do not add gateway-side replacement-history rewriting without a separate protocol design, response validation, timeout, and fail-open rollback path.

### Tests Required

- Runtime default and setter round-trip.
- App-settings persisted round-trip.
- Disabled model-list projection hides the threshold without mutating its source.
- Enabled model-list projection preserves the threshold.
- Frontend type-check and production build after changing the settings payload.

## Scenario: Aggregate API Runtime Cooldown Status and Reset

### 1. Scope / Trigger

- Trigger: adding an Aggregate API runtime health/cooldown read or reset capability that spans gateway memory, service RPC, Tauri commands, Web command mapping, and typed frontend clients.
- Runtime cooldown is process-local state; do not infer or persist it through Aggregate API configuration storage.

### 2. Signatures

- RPC methods: `aggregateApi/runtimeStatus/list` and `aggregateApi/runtimeStatus/reset`.
- Reset params: `{ id: string }`.
- Tauri commands: `service_aggregate_api_runtime_status_list(addr?: string)` and `service_aggregate_api_runtime_status_reset(addr?: string, id: string)`.
- Frontend wrappers: `accountClient.listAggregateApiRuntimeStatuses()` and `accountClient.resetAggregateApiRuntimeStatus(apiId)`.

### 3. Contracts

- A status item uses camelCase fields: `aggregateApiId`, `isCoolingDown`, `consecutiveFailures`, `failureThreshold`, `cooldownUntil`, `remainingSecs`, `lastFailureAt`, and `reason`.
- Listing returns only in-memory cooldown entries. Clients merge them with the persistent Aggregate API list and treat an absent entry as routable.
- A reset clears both the cooldown entry and the matching system policy action so routing selection and request-log evidence cannot disagree.
- Status reasons must be stable, sanitized classifications; never return raw upstream responses, API secrets, or authentication parameters.

### 4. Validation & Error Matrix

- Empty reset id -> `aggregate api id required`.
- Unknown reset id -> `aggregate api not found`.
- Existing API with no runtime entry -> successful reset with `isCoolingDown = false`.
- Expired entries -> remove before list/status projection; remaining seconds must never be negative.

### 5. Good / Base / Bad Cases

- Good: a cooling API reports the current failure count and a countdown, and reset lets it re-enter routing immediately.
- Base: a healthy API has no list item and the UI displays the normal routable state without a reset action.
- Bad: clearing only the cooldown map but leaving the policy action, which produces stale request-log route evidence.

### 6. Tests Required

- Cooldown unit test: threshold, snapshot countdown, expiration cleanup, and reset clearing both cooldown and policy action.
- RPC test: list projects the active runtime item; reset succeeds for an existing API and rejects an unknown API.
- Frontend build/type check and runtime transport coverage after changing command names or payload normalization.

### 7. Wrong vs Correct

#### Wrong

```rust
state.entries.remove(api_id);
```

#### Correct

```rust
state.entries.remove(api_id);
clear_system_policy_action(PolicyTargetKind::AggregateApi, api_id);
```

## Scenario: Per-Aggregate-API Consecutive Failure Freeze Policy

### 1. Scope / Trigger

- Trigger: adding a per-source setting that controls the existing five-failure Aggregate API freeze across SQLite, gateway health state, service RPC, Tauri commands, and typed frontend clients.
- Applies to `crates/core/src/storage/aggregate_apis.rs`, `crates/service/src/aggregate_api_health.rs`, `crates/service/src/gateway/mod.rs`, `apps/src-tauri/src/commands/aggregate_api.rs`, and Aggregate API UI wrappers.

### 2. Signatures

- DB column: `aggregate_apis.enable_consecutive_failure_freeze INTEGER NOT NULL DEFAULT 1`.
- RPC methods: `aggregateApi/list`, `aggregateApi/create`, and `aggregateApi/update`; field is `enableConsecutiveFailureFreeze: boolean` in camelCase.
- Tauri create/update commands must accept and forward `enable_consecutive_failure_freeze: Option<bool>`.
- Frontend model: `AggregateApi.enableConsecutiveFailureFreeze: boolean`, normalized with default `true`.

### 3. Contracts

- Missing/legacy values resolve to `true`; existing APIs retain current behavior.
- `false` suppresses only generic consecutive-failure cooldown: memory cooldown entries are not recorded/checked, and persisted generic cooldown states do not block routing.
- Persisted `auth`, `model_not_supported`, and `rate_limited` cooldowns remain route-blocking when proactive health monitoring is enabled; `unhealthy` remains blocking.
- Health observations and failure counters remain recorded when freeze is disabled; disabling the switch clears the API's in-memory cooldown entries but does not erase health history.
- The aggregate API page may expose the setting in a table switch and edit modal; the unused health-monitoring column must not issue probe-cost or active-probe configuration queries.

### 4. Validation & Error Matrix

- Missing switch on create/list -> default `true`.
- Unknown API on update -> existing `aggregate api not found` behavior.
- Storage read error while deciding a freeze -> fail closed to the existing behavior (`true`); do not silently disable protection.
- Generic five failures + switch false -> no route freeze.
- Auth/model/rate-limit failure + switch false -> classified cooldown remains effective.

### 5. Good / Base / Bad Cases

- Good: storage, service list, Tauri/Web RPC, modal, and table switch all round-trip the same boolean.
- Base: an old database row receives default `1` and routes exactly as before.
- Bad: gateway returns early when the switch is false and thereby skips persisted auth/rate-limit health blocking.
- Bad: updating only the frontend payload while omitting the Tauri command or service RPC parameter.

### 6. Tests Required

- Storage round-trip: insert false, read/list false, update true, and missing id returns `None`.
- Health regression: generic threshold is disabled by false; classified cooldown remains enabled; persisted generic cooldown is ignored after disabling.
- Gateway regression: memory cooldown is gated by the switch while persisted health checks still run.
- Service list contract: `AggregateApiSummary` returns the field.
- Frontend runtime/build: normalization defaults true; table source has no health-monitoring column or probe-cost/config query; static build passes.

### 7. Wrong vs Correct

#### Wrong

```rust
if !freeze_enabled {
    return false; // skips both memory and persisted classified cooldowns
}
```

#### Correct

```rust
let memory_cooldown = freeze_enabled && is_aggregate_api_in_cooldown(api_id, model);
memory_cooldown || is_routing_blocked_with_storage(storage, api_id, model)
```

## Scenario: Gateway integration fixtures after Model Catalog V2

### 1. Scope / Trigger

- Trigger: changing a Gateway integration test that routes a model through an
  account pool or Aggregate API.
- Applies to `crates/service/tests/gateway_logs/**` and any test fixture that
  combines legacy source mappings with runtime request execution.

### 2. Signatures

- V2 account route: `ModelRouteV2 { source_kind: "account_pool", source_id:
  "default", upstream_model, .. }`.
- V2 Aggregate API route: `ModelRouteV2 { source_kind: "aggregate_api",
  source_id: aggregate_api_id, upstream_model, .. }`.
- Test helpers: `seed_model_catalog_models` and `seed_model_catalog_route`.

### 3. Contracts

- The Gateway resolves runtime candidates and upstream model overrides only
  from Model Catalog V2.
- `openai_account` remains a legacy storage/migration representation and is
  not a valid `model_routes.source_kind`; account-pool runtime routes always
  use `account_pool/default`.
- Legacy `ModelSourceMapping` records may remain in a fixture only when the
  assertion explicitly covers compatibility. They must not be the sole route
  required for a mocked upstream request to occur.

### 4. Validation & Error Matrix

- A route with `source_kind = "openai_account"` -> reject as an invalid V2
  model route.
- A fixture with only a legacy mapping -> the test does not represent the
  production V2 runtime and must seed the matching V2 route.
- An Aggregate API fixture without a V2 route for its id -> candidate is
  unavailable, even if a legacy mapping exists.

### 5. Good / Base / Bad Cases

- Good: an account fixture seeds `account_pool/default` and separately keeps a
  legacy mapping to assert that the client model is not overridden.
- Base: a test only exercises migration compatibility and never invokes the
  Gateway; it may use legacy records without a V2 route.
- Bad: adding an `openai_account` V2 route merely to make an integration test
  pass.

### 6. Tests Required

- Gateway tests that send a request must assert the mocked upstream receives
  it after seeding the appropriate V2 route.
- Aggregate API tests must seed each configured candidate id in the V2 model
  catalog when failover order is part of the assertion.
- Keep at least one compatibility test proving legacy mappings do not control
  the Model Catalog V2 runtime path.

### 7. Wrong vs Correct

#### Wrong

```rust
seed_model_catalog_route(storage, model, "openai_account", account_id, upstream, 0);
```

#### Correct

```rust
seed_model_catalog_models(storage, &[model]); // seeds account_pool/default
// Add ModelSourceMapping separately only for legacy compatibility assertions.
```
