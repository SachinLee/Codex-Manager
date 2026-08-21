# Outcome

## Delivery

- Status: complete
- Summary: Implemented the compatibility-first Aggregate API daily spend budget. The configured limit now uses a durable per-API/per-local-day ledger, atomically reserves every priceable upstream attempt before dispatch, reconciles Guard retries/capacity retries/failures into the same ledger, and aligns the Aggregate API usage page with enforcement. Requests without a priceable output bound remain eligible by design, with the UI explicitly disclosing that they may settle above the configured amount.

## Acceptance Criteria

| Criterion | Result | Evidence |
| --- | --- | --- |
| AC-001: one authoritative budget definition covering normal attempts, Guard retries, capacity retries, failures, and final successes | PASS | `crates/core/src/storage/aggregate_api_daily_spend.rs`; marker `attempt_kind` values in `crates/core/migrations/137_aggregate_api_daily_spend.sql`; gateway lifecycle in `crates/service/src/gateway/upstream/protocol/aggregate_api.rs` |
| AC-002: atomic reservation/reconciliation prevents concurrent admissions against a stale balance | PASS | `BEGIN IMMEDIATE` (vendored `unchecked_transaction`) in `aggregate_api_daily_spend.rs`; cross-connection test `cross_connection_reservation_does_not_double_spend` |
| AC-003: compatible persistence, migration, restart recovery, observability, rollback | PASS | additive migration `137_aggregate_api_daily_spend.sql`; bucket lazy-creation captures legacy opening spend; stale reservations roll to `held`; structured logs added |
| AC-004: ordered RED/GREEN slices with the 97.421556 + 3.350178 failure regression | PASS | implement.md slices; core tests cover bucket baseline/settle/release/hold; page and enforcement now use the same `budget_spent_usd` |

## Implementation

- Added `crates/core/migrations/137_aggregate_api_daily_spend.sql` with durable budget buckets and attempt records.
- Added `crates/core/src/storage/aggregate_api_daily_spend.rs`:
  - `reserve_aggregate_api_daily_spend` (atomic, bucket lazy-creation, opening legacy spend, reclaim-stale-to-held)
  - idempotent `settle_aggregate_api_daily_spend_attempt` / `release_aggregate_api_daily_spend_attempt` / `hold_aggregate_api_daily_spend_attempt`
  - summary readers for the page/RPC
- Extended aggregated daily usage projection (`request_token_stats.rs`) with optional budget fields.
- Added priceable quote and settled-usage helpers in `crates/service/src/quota/model_pricing.rs`, applying the Aggregate API multiplier once.
- Wired the gateway in `aggregate_api.rs`:
  - removed the old completed-snapshot prefilter
  - reserve immediately before each upstream dispatch, reject with 429 when the quote exceeds remaining budget
  - settle Guard rounds synchronously before retry, release no-billable failures, hold ambiguous/timeout/in-flight rounds
  - pass the attempt ID into request-log charging to settle from the authoritative charge snapshot
- Fixed Guard event cost to include the Aggregate API multiplier.
- Extended the RPC and frontend (types, normalize, page, modal, en/ko/ru i18n) so the page shows `budget_spent_usd`, reserved/held/remaining/over-limit state and the compatibility disclosure.
- Added changelog entries in `docs/en/CHANGELOG.md` and `docs/zh-CN/CHANGELOG.md`.

## TDD Evidence

- RED: reproduced mismatch is documented in `research/daily-spend-cap-evidence.md` (97.421556 + 3.350178 = 100.771734).
- GREEN:
  - `cargo test -p codexmanager-core -j 2 --lib -- --test-threads=1` -> 442 passed after the storage/migration implementation
  - `cargo test -p codexmanager-core -j 2 --lib aggregate_api_daily_spend -- --test-threads=1` -> 4 passed
  - `cargo test -p codexmanager-core -j 2 --lib aggregate_api_daily_usage_includes_budget_only_inflight_api -- --test-threads=1` -> 1 passed; verifies a reservation-only API remains visible in daily usage
  - `cargo test -p codexmanager-service -j 2 --lib quota::model_pricing::spend_quote_tests -- --test-threads=1` -> 2 passed
  - `cargo test -p codexmanager-service -j 2 --lib "gateway::upstream::protocol::aggregate_api::tests" -- --test-threads=1` -> 36 passed, 3 pre-existing Chat-Completions protocol failures unrelated to this change
  - `pnpm -C apps run build` -> passed twice after the typed budget projection/UI changes

## Verification

- `cargo check -p codexmanager-core -j 2` and `cargo check -p codexmanager-service -j 2` passed after the final projection change.
- Rust Analyzer reported 0 diagnostics for the five changed Rust implementation files; the frontend production build type-checked the TypeScript changes.
- `git diff --check` passed for all task-owned source, migration, frontend, i18n, and task-artifact paths.
- `task.py validate 08-20-diagnose-daily-api-spend-cap-overrun` passed with six implementation and six review context entries.
- `cargo fmt --all -- --check` remains non-green because the already-dirty workspace contains unrelated unformatted files and pre-existing formatting differences. The new `aggregate_api_daily_spend.rs` module was formatted directly.

## Independent Review

- NOT RUN: no `workflow-reviewer` subagent was available in this Pi session with the required fresh/reviewer context. Deterministic checks were run in the main session and are listed above.

## Commits

- NOT COMMITTED. The implementation is in the working tree, including the new task artifact directory and a large pre-existing dirty worktree.

## Remaining Risk

- Compatibility-first policy: requests without a priceable output limit, or with provider-reported final cost above the local quote, can still settle above the configured amount after admission. This is disclosed in the UI.
- Pre-existing Chat Completions upstream protocol tests in the working tree failed before this change; they are not caused by the budget work and should be fixed in the existing Chat-Completions integration branch.
- No full `cargo test --workspace` or Playwright runtime run was executed in this session due to the memory constraint and time; targeted serial core/service/frontend checks passed.