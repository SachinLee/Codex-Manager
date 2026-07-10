# Design

## Scope

This task implements only P1-P3 from the fusion plan:

- P1: upstream capability diagnostics for Aggregate API / gateway targets.
- P2: route evidence and system-owned cooldown policy actions.
- P3: semantic validation for hosted image generation responses.

Out of scope:

- model catalog context-window editing,
- Codex App marketplace repair,
- CDP injection,
- fleet / multi-node observability,
- Stepwise, Zed, worktree, WeChat bridge.

## Existing Boundaries

- `crates/service/src/gateway/` owns gateway request normalization, routing, upstream proxying, observability, and response bridging.
- `crates/service/src/aggregate_api.rs` owns Aggregate API CRUD, connectivity tests, balance refresh, and supplier model metadata.
- `crates/service/src/requestlog/` owns request log query projections.
- `apps/src/lib/api/` owns typed frontend API wrappers.
- `apps/src/app/aggregate-api/` and shared modals own Aggregate API management UI.
- `crates/core/` owns SQLite schema and storage primitives. Use it only if persistence is required.

## P1 Capability Diagnostics

Add a diagnostic model that is separate from normal request routing:

```text
UI / RPC request
  -> aggregate API diagnostic handler
  -> bounded probe executor
  -> diagnostic result projection
  -> optional persisted latest summary
  -> UI detail modal
```

Diagnostic status values:

- `supported`
- `unsupported`
- `unknown`
- `not_tested`

Probe cases:

- `models`: `GET /models` or `/v1/models` depending on configured base URL conventions.
- `responses`: minimal bounded `POST /responses` or `/v1/responses`.
- `responsesCompact`: bounded `POST /responses/compact`; validation errors can still prove endpoint existence.
- `responsesWebSocket`: optional, off by default unless explicitly requested.
- `hostedImageGeneration`: optional live probe, must be explicitly marked because it may consume quota.

Default diagnostic should prefer low-cost, non-mutating checks. Live image/WebSocket smoke should be opt-in.

## P2 Route Evidence / Policy Action

Introduce shared domain structs close to routing code:

- `RouteEvidence`
  - `kind`
  - `source`
  - `targetKind`
  - `targetId`
  - `confidence`
  - `reason`
  - `statusCode`
  - `retryAfterSecs`
  - `observedAt`

- `GatewayPolicyAction`
  - `id`
  - `owner = system`
  - `kind = cooldown`
  - `targetKind`
  - `targetId`
  - `sourceEvidence`
  - `reason`
  - `createdAt`
  - `expiresAt`

First implementation can be in memory if persistence is too broad, but the API shape should allow later persistence.

Existing cooldown and route quality code should not be replaced all at once. Instead:

1. Keep current behavior.
2. Add evidence projection at the point where current code already decides skip/cooldown.
3. Add policy action summaries as a typed read model.
4. Extend request log projections to include evidence summaries without changing old fields.

Manual user actions have priority over system policy actions. System policy actions must not enable/disable accounts or mutate third-party provider config.

## P3 Semantic Validation

Add a semantic validation step for hosted image generation results:

```text
upstream status success
  -> semantic contract selected by request type
  -> validate response body
  -> pass response OR synthesize gateway semantic failure
  -> request log includes semantic failure class
```

Initial contract:

- `HostedImageGeneration`
  - response JSON must contain at least one output item with:
    - `type = "image_generation_call"`
    - non-empty `result`

If validation fails:

- return a clear gateway error body,
- mark failure as retryable when safe,
- include `error_class = image_generation_missing_result`,
- do not log or persist raw image payloads.

## Cross-Layer Contracts

All new RPC/API payloads use camelCase for frontend-facing fields.

Backend owns normalization and validation. UI must consume typed wrappers rather than reading raw JSON fields.

Request-log extensions must be additive to preserve existing UI/tests.

## Risks

- Capability probes may be mistaken for real requests by upstreams. Keep default probes bounded and conservative.
- System cooldown can hide healthy resources if evidence confidence is too broad. Start with high-confidence events only.
- Semantic validation can reject non-standard but usable upstream responses. Limit P3 validation to known hosted image generation flows.
