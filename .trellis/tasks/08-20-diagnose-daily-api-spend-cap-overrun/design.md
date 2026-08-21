# Design: Aggregate API compatibility-first daily spend enforcement

## Context

The current daily spend check reads only finalized `request_charge_snapshots`, whereas the Aggregate API page adds `gateway_reasoning_guard_events` for `internal_retry` and `continuation_recovery`. The reproduced 2026-08-19 discrepancy was 97.421556 USD in snapshots plus 3.350178 USD from retries, producing a page total of 100.771734 USD against a 98 USD limit.

This is critical-risk work because it controls spend, writes persistent data, and must serialize across the service storage pool's independent SQLite connections.

## Selected Policy

Use the user-selected compatibility-first policy:

- Every deterministically priced portion of an upstream attempt is atomically reserved before dispatch.
- Every completed, priceable attempt is settled into the same daily budget, including Guard retries and continuation recovery.
- A request with no usable output-cost upper bound remains eligible. It reserves its known input portion (or zero when the model cannot be priced) and may settle above the remaining budget when the upstream response returns actual output or provider cost.
- The service reports this residual overrun explicitly. It must never silently report an amount that uses a different accounting scope than enforcement.

This removes the observed retry omission and concurrent stale-read race. It is intentionally not a guarantee that an unbounded or provider-repriced request can never settle above the configured amount.

## Budget Definition

For an Aggregate API and one service-local calendar day:

```
used = opening_spend + settled_spend
committed = used + active_reservations
remaining = max(limit - committed, 0)
```

- `opening_spend` is a one-time, immutable snapshot of the legacy page-equivalent cost at the first reservation for that API/day. It includes existing finalized snapshots and Guard retry events, so no historical rows need rewriting.
- `settled_spend` is the immutable micro-USD amount for every subsequently completed, priceable upstream attempt.
- `active_reservations` are pre-dispatch quotes that have not yet been settled, released, or held after an indeterminate timeout.
- All amounts are non-negative integer micro-USD. Floats are only converted at the UI/RPC boundary.
- A candidate with an active limit may dispatch only when its deterministic quote fits within `remaining`. An unpriced quote remains eligible under the selected policy and is logged as unbounded.

The legacy aggregate-page token and request counts remain the source of token detail. Its cost display is overridden by an active daily budget's `used` amount; before the first reservation on a day it continues using the existing legacy aggregate, which is also the source of `opening_spend`.

## Persistence and Atomicity

Add an additive core migration, allocated as the next available migration number at implementation time (currently expected after `136_request_logs_upstream_protocol`):

```sql
CREATE TABLE aggregate_api_daily_spend_buckets (
  aggregate_api_id TEXT NOT NULL REFERENCES aggregate_apis(id) ON DELETE CASCADE,
  day_start_ts INTEGER NOT NULL,
  opening_spend_microusd INTEGER NOT NULL,
  settled_spend_microusd INTEGER NOT NULL DEFAULT 0,
  reserved_spend_microusd INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (aggregate_api_id, day_start_ts)
);

CREATE TABLE aggregate_api_daily_spend_attempts (
  id TEXT PRIMARY KEY,
  aggregate_api_id TEXT NOT NULL REFERENCES aggregate_apis(id) ON DELETE CASCADE,
  day_start_ts INTEGER NOT NULL,
  trace_id TEXT NULL,
  attempt_kind TEXT NOT NULL,
  state TEXT NOT NULL,
  pricing_state TEXT NOT NULL,
  reserved_microusd INTEGER NOT NULL DEFAULT 0,
  settled_microusd INTEGER NULL,
  request_log_id INTEGER NULL,
  created_at INTEGER NOT NULL,
  resolved_at INTEGER NULL
);
```

`attempt_kind` is constrained to `initial`, `transport_retry`, `capacity_retry`, `guard_retry`, and `continuation_recovery`. `state` is constrained to `reserved`, `settled`, `released`, and `held`. `pricing_state` distinguishes `quoted`, `unbounded_output`, `unpriced_model`, and `provider_reported` for auditability.

Core storage owns a focused `aggregate_api_daily_spend` module with these transaction-safe operations:

- `reserve_aggregate_api_daily_spend`: opens the bucket lazily, computes the legacy opening amount once, reclaims stale reservations to `held`, and uses `TransactionBehavior::Immediate` to compare and insert a reservation without a cross-connection race.
- `settle_aggregate_api_daily_spend_attempt`: idempotently moves an attempt's reserved amount to its actual charge and updates bucket totals in the same transaction.
- `release_aggregate_api_daily_spend_attempt`: idempotently removes an attempt that definitively reached no billable upstream execution.
- `hold_expired_aggregate_api_daily_spend_attempts`: moves an ambiguous in-flight attempt to `held`; held value remains committed for the day rather than being silently dropped after a crash or uncertain timeout.
- `read_aggregate_api_daily_spend_summary`: returns optional opening, settled, reserved, held, committed, remaining, and over-limit state for the page/RPC.

The existing service storage pool can open multiple SQLite connections. `Immediate` transactions, not an in-memory mutex, are required so independent pooled connections serialize the read-modify-write admission decision. Storage lock retry behavior should match the existing charged-snapshot retry policy.

## Pricing and Attempt Lifecycle

### Quote before dispatch

The Aggregate API protocol path creates a fresh opaque attempt ID directly before `reqwest` dispatch, after local body conversion/auth validation has succeeded.

The quote helper uses the current model catalog's integer price tiers and the Aggregate API multiplier:

