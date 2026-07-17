# Provider capability-aware gateway routing

## Goal

Prevent OpenAI Responses-compatible requests from failing during model or
supplier switching when an upstream exposes the endpoint but does not support
every request field or hosted tool. The gateway must adapt each attempt to the
selected supplier's effective capabilities without provider-name conditionals
and without silently weakening required user intent.

## Background

- A real Codex task switched from `gpt-5.6-sol` to `grok-4.5` and produced two
  consecutive 502 responses with the same session and conversation anchors as
  successful GPT turns.
- Each failed Grok request tried `input`, `esfaery-grok`, and
  `codexforme-grok`; the final supplier returned
  `permission_error: Image generation is not enabled for this group`.
- A controlled GPT-to-Grok stateless replay succeeded through `input` when the
  Grok request contained no `image_generation`, `previous_response_id`, or
  encrypted reasoning state.
- Existing P1-P3 work already provides upstream capability diagnostics, route
  evidence, cooldown policy actions, and hosted-image response validation. This
  task extends those mechanisms rather than introducing a parallel system.

## Requirements

### R1. Typed, layered capability contract

- Represent capabilities with typed, versionable keys, not provider, supplier,
  model, or URL conditionals.
- Capability state is `supported`, `unsupported`, or `unknown` and includes
  scope, provenance, confidence, observation time, and optional expiry.
- Scope is `(source_kind, source_id, upstream_model_pattern, protocol,
  capability_key)` and supports wildcard fallback; the most specific matching
  fact wins within a precedence layer.
- Effective resolution precedence is: operator override, recent runtime
  observation, recent live diagnostic, built-in profile, then `unknown`.
- Operator overrides are `auto`, `supported`, or `unsupported`; explicit values
  do not expire and reset to `auto` removes the override.
- Runtime observations are stored separately from Aggregate API configuration.
  High-confidence negative observations default to seven days; positive
  observations default to 24 hours.
- Repeated identical observations update count and timestamps rather than
  creating unbounded duplicate facts.

### R2. Machine-readable intent and immutable planning

- Derive required capabilities only from machine-readable protocol signals:
  endpoint semantics, an explicitly forced `tool_choice`, or a Manager-private
  required-capabilities header/metadata field removed before upstream delivery.
- Do not inspect prompt text to infer capability intent.
- A tool present in the catalog with `tool_choice=auto` is optional.
- Build an effective request plan independently for every candidate from one
  immutable normalized original body. Retries must not accumulate transforms.
- Adapt only transformations in a code-owned, tested allowlist with explicit
  safety preconditions. Operators cannot define arbitrary JSON rewrites.
- Never remove a required capability. Unsafe provider-state cleanup is treated
  as incompatible unless the request contains sufficient stateless replay data.

### R3. Risk-aware unknown policy

- Low-risk metadata with unknown support passes through.
- Optional, safely removable features may be retried once after a
  high-confidence rejection.
- Required unknown features may be attempted but never silently removed.
- High-risk provider-bound state is sanitized only when an allowlisted safety
  precondition proves the request remains semantically complete; otherwise the
  candidate is incompatible.

### R4. Two-phase routing and classified recovery

- First try candidates that preserve the native request capability set, then
  candidates requiring only approved safe downgrades.
- Preserve existing `sort`, ordered/balanced strategy, cooldown, quota, and cost
  ordering within each phase.
- Required capabilities never enter the downgrade phase. Optional capabilities
  prefer full support before downgrade-compatible suppliers.
- Evaluate and adapt each failover candidate independently.
- Classify capability failures separately from transport, authentication,
  capacity, rate-limit, content-policy, invalid-request, and malformed-response
  failures.
- One exact high-confidence capability rejection is enough to record a negative
  observation. Generic 400/403/502, timeout, or `upstream failed` is not.
- A high-confidence optional-capability rejection may trigger at most one
  same-candidate downgrade retry, within the existing request deadline.
- No retry or failover is allowed after any response bytes have been delivered
  to the client.
- Capability failures and retries must not increment global supplier cooldown;
  existing transport/auth/health behavior remains unchanged.

