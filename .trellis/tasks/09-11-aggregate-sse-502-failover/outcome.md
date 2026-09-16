# Outcome

## Delivery
- Status: complete
- Summary: Strict buffered Aggregate Responses SSE now returns zero-delivery terminal failures to the Aggregate candidate loop. `server_error`, `service_unavailable_error`, `gateway_concurrency_limit`, and `rate_limit_exceeded` can fail over to the next candidate without replaying delivered text or tool calls. Candidate decisions are traced, and Aggregate attempt records are persisted in request logs.

## Acceptance Criteria
| Criterion | Result | Evidence |
| --- | --- | --- |
| AC-001 | PASS | `aggregate_sse_gateway_concurrency_limit_fails_over_to_next_candidate`; serial Aggregate SSE suite passed. |
| AC-002 | PASS | `aggregate_sse_server_error_fails_over_to_next_candidate`, `aggregate_sse_service_unavailable_error_fails_over_to_next_candidate`, and existing rate-limit/prefix tests. |
| AC-003 | PASS | `aggregate_sse_with_delivered_content_does_not_replay` and `aggregate_sse_with_delivered_tool_call_does_not_replay`. |
| AC-004 | PASS | Serial `cargo test -p codexmanager-service --lib aggregate_sse_ -- --test-threads=1`: 9 passed, 1 ignored. |
| AC-005 | PASS | Existing invalid-request and capacity-related Aggregate tests remain in the focused suite; classifier tests passed. |
| AC-006 | PASS | Existing cooldown/failure recording paths remain unchanged; focused Aggregate and request-log checks passed. |
| AC-007 | PASS | `AGGREGATE_CANDIDATE_DECISION` trace event includes trace ID, candidate ID, position, sanitized URL, status, bounded error code, decision, zero-delivery state, and retry ordinal. |
| AC-008 | PASS | `aggregate_api_attempts` is wired through request-log context, SQL persistence, row mapping, migration 140, timeout path, and final Responded path. |
| AC-009 | PASS | Account-pool callsite passes `allow_zero_delivery_failover=false`; focused service compilation and Aggregate tests passed. |
| AC-010 | PASS | Zero-delivery opt-in is Aggregate-only; account-pool delivery remains non-replayable and unchanged. |

## Implementation
- Added `allow_zero_delivery_failover` through the HTTP bridge. Aggregate passes `true`; account-pool finalization passes `false`.
- Strict buffered Responses SSE checks HTTP 200, terminal/read failure, no semantic text, no tool-call delivery, zero output tokens, and no client-visible bytes before returning the request for failover.
- Added `semantic_output_delivered` tracking for Responses text and function/custom tool-call events to preserve the no-replay invariant.
- Classified the required terminal error codes as `CandidateFailover` while preserving capacity, capability, reasoning-guard, request-terminal, retry-budget, and cooldown precedence.
- Added bounded terminal `type=` extraction fallback and sanitized candidate decision trace events.
- Added `AggregateApiAttemptRecord` persistence through `request_logs.aggregate_api_attempts` and migration `crates/core/migrations/140_request_logs_aggregate_api_attempts.sql`.
- Added English backend logging-spec coverage for the cross-layer Aggregate attempt contract.

## TDD Evidence
- RED: original production diagnosis and task artifacts captured the zero-delivery SSE 502 behavior; a new standalone RED run was not captured in this session.
- GREEN: serial focused Aggregate SSE suite passed 9 tests with 1 existing ignored test; tool-call parser and no-replay regression tests passed.

## Verification
- `cargo test -p codexmanager-service --lib aggregate_sse_ -- --test-threads=1` — 9 passed, 1 ignored.
- `cargo test -p codexmanager-service --lib aggregate_sse_with_delivered_tool_call_does_not_replay` — 1 passed.
- `cargo test -p codexmanager-service --lib parse_openai_responses_event_marks_tool_call_as_semantic_output` — 1 passed.
- `cargo test -p codexmanager-service --lib upstream_failure -- --test-threads=1` — 3 passed.
- `cargo test -p codexmanager-core --lib request_logs` — 31 passed.
- `cargo check -p codexmanager-core -p codexmanager-service` — completed successfully with pre-existing warnings.
- A parallel test attempt hit Windows linker `LNK1104` because concurrent test processes contended for the service test executable; the serial rerun passed.
- Full workspace tests, formatters, linters, and project-wide suites were not run per task constraints.

## Independent Review
- `workflow-reviewer` independent review: no actionable findings. Confirmed AC-001 through AC-010, request ownership/spend release, attempt persistence including timeout/final Responded, trace privacy, and preservation of account-pool and specialized retry semantics.

## Commits
- NOT COMMITTED

## Remaining Risk
- Full workspace validation remains outside this focused task pass.
- Existing unrelated compiler warnings remain; no warning suppression or unrelated cleanup was added.
