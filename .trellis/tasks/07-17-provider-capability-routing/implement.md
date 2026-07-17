# Implementation Plan

## Phase 0: Contract tests and scaffolding

- [ ] Add failing unit tests for capability scope, precedence, TTL and intent.
- [ ] Add failing request-transform tests for optional/required image tools and
      stateless provider-state cleanup.
- [ ] Create focused gateway capability module skeleton and typed contracts.

## Phase 1: Storage foundation

- [x] Add migration `116_gateway_capability_routing.sql` for overrides,
      observations and upstream attempt events.
- [ ] Add focused core storage modules and public record/query types.
- [ ] Implement override CRUD, observation upsert/coalescing, effective-scope
      queries, expiry pruning and attempt-event retention pruning.
- [ ] Add legacy-database migration and storage regression tests.

## Phase 2: Resolver and immutable planner

- [ ] Implement capability keys, scope matching and precedence resolver.
- [ ] Implement request intent extraction without prompt inspection.
- [ ] Implement redacted structural signature.
- [ ] Implement transform registry with explicit safety predicates.
- [ ] Implement native/downgrade/incompatible candidate partitioning while
      preserving existing order inside each phase.
- [ ] Add unit and property-style tests proving immutable body isolation.

## Phase 3: Failure classification and retry integration

- [ ] Add structured HTTP/SSE error classification and exact image entitlement
      classifier.
- [ ] Add typed failure disposition separating capability from source health.
- [ ] Integrate plan generation into the Aggregate API attempt flow through
      narrow helpers.
- [ ] Add one bounded same-candidate capability retry from the original body.
- [ ] Guard against retry/failover after response delivery starts.
- [ ] Keep capability retry independent from transport and reasoning-guard
      budgets.
- [ ] Add full streaming/non-streaming/failover gateway tests.

## Phase 4: Observability and runtime mode

- [ ] Add bounded async attempt-event recorder with synchronous test behavior and
      synchronous fallback on queue pressure.
- [ ] Add sanitized route evidence and bounded metrics.
- [ ] Add `capabilityRoutingMode` runtime/app setting and
      `CODEXMANAGER_CAPABILITY_ROUTING_MODE` override.
- [ ] Implement and test `off`, `observe`, and `enforce` modes.
- [ ] Verify final `request_logs` cardinality remains one row per client request.

## Phase 5: Diagnostics, RPC and management UI

- [ ] Feed eligible live diagnostic results into capability observations.
- [ ] Add capability get/override/reset/clear/recent-attempt RPC methods.
- [ ] Synchronize service dispatch, Tauri registry, Web command mapping and typed
      frontend wrapper.
- [ ] Add TypeScript normalization/types with backward-compatible defaults.
- [ ] Add a focused Capability panel/hook to the existing Aggregate API page.
- [ ] Add i18n strings and UI/runtime transport tests.

## Phase 6: Verification and rollout review

- [ ] Run focused core storage and migration tests.
- [ ] Run focused service capability, Aggregate API and gateway tests.
- [ ] Run full workspace tests because retry/cooldown behavior is shared.
- [ ] Run frontend runtime tests and desktop static build.
- [ ] Run dependency/security checks applicable to changed Rust and frontend
      packages.
- [ ] Review diffs for secrets, raw request/response persistence, unbounded metric
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