### R5. Safe rollout and operations

- Support `off`, `observe`, and `enforce` modes.
- Default to `enforce`, but only execute code-owned allowlisted transforms.
- The initial allowlist covers optional hosted `image_generation` removal and
  safe cross-provider cleanup of `previous_response_id` / encrypted reasoning
  state when stateless-replay preconditions hold.
- Switching to `observe` or `off` provides an immediate rollback without schema
  rollback.
- Existing Aggregate API rows remain valid and resolve missing facts as
  `unknown` under the agreed risk policy.
- Desktop, service, Web, and Docker modes share backend behavior.

### R6. Privacy-safe observability

- Keep `request_logs` as one final row per client request.
- Record intermediate attempts in a dedicated event model containing candidate,
  phase, structural contract signature, capability decisions, transform codes,
  classified error, duration, and outcome.
- Never store prompts, Authorization, secrets, tool arguments, file content,
  encrypted payload values, image bytes/base64, or arbitrary upstream bodies.
- Attempt events default to the same 14-day retention as request logs and follow
  `CODEXMANAGER_REQUEST_LOG_RETENTION_DAYS`.
- Capability observations follow their own TTL; operator overrides persist until
  reset.

### R7. Minimal complete management surface

- First delivery spans persistence, gateway, RPC/transport, and the existing
  Aggregate API management page.
- Show effective capability, resolved source, confidence, scope, expiry, recent
  incompatibility evidence, and current routing mode.
- Provide `auto/supported/unsupported` overrides and learned-observation reset.
- Do not add a top-level navigation entry, arbitrary transform editor, or bulk
  capability management in the first delivery.

## Constraints

- Keep gateway/protocol behavior under `crates/service/src/gateway/`; keep
  Aggregate API administration in its existing domain; use `crates/core/` for
  schema and storage primitives.
- New substantial logic must live in focused modules rather than expanding the
  legacy Aggregate API proxy loop or page component.
- Preserve Responses streaming/non-streaming, tool calls, billing, reasoning
  guard, retry, and cooldown behavior.
- RPC additions are additive, frontend fields are camelCase, desktop IPC uses
  typed wrappers, and Web fallback uses the existing transport stack.
- Metrics labels must remain bounded; supplier IDs and arbitrary capability
  strings must not become unbounded metric labels.

## Acceptance Criteria

- [ ] An optional `image_generation` catalog entry with `tool_choice=auto` can
      recover from the observed group permission error through one safe,
      observable downgrade retry.
- [ ] Forced image generation, image endpoint requests, or private required
      capability declarations are never silently downgraded.
- [ ] Two suppliers receive independently generated effective bodies and no
      transform from one attempt leaks into another.
- [ ] Native-compatible candidates are attempted before downgrade candidates,
      with existing route order preserved inside each phase.
- [ ] Exact capability failures update only their scoped observation and do not
      affect global supplier cooldown; generic errors do not create capability
      facts.
- [ ] SSE/stream tests prove that no retry occurs after client delivery begins.
- [ ] Final request logs remain one row per request while redacted per-attempt
      events explain every candidate and retry outcome.
- [ ] Facts survive restart, expire according to TTL, resolve by specificity and
      precedence, and can be overridden/reset from the existing Aggregate API UI.
- [ ] `off`, `observe`, and `enforce` modes behave as documented and can be
      changed without restarting or migrating the database.
- [ ] Existing databases and Aggregate API records work with no manual migration
      or capability configuration.
- [ ] Tests cover streaming and non-streaming Responses, optional and required
      tools, unknown-risk behavior, same-candidate retry, multi-supplier
      failover, precedence, TTL, redaction, RPC transport, and frontend build.
- [ ] No provider-name, supplier-name, model-name, or URL-specific rewrite branch
      is introduced.

## Out of Scope

- Replacing Codex CLI transcript serialization.
- Prompt analysis, content-policy evasion, or enabling upstream entitlements.
- User-defined JSON transformations.
- Broad billing/model-pricing redesign or a new capability-management section.
