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
