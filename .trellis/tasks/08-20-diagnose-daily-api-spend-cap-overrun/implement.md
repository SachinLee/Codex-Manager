# Implementation Plan: Aggregate API daily spend enforcement

## Profile and Scope

- Profile: critical. This changes spend controls, SQLite state, and a public RPC/UI contract.
- Task artifacts: `prd.md`, `design.md`, and `research/daily-spend-cap-evidence.md` are mandatory context.
- No new external dependency is required. Reuse rusqlite transactions, the existing storage-pool retry model, current model price tiers, request charge snapshots, and Guard observability events.

### Slice 1: AC-003 - Add durable daily budget storage

- Behavior: Each Aggregate API/day can hold a legacy opening amount, settled cost, and active reservation amount; each upstream attempt has an idempotent persistent lifecycle record.
- Code boundary: New next-numbered SQL migration under `crates/core/migrations/`; migration registration in `crates/core/src/storage/mod.rs`; new `crates/core/src/storage/aggregate_api_daily_spend.rs`; exports and typed records in `crates/core/src/storage/mod.rs`.
- Test seam: Public `Storage` methods against both an in-memory new schema and a file-backed migrated fixture.
- RED: Add storage tests that fail before the migration for bucket/attempt creation, idempotent transitions, baseline persistence, and migration initialization.
- Implementation: Add additive bucket and attempt tables, constraints, indexes for `(aggregate_api_id, day_start_ts, state)`, and focused typed storage operations. Register the migration after the currently allocated migration sequence without renumbering unrelated in-flight migrations.
- GREEN: `cargo test -p codexmanager-core aggregate_api_daily_spend` and the relevant `cargo test -p codexmanager-core storage` target pass.
- Validation: Open a pre-migration fixture, run `Storage::init()`, and assert existing aggregate APIs, request logs, snapshots, and Guard events survive unchanged.
- Dependencies: None.
- Rollback: The migration is additive; the previous binary ignores the new tables.

### Slice 2: AC-001 and AC-002 - Atomically reserve and reconcile priceable attempts

- Behavior: A bounded quote reserves budget before upstream dispatch; settled, released, and held attempts update the same bucket exactly once. A second concurrent reservation cannot reuse the same remaining balance.
- Code boundary: The storage module from Slice 1; exact micro-USD quote helper near `crates/service/src/quota/model_pricing.rs`; reuse `ModelPriceTierV2`, `compute_charge_v2`, provider tick/nano conversion, and the Aggregate API multiplier.
- Test seam: Storage operations invoked from two independently opened file-backed `Storage` connections and service quote unit tests.
- RED: Add tests for the observed `97.421556 + 3.350178 = 100.771734` baseline, a bounded quote that is rejected after another reservation consumes the remainder, idempotent settle/release calls, stale reservations becoming held, provider-cost precedence, cache normalization, multiplier application, missing price, and unbounded output.
- Implementation: Use `TransactionBehavior::Immediate` to lazily create a bucket with page-equivalent opening spend, transition attempts atomically, and return a typed grant/reject result. Quote transformed Responses, Chat, and Anthropic payloads; recognize `max_output_tokens`, `max_completion_tokens`, `max_tokens`, and Chat `n`. Under the selected policy, reserve known input only for unbounded output and zero for an unpriced model while returning explicit quote state.
- GREEN: Targeted core and service quote tests pass, including a two-connection race test in which at most one near-limit reservation is granted.
- Validation: Verify all persisted amounts are integer micro-USD, non-negative, and that the sum of bucket settled/reserved values equals the sum of non-released attempts plus opening spend.
- Dependencies: Slice 1.
- Rollback: Disable the reservation caller; records remain audit-only and do not mutate historical charge snapshots.

### Slice 3: AC-001 and AC-002 - Gate every Aggregate API upstream attempt

- Behavior: The gateway reserves immediately before every actual upstream dispatch and resolves the reservation on terminal success, Guard retry, capacity retry, failure, timeout, or candidate failover. Rejected candidates preserve existing fallback semantics.
- Code boundary: `crates/service/src/gateway/upstream/protocol/aggregate_api.rs`; `crates/service/src/gateway/observability/request_log.rs`; any focused gateway spend helper introduced by Slice 2; existing protocol integration tests in `crates/service/src/gateway/upstream/protocol/aggregate_api_tests.rs`.
- Test seam: Mock Aggregate API upstream through the public Responses/Chat/Anthropic gateway paths, then inspect the Storage budget records and the final HTTP status.
- RED: Add integration tests proving that: a Guard-triggered first response settles before its retry; a daily limit rejects the next candidate after the retry cost consumes remaining budget; capacity/transport retry reservations release when no billable evidence exists; a known billable failure settles; final 2xx and 499 charge snapshots reconcile their reservation; and all candidates exhausted by budget return the existing 429 behavior.
- Implementation: Remove the non-atomic `aggregate_api_has_daily_budget` prefilter. Create attempt IDs only after local conversion/auth preparation succeeds. Pass a reservation ID through `RequestLogTraceContext` so final request-log charging settles from `ChargeSnapshotV2.charged_cost_microusd`. Make the Guard event use the same quoted/settled amount and candidate multiplier. Hold, rather than silently release, ambiguous dispatched requests after deadline/grace.
- GREEN: Targeted Aggregate API protocol tests and `cargo test -p codexmanager-service gateway` pass.
- Validation: Confirm model fallback, protocol adapters, streaming delivery, no-secret logging, cooldown handling, and zero-balance routing retain their current behavior when no daily limit is set.
- Dependencies: Slices 1 and 2.
- Rollback: Revert only the reservation gate call site; existing raw snapshots and observability rows remain valid.

