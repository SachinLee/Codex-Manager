# 合并 codex-retry-gateway 优先优化实施计划

## Pre-check

1. Snapshot dirty state and avoid reverting unrelated changes.
2. Read the relevant existing tests before editing:
   - `crates/service/tests/gateway_logs/usage_limit_failover.rs`
   - `crates/service/src/gateway/observability/http_bridge/reasoning_guard.rs`
   - `crates/service/src/gateway/observability/http_bridge/delivery.rs`
   - `crates/service/src/app_settings/*`
   - `apps/tests/settings-page-helpers.test.mjs`
   - `apps/tests/gateway-settings.test.mjs`

## Implementation Steps

1. Runtime config
   - Add default reasoning guard targets `[516, 1034, 1552]`.
   - Add runtime getters/setters for targets, stream intercept, non-stream intercept, retry attempts.
   - Add env/app-settings sync defaults for missing fields.

2. App settings backend
   - Add app setting keys in `shared.rs`.
   - Add current snapshot fields in `api/current.rs`.
   - Add patch fields and validation in `api/patch.rs`.
   - Add helper setters/current getters in `app_settings/gateway.rs`.
   - Update runtime sync.

3. Reasoning guard decision
   - Replace fixed `decide_for_516` naming with target-aware decision.
   - Preserve consecutive bypass behavior.
   - Add tests for matching `516`, `1034`, `1552`, non-matching token, disabled, observe-only, and invalid config normalization.

4. Internal retry wiring
   - Locate upstream attempt loop and bridge result handling.
   - Add retry budget per incoming request.
   - On reasoning guard internal retry, rerun upstream attempt without marking account unavailable or supplier failure.
   - Ensure retry attempts are counted and logged.

5. Metrics/logging
   - Add counters and Prometheus output.
   - Record match/block/internal retry with stream vs non-stream classification.
   - Update metrics tests.

6. Capacity retry wiring
   - Add a narrow helper for `Selected model is at capacity. Please try a different model.` matching.
   - Preserve bridge pending requests for matching upstream errors before writing them to the client.
   - Return `RetrySameCandidateReason::UpstreamCapacity` from response finalization and retry the same candidate once.
   - Record `codexmanager_gateway_upstream_capacity_internal_retries_total`.
   - Add gateway regression coverage for same-candidate retry, no ordinary failover, and metric increment.

7. Passive model consistency foundation
   - Add low-risk extraction/logging of request/effective/upstream/stream model signals where already available.
   - Avoid persistence/schema unless clearly required by existing request log shape.

8. Frontend settings
   - Extend `AppSettings` type, zustand defaults, normalize defaults.
   - Update settings page state/drafts for targets and retry attempts.
   - Update `GatewayTabContent` UI labels and controls.
   - Update frontend tests for normalize/defaults and settings page helpers.

9. Validation
   - Run focused Rust tests first:
     - `cargo test -p codexmanager-service reasoning_guard`
     - `cargo test -p codexmanager-service gateway_reasoning_guard`
     - `cargo test -p codexmanager-service --test gateway_logs capacity`
     - `cargo test -p codexmanager-service gateway_metrics`
   - Run focused app runtime tests:
     - `pnpm -C apps run test:runtime -- gateway-settings`
     - `pnpm -C apps run test:runtime -- settings-page-helpers`
   - If focused test commands are not supported by scripts, run the nearest available command and record limitations.

## Review Gates

- Before touching frontend UI, backend API shape must be stable.
- Before final response, inspect `git diff` and ensure no unrelated dirty files were reverted or staged.
- If internal retry wiring requires large pipeline refactor, pause and report a narrowed phase split instead of forcing a risky implementation.
