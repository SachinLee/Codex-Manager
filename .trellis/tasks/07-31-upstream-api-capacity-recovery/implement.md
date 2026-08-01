# 上游 API 容量错误自动恢复实施计划

## Implementation Checklist

1. In `crates/service/src/gateway/upstream/protocol/aggregate_api.rs`, replace the one-attempt capacity budget with two replays and introduce service-local backoff constants.
2. Add a focused capacity-retry branch or helper in the Aggregate API loop that consumes only the capacity budget, applies `sleep_with_exponential_jitter`, honours `request_deadline`, and logs its decision.
3. Ensure HTTP non-success capacity errors cannot fall through to `transport_retry_budget_remaining` and cannot continue to another Aggregate API candidate after exhaustion.
4. Handle `respond_with_upstream` capacity outcomes consistently: retry while `pending_failover_request` is available; on exhaustion, write a 503 using that request and return immediately.
5. Preserve the no-replay condition after visible stream delivery and retain existing behavior for non-capacity errors.
6. Extend `crates/service/src/gateway/observability/metrics.rs` and its public gateway wrappers for capacity detections and exhausted recoveries; retain the existing retry metric for compatibility.
7. Add focused Aggregate API regression tests and metric tests for all scenarios in `design.md`.
8. Run formatting, narrow service tests, then the workspace-level validation required for this Rust gateway change.

## Planned Files

- `crates/service/src/gateway/upstream/protocol/aggregate_api.rs`
- `crates/service/src/gateway/upstream/protocol/aggregate_api_tests.rs`
- `crates/service/src/gateway/observability/metrics.rs`
- `crates/service/src/gateway/mod.rs`
- Relevant metrics or gateway test modules discovered while implementing

## Validation

```powershell
cargo fmt --check
cargo test -p codexmanager-service aggregate_api
cargo test -p codexmanager-service gateway
cargo test --workspace
```

If the full workspace suite is impractical in the environment, record the exact failing or unavailable command and run the narrowest relevant service tests.

## Risk Controls

- Keep the error matcher unchanged and narrow.
- Do not mutate the original request body; every replay derives from the original candidate body.
- Do not route to another candidate after a recognized capacity error.
- Do not retry after client-visible stream output.
- Review the final diff for accidental changes to cooldown, account-pool or generic transport retry behavior.