### Slice 4: AC-001 and AC-003 - Align reporting and user-facing limit semantics

- Behavior: The Aggregate API page reports the same settled daily amount used by enforcement, exposes active reservations/held amounts and remaining budget, and states the compatibility-first overrun behavior.
- Code boundary: `crates/core/src/rpc/types.rs`; `crates/core/src/storage/request_token_stats.rs` or a focused daily-spend summary reader; `crates/service/src/requestlog/requestlog_aggregate_api_daily_usage.rs`; `apps/src/types/api-key.ts`; `apps/src/lib/api/normalize.ts`; `apps/src/app/aggregate-api/page.tsx`; `apps/src/components/modals/aggregate-api-modal.tsx`; relevant i18n files.
- Test seam: `requestlog/aggregate_api_daily_usage` RPC output and the aggregate-page render/normalization tests.
- RED: Add a core/service test that a bucket overrides only cost fields after activation while raw token counts remain unchanged; add frontend tests for absent optional budget fields and for settled/reserved/over-limit tooltip rendering.
- Implementation: Add optional budget fields to the existing daily-usage response. When a bucket exists, use `budgetSpentUsd` as the cost shown in the per-API row and header total; otherwise preserve the legacy response. Update the input helper text and usage tooltip to distinguish settled, reserved, held, remaining, and potential unbounded overrun. Keep all new fields additive for desktop and web transport compatibility.
- GREEN: `pnpm -C apps test -- <targeted aggregate API test>` and the corresponding service RPC tests pass.
- Validation: Run `pnpm -C apps run build` and verify both desktop and web transport return the same optional fields.
- Dependencies: Slices 1 through 3.
- Rollback: Older clients ignore the additive response fields; newer clients fall back to legacy display if the fields are absent.

### Slice 5: AC-004 - Complete critical-path verification and rollout evidence

- Behavior: The merged slices prove the reproduced bug is prevented for priceable retries and disclose the compatibility exception for unbounded/provider-repriced requests.
- Code boundary: Focused core/service/frontend regression tests, docs, and no unrelated routing changes.
- Test seam: Storage migration, concurrent reservation test, gateway mock flow, RPC output, and frontend render.
- RED: Preserve the original mismatch assertion as a failing regression: finalized 97.421556 USD plus a 3.350178 USD Guard retry must not leave a priceable follow-up attempt eligible under a 98 USD limit.
- Implementation: Add docs/changelog only after the behavior is verified. Document that the limit is compatibility-first, tracks all locally priceable attempts, and may be exceeded by unbounded output or a higher provider-reported final cost.
- GREEN: Run `cargo test --workspace`, `pnpm -C apps run build`, and the targeted runtime/UI tests. Run the migration fixture and concurrency regression again after any review fix.
- Validation: Use an isolated file database to verify restart recovery turns abandoned reservations into held state, starts a new local-day bucket cleanly, and never exposes secrets in structured spend logs.
- Dependencies: Slices 1 through 4.
- Rollback: Deploy the preceding build; do not delete the additive budget audit data.

## Expected Files

- `crates/core/migrations/<next>_aggregate_api_daily_spend.sql`
- `crates/core/src/storage/mod.rs`
- `crates/core/src/storage/aggregate_api_daily_spend.rs`
- `crates/core/src/storage/tests/...` or module-local storage tests
- `crates/service/src/quota/model_pricing.rs`
- `crates/service/src/gateway/upstream/protocol/aggregate_api.rs`
- `crates/service/src/gateway/observability/request_log.rs`
- `crates/service/src/requestlog/requestlog_aggregate_api_daily_usage.rs`
- `crates/service/src/...tests...`
- `crates/core/src/rpc/types.rs`
- `apps/src/types/api-key.ts`
- `apps/src/lib/api/normalize.ts`
- `apps/src/app/aggregate-api/page.tsx`
- `apps/src/components/modals/aggregate-api-modal.tsx`
- relevant `apps/tests/` and localized Aggregate API messages

## Verification Gate

Before moving this task to implementation, ensure:

- The active migration number is allocated without colliding with concurrent work.
- RED tests identify the current accounting mismatch and concurrent stale-balance failure.
- The reservation design is reviewed independently for data integrity, crash recovery, provider-cost reconciliation, and interaction with `request_charge_snapshots`.
- The plan remains limited to Aggregate API daily spend enforcement; no unrelated gateway or model-catalog refactor is bundled.
