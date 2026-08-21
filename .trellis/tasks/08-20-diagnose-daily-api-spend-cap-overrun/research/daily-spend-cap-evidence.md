# Daily Spend Cap Evidence

## Scope

Read-only diagnosis of the Aggregate API daily spend limit. No API secret, upstream URL, user data, or persistent setting was modified.

## Captured At

2026-08-20 local time.

## Confirmed Configuration

A single active Aggregate API with a configured positive daily limit exists in the local database:

| Field | Value |
| --- | --- |
| API identifier | `ag_e61927004...` |
| Supplier label | `input` |
| Status | `active` |
| Daily limit | 98.000000 USD |
| Updated local | 2026-08-20 10:43:26 |

The query intentionally selected no key, secret, or URL columns.

## Charge Snapshot Totals

The following values were calculated from `request_charge_snapshots`, using the same Aggregate API source attribution order as `Storage::sum_aggregate_api_charged_spend_microusd_between`:

| Local day | Charged requests | Charged USD |
| --- | ---: | ---: |
| 2026-08-20 | 200 | 20.738965 |
| 2026-08-19 | 1017 | 97.421556 |
| 2026-08-18 | 1186 | 93.185145 |
| 2026-08-14 | 914 | 94.394445 |
| 2026-08-13 | 1004 | 96.969767 |

The inspected local billing total does not exceed 98 USD for 2026-08-19.

## Implementation Evidence

1. `aggregate_api_has_daily_budget` converts the configured limit to integer micro-USD, reads completed charged snapshots, and approves the candidate while `charged_microusd < limit_microusd` in `crates/service/src/gateway/upstream/protocol/aggregate_api.rs`.
2. The check runs before the candidate is dispatched upstream in the same file.
3. `write_request_log_with_attempts` writes the request log and invokes `record_request_charge_v2` only after the response path returns an outcome in `crates/service/src/gateway/observability/request_log.rs`.
4. The charge snapshot query uses `request_charge_snapshots.created_at` in `crates/core/src/storage/model_billing_v2.rs`.

## Root Cause

The configured cap and the displayed usage total use different accounting scopes.

For 2026-08-19, the exact local reconciliation is:

| Component | USD | Requests | Included by cap | Included by page |
| --- | ---: | ---: | :---: | :---: |
| Completed normal request charge snapshots | 97.421556 | 1,017 | Yes | Yes |
| Guard `internal_retry` events | 3.350178 | 27 | No | Yes |
| Page total | 100.771734 | -- | No | Yes |

`request_token_stats.rs` deliberately adds `gateway_reasoning_guard_events` for `internal_retry` and `continuation_recovery` to `billable_estimated_cost_usd`. The Aggregate API page displays that field and labels it as including Guard retries. In contrast, the limit admits an API while the sum from `request_charge_snapshots` is below 98; Guard-retry events do not create an equivalent charge snapshot before the retry is dispatched. The page therefore shows 100.771734 USD while the enforcement path still sees 97.421556 USD and permits further use.

These Guard retries represent prior upstream responses that have already consumed supplier capacity and accrued estimated cost. Excluding them from the cap is the direct cause of the observed overrun.

## Secondary Risk

The existing comparison is still not an atomic hard cap. It evaluates completed snapshots before dispatch and records charges after the response completes. One expensive request, or several overlapping requests that each observe the same pre-request total, can overshoot the cap even after Guard costs are accounted for.

## Required Correction Direction

A correction should define one canonical daily-spend ledger for both display and enforcement, and debit or reserve every upstream attempt before a follow-up attempt is dispatched. At minimum, the limit must include the same `internal_retry` and `continuation_recovery` amounts shown on the page. To enforce a true ceiling, use a per-API, per-local-day atomic reservation that includes existing finalized charges plus in-flight reservations; reconcile each reservation to actual usage when the attempt completes.

Regression coverage should prove:

- A 97.421556 USD finalized total plus a 3.350178 USD Guard retry is treated as 100.771734 USD by both the page-facing query and the limit decision.
- A Guard retry that consumes remaining budget prevents any further attempt for that Aggregate API that day.
- Concurrent near-limit admissions cannot each pass against the same stale balance.
- Final charge reconciliation releases or adjusts reservations without allowing a second request to double-spend the daily budget.
