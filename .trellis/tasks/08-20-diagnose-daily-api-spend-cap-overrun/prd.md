# Correct Aggregate API daily spend enforcement

## Goal

Produce an implementation-ready, critical-risk design for correcting Aggregate API daily spend enforcement so its budget scope matches the Aggregate API page and the configured limit has defined behavior for retries and concurrent requests. No product code will be changed during planning.

## Background

- An active Aggregate API had a configured daily limit of 98 USD.
- On 2026-08-19, the limit read 97.421556 USD from finalized charge snapshots, while the Aggregate API page displayed 100.771734 USD.
- The 3.350178 USD difference was 27 Reasoning Guard `internal_retry` attempts. The page includes those costs but the cap excludes them.
- The existing pre-dispatch check observes only finalized charges, so it also cannot prevent concurrent or single-request post-admission overshoot.
- Complete evidence is recorded in `research/daily-spend-cap-evidence.md`.

## In Scope

- Design a single daily spend definition for Aggregate APIs that includes every supplier attempt represented in the existing page total, including `internal_retry` and `continuation_recovery`.
- Design durable, atomic per-API/per-local-day budget reservation and actual-cost reconciliation so concurrent gateway requests cannot independently admit against a stale balance.
- Define how the gateway handles known and unknown output bounds, provider-reported cost, failed attempts, retries, timeouts, and restart recovery while a daily limit is active.
- Specify the minimal schema, storage, gateway, observability, API/UI, migration, test, rollout, and rollback changes required.

## Out of Scope

- Changing the configured 98 USD value, rewriting historical request logs, or changing upstream provider billing.
- Changing display cost merely to hide retry costs.
- General Aggregate API routing, health, model catalog, or UI redesign unrelated to spend enforcement.

## Acceptance Criteria

- [ ] AC-001: The design defines one authoritative daily budget amount and explicitly maps normal attempts, Guard retries, capacity retries, failures, and final successful responses to it.
- [ ] AC-002: The design defines an atomic reservation/reconciliation flow that prevents two concurrent attempts for the same API/day from exceeding the configured daily limit according to the selected policy.
- [ ] AC-003: The design specifies compatible persistence, migration, restart recovery, observability, and rollback behavior without recalculating historical charges.
- [ ] AC-004: The execution plan contains ordered RED/GREEN slices, public test seams, validation commands, and regression cases for the observed 97.421556 + 3.350178 = 100.771734 USD failure.

## Key Decision

- Selected policy: compatibility-first. Atomically reserve every deterministically quoted portion of each upstream attempt and include all settled retry costs. Requests without a usable output-cost upper bound remain eligible; the UI and logs must make clear that such a request may settle above the remaining daily budget.