- Input is the current request-body estimate, with cache reads/writes assumed to be zero for conservative local pricing.
- Output is the transformed upstream request's declared maximum when available: `max_output_tokens`, Chat `max_completion_tokens`/`max_tokens`, or Anthropic `max_tokens`; Chat `n` multiplies the output bound.
- No output bound produces an `unbounded_output` quote containing only the known input component.
- An unknown or missing local price produces `unpriced_model` with a zero deterministic reservation; it is allowed by the selected policy and generates a structured warning.
- Provider-reported price is not available pre-dispatch and therefore cannot be reserved exactly.

The quote API is shared by the Guard-event writer and final settlement. It must preserve the existing provider-cost precedence and apply the Aggregate API multiplier once. This also fixes the current Guard estimate path's failure to carry the candidate multiplier.

### Resolve after dispatch

- A successful terminal response creates its existing request log and immutable charge snapshot. `write_request_log_with_attempts` receives an optional budget attempt ID, then settles that attempt from the resulting `ChargeSnapshotV2.charged_cost_microusd`. The snapshot remains authoritative for provider-reported cost.
- A Guard trigger synchronously settles the budget attempt before scheduling a retry, then records its existing observability event through the current bounded async queue. The budget never waits for or derives from that asynchronous event; the retry passes through a new reservation gate.
- A continuation-recovery attempt uses `attempt_kind = continuation_recovery`; ordinary Guard retry uses `guard_retry`.
- Capacity and transport retries reserve independently. An upstream error that contains a valid provider cost or usable usage settles the attempt; an error with no billable evidence releases it. This preserves current successful-request accounting while making known billable failures visible in the budget summary.
- Local conversion, configuration, secret lookup, and client-builder failures occur before reservation and never alter budget state.
- Send-time ambiguity, client disconnect, process crash, and timeout after dispatch do not silently release the reservation. They become `held` after the request deadline plus a bounded grace period, count for the current day, and are retained for audit cleanup.

When a limit is cleared during a day with an existing bucket, the gateway continues lifecycle tracking for that bucket but skips admission rejection. This keeps the page and later re-enabled enforcement coherent. A day that never had an active bucket continues to use the legacy view.

## Gateway Behavior and Public Contract

Replace the non-atomic `aggregate_api_has_daily_budget` prefilter with a per-dispatch reservation gate. A rejected candidate is recorded as `aggregate_api_daily_spend_limit_reached` and the existing routing behavior continues to the next eligible candidate; all exhausted candidates retain the existing 429/fallback semantics.

Add structured logs without secrets or request bodies:

- `aggregate_api_daily_spend_reserved`
- `aggregate_api_daily_spend_settled`
- `aggregate_api_daily_spend_released`
- `aggregate_api_daily_spend_held`
- `aggregate_api_daily_spend_rejected`
- `aggregate_api_daily_spend_unbounded_quote`
- `aggregate_api_daily_spend_settled_over_limit`

Extend the existing aggregate daily-usage RPC additively with optional fields: `budgetSpentUsd`, `budgetReservedUsd`, `budgetHeldUsd`, `budgetRemainingUsd`, and `budgetOverLimit`. Existing desktop/web clients remain valid when fields are absent.

The Aggregate API page uses `budgetSpentUsd` for each API that has an active bucket and retains the legacy cost otherwise. Its tooltip shows settled versus reserved/held budget and explains that unbounded requests may settle beyond the limit. The daily-limit input description is revised to state the compatibility-first policy; no new setting is introduced.

## Compatibility, Migration, and Recovery

- The migration is additive. No existing request log, charge snapshot, Guard event, or usage rollup is rewritten.
- Lazy bucket creation captures a current-day legacy baseline inside the first reservation transaction. This is the rollout bridge for a process updated mid-day.
- Historical day reporting remains unchanged because there are no buckets for past days. Current-day reporting is overridden only after a bucket exists.
- Cleanup may prune resolved attempt records after the repository's existing observability retention horizon. Held records remain queryable through the end of their own day and are never moved into a future day.
- Rolling back code leaves additive tables ignored. The legacy cap behavior resumes, so rollback is operationally safe but reintroduces the known accounting defect; no data deletion is part of rollback.

## Alternatives Considered

### Only add Guard events to the current snapshot sum

Rejected. It fixes the reproduced discrepancy only after the first omitted retry and does not atomically protect concurrent admissions, capacity retries, or restart recovery.

### Hide Guard retry costs in the page

Rejected. Those calls reached the upstream and are observable cost. Hiding them would make the UI inaccurate rather than enforcing the limit.

### Strict hard cap by rejecting unbounded requests

Rejected for this task by the selected compatibility-first policy. It would require clients to supply a priceable output bound and still cannot precisely model provider-side tokenization or post-response provider prices.

### In-memory per-API locks

Rejected. The service uses a SQLite connection pool and may be restarted; process-local locking cannot serialize all pooled connections or provide crash recovery.

## Risks and Rollout

The selected policy still permits an individual unbounded request, a provider-reported cost higher than its local quote, or an uncertain in-flight attempt to consume more than the nominal remaining amount. The UI must show this as a settled-over-limit event rather than making a false hard-cap claim.

Roll out with the migration and structured logs first. Exercise bounded, unbounded, Guard retry, and concurrent cases in an isolated database. Monitor rejected/reserved/held/over-limit events for one release before considering a strict-cap mode. Roll back by deploying the prior binary; retain the additive audit tables for later diagnosis.
