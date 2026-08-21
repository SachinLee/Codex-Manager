# Implementation — Restore tokenrhythm upstream

## Scope

`crates/service/src/gateway/upstream/protocol/aggregate_api.rs` and its focused regression tests. No frontend, storage schema, credential, route, or endpoint configuration edits.

## Ordered work

1. Read the service gateway specs and the relevant preflight/candidate-loop callsites before editing.
2. Add a deterministic failing regression: first Chat candidate returns an HTTP-success empty stream; a later candidate returns a valid response. Assert the later candidate is called and the final client result succeeds.
3. Change the pre-delivery `ChatPreflightOutcome::Failover` handling so it records the candidate failure and leaves `terminal_failure` false, allowing the outer candidate loop to continue.
4. Ensure the failure remains eligible for existing `gateway_record_aggregate_api_failure` handling after the inner loop exits.
5. Add no-replay coverage for a semantic Chat delta and retain no-later-candidate behaviour.
6. Run the narrow service regression suite; then run the required service package tests. Review the final diff for endpoint/auth/request-conversion changes outside scope.

## Acceptance checks

```powershell
cargo test -p codexmanager-service aggregate_api
cargo test -p codexmanager-service protocol_adapter
cargo test -p codexmanager-service http_bridge
```

The targeted test must prove both actual upstream requests: empty Chat stream first, serving fallback second. It must not use the real provider or stored credential.

## Risk controls

- Never retry after a semantic event has reached the client.
- Do not broaden terminal 413 semantics.
- Do not alter probe behavior in this focused repair; its non-streaming limitation is documented and separate from restoring safe fallback.
- Keep stable local errors and existing safe logging rules.

## Rollback

Revert the candidate-failure continuation change and its tests. No data migration, remote configuration change, or credential modification occurs.
