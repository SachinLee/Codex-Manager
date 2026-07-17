# Implementation Plan

## Phase 0: Contract tests and scaffolding

- [x] Add unit tests for capability scope, precedence, TTL and intent.
- [x] Add request-transform tests for optional/required image tools.
- [ ] Add safe stateless provider-state cleanup and its replay-completeness tests.
- [x] Create focused gateway capability modules and typed contracts.

## Phase 1: Storage foundation

- [x] Add migration `116_gateway_capability_routing.sql` for overrides,
      observations and upstream attempt events.
- [x] Add focused core storage modules and public record/query types.
- [x] Implement override CRUD, observation upsert/coalescing, effective-scope
      queries, expiry pruning and attempt-event retention pruning.
- [x] Add additive migration and focused storage regression tests.

## Phase 2: Resolver and immutable planner

- [x] Implement capability keys, scope matching and precedence resolver.
- [x] Implement request intent extraction without prompt inspection.
- [x] Implement redacted structural signature.
- [x] Implement the optional image transform with explicit safety predicates.
- [ ] Implement provider-state cleanup transforms only after a trustworthy
      complete-stateless-replay signal is designed.
- [x] Implement native/downgrade/incompatible candidate partitioning while
      preserving existing order inside each phase.
- [x] Add unit tests proving immutable body isolation.

## Phase 3: Failure classification and retry integration

- [x] Add structured HTTP/SSE error classification and exact image entitlement
      classifier.
- [x] Separate capability failure handling from source health/cooldown effects.
- [x] Integrate plan generation into the Aggregate API attempt flow through
      narrow helpers.
- [x] Add one bounded same-candidate capability retry from the original body.
- [x] Guard against retry/failover after response delivery starts.
- [x] Keep capability retry independent from transport and reasoning-guard
      budgets.
- [ ] Add a dedicated streaming rejection regression test; non-streaming
      same-candidate downgrade and failover paths are covered.

## Phase 4: Observability and runtime mode

- [x] Add bounded async attempt-event recorder with synchronous test behavior and
      synchronous fallback on queue pressure.
- [x] Add sanitized structural route evidence.
- [ ] Add capability-specific bounded metrics.
- [x] Add `capabilityRoutingMode` runtime/app setting and
      `CODEXMANAGER_CAPABILITY_ROUTING_MODE` override.
- [x] Implement and test `off`, `observe`, and `enforce` modes.
- [x] Verify final `request_logs` cardinality remains one row per client request.

## Phase 5: Diagnostics, RPC and management UI

- [x] Feed eligible live diagnostic results into capability observations.
- [x] Add capability get/override/reset/clear/recent-attempt/mode RPC methods.
- [x] Synchronize service dispatch, Tauri registry, Web command mapping and typed
      frontend wrapper.
- [x] Add TypeScript normalization/types with backward-compatible defaults.
- [x] Add a focused Capability panel/hook to the existing Aggregate API page.
- [x] Add focused frontend normalization/transport tests.
- [x] Synchronize capability panel copy across English, Korean, and Russian
      message resources.

## Phase 6: Verification and rollout review

- [x] Run focused core storage tests.
- [x] Run focused service capability and Aggregate API gateway tests.
- [ ] Run full workspace tests because retry/cooldown behavior is shared.
- [x] Run frontend production build and Tauri Rust check.
- [ ] Obtain a clean full frontend runtime suite; capability tests pass, while
      nine unrelated dirty-worktree tests currently fail.
- [ ] Run dependency/security checks applicable to changed Rust and frontend
      packages.
- [x] Review scoped diffs for secrets, raw request/response persistence,
      provider-specific branches and legacy-file expansion.
- [ ] Review capability metrics for unbounded labels after metrics are added.
- [ ] Review remaining workspace diffs for unbounded metric
      labels, provider-specific branches and legacy-file expansion.
- [ ] Validate `observe` telemetry and `off` rollback behavior with a controlled
      GPT-to-Grok reproduction.

## Validation Commands

```powershell
cargo test -p codexmanager-core capability
cargo test -p codexmanager-core migration
cargo test -p codexmanager-service capability
cargo test -p codexmanager-service aggregate_api
cargo test -p codexmanager-service gateway
cargo test --workspace
pnpm -C apps run test:runtime
pnpm -C apps run build
pnpm -C apps run build:desktop
cargo audit
pnpm -C apps audit
```

If a narrow test filter does not exist yet, add focused module tests first and
then use the nearest package-level command.

## Review gates

- Storage review: additive migration, indexes, TTL and retention behavior.
- Gateway review: no retry after delivery, immutable retry body, health-neutral
  capability failures, reasoning-guard compatibility.
- Security/privacy review: no prompt/body/secret/tool-argument persistence.
- API parity review: Rust implementation, RPC dispatch, Tauri command, Web map,
  typed wrapper and UI all synchronized.
- Maintainability review: no provider-name branches and no substantial new logic
  in legacy proxy/page entrypoints.

## Rollback

- Runtime rollback: set capability routing mode to `observe` or `off`.
- UI/RPC rollback: additive APIs and tables can remain unused.
- Data rollback: do not drop fact/event tables during emergency rollback; they
  are backward-compatible and preserve operator evidence.
- Code rollback boundary: capability planner integration is isolated from the
  existing routing and transport implementation.
