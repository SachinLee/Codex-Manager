# Integrate latest main while preserving custom features

## Goal

Integrate the latest `main` into `codex/custom-features` without losing the
branch's product behavior, while adopting `main`'s model catalog v2 and
candidate-isolated Gateway architecture as the single maintainable foundation.

## Confirmed facts

- The integration branch starts at custom-features commit `08b740ff`; latest
  `main` is `514f3dba`; the merge base is `ce5d3f38`.
- A merge simulation reports 65 conflicts across 107 overlapping files.
- `main` has replaced legacy remote model discovery and `apikey_models` runtime
  paths with model catalog v2, atomic model RPC, and integer charge snapshots.
- The custom branch adds Codex Launcher/session support, usage and quota views,
  Aggregate API cooldown/diagnostics, retry and capability-aware routing,
  advanced GPT-5.6/Grok pricing, and request-log model statistics.
- `main` has migration IDs through `116_request_logs_visibility`; this branch
  has separately-applied migrations with overlapping numeric prefixes.

## Requirements

- R1: Integrate latest `main` through a non-fast-forward merge on this dedicated
  integration branch; do not alter `codex/custom-features`.
- R2: Preserve all custom product capabilities listed above, subject to the
  behavioral contracts in their existing Trellis PRDs/designs.
- R3: Keep model catalog v2 as the only runtime source of truth. Do not restore
  remote model sync, legacy `apikey_models`, or compiled price-rule fallbacks.
- R4: Port custom pricing features to model catalog v2 and immutable integer
  charge snapshots: GPT-5.6 cache writes/long-context semantics, Grok 4.5
  actual-cost fallback, service tier, and multiplier-once accounting.
- R5: Preserve main's candidate-specific request rewrite isolation. Capability,
  timeout, capacity, and reasoning-guard recovery must operate from immutable
  original requests and must never retry after client delivery begins.
- R6: Maintain the request-log visibility/clear behavior from main while adding
  session tracing, pricing audit fields, attempt events, and filtered model-use
  statistics.
- R7: Preserve desktop and Web command parity, static-export compatibility, and
  all existing authorization boundaries.
- R8: Keep the root untracked `package.json` outside the integration and outside
  all commits.

## Acceptance Criteria

- [ ] The merge completes with no conflict markers and no loss of main's model
      catalog v2, log visibility, startup, onboarding, or candidate-isolation
      behavior.
- [ ] No runtime reference to deleted legacy model discovery or `apikey_models`
      paths is reintroduced.
- [ ] A post-main additive migration path supports a fresh database, an
      up-to-date main database, and a database that previously ran the custom
      branch migrations without overwriting user-edited prices.
- [ ] GPT-5.6 and Grok pricing tests cover cache write, 272K strict threshold,
      200K inclusive threshold, Priority, provider actual cost, and a
      multiplier applied exactly once.
- [ ] Gateway tests cover Responses and Chat Completions; streaming and
      non-streaming; tools/tool_calls; capability off/observe/enforce; retry
      isolation; and no retry after delivery.
- [ ] Request-log list/clear/visibility, session context, model-use statistics,
      and admin/non-admin authorization retain their defined behavior.
- [ ] Codex Launcher/session controls coexist with main's tray/window/startup
      handling.
- [ ] `cargo test --workspace`, `cargo test -p codexmanager-web`,
      `pnpm -C apps run test:runtime`, `pnpm -C apps run build`, and
      `pnpm -C apps run build:desktop` pass, or any environment failure is
      recorded precisely.
- [ ] Request-token daily, user, source, key, and model range reads retain
      hourly-compacted history without double counting raw rows; partial-hour
      boundaries remain excluded from hourly aggregates.
- [ ] Request-log summary and `requestlog/list_with_summary` work for both
      administrators and members while preserving key ownership filtering.
- [ ] WebSocket usage preserves cache-write token details, and dashboard token
      activity rejects ranges wider than 365 days.
- [ ] Frontend runtime checks pass with complete locale coverage and
      newline-format-independent static source assertions.

## Out of Scope

- Rewriting product features that are unrelated to merge compatibility.
- Backfilling historical charges, request logs, or wallet balances.
- Pushing, opening a pull request, or changing the remote branch.
