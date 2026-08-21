# Design — Restore tokenrhythm upstream

## Decision

Treat a Chat stream that fails before any semantic event as a **candidate-local, recoverable upstream failure** when a later candidate exists. Do not mark it terminal. The next candidate receives the unchanged request; no request can be replayed once semantic output has been exposed to the client.

The authenticated live verification proved the configured 基元律动 endpoint, model, authentication mode, `stream:true`, `stream_options.include_usage:true`, and `reasoning_effort:high` can produce a semantic SSE event. No endpoint, protocol, URL, credential, or model-route change is required by this task.

## Existing flow

```text
candidate request
  -> HTTP success
  -> Chat preflight (only if a later candidate exists)
       -> semantic delta / [DONE]        -> delivery
       -> error / filter / EOF / timeout -> current code sets terminal_failure
  -> outer candidate loop
```

The current `ChatPreflightOutcome::Failover` path in `aggregate_api.rs` sets `last_failure_status=502`, `terminal_failure=true`, and exits the loop. This contradicts the preflight boundary: before semantic delivery, request ownership remains with the gateway and a later candidate is safe to try.

## Target flow

```text
candidate request
  -> HTTP success
  -> Chat preflight (later candidate exists)
       -> semantic delta / [DONE]        -> delivery; no replay thereafter
       -> pre-delivery failure            -> record candidate failure; continue outer loop
  -> later candidate
```

### Failure classes

| Condition before semantic delivery | Candidate action | Health |
|---|---|---|
| `data:` error / content filter | Continue to next candidate | Existing availability failure handling |
| EOF without semantic content | Continue to next candidate | Existing availability failure handling |
| idle timeout / disconnect / read error | Continue to next candidate | Existing availability failure handling |
| request body too large | Terminal | Health-neutral as today |
| no later candidate | Preserve current delivery path; no replay possible | Existing delivery result governs |

`terminal_failure` remains reserved for genuine request-global failures. The preflight branch must not use it.

## Observability

The first candidate must be appended to `attempted_aggregate_api_ids` before it is attempted, as today. On preflight failure, existing outer-loop accounting records the first candidate’s per-model health failure, then the terminal request record reflects the candidate that actually serves or the final candidate that fails. Do not log stream bytes, parsed SSE bodies, credentials, or raw upstream error text.

## Test design

Extend the existing Aggregate protocol mock harness, or add a focused streaming variant beside it, with deterministic local HTTP/SSE responses:

1. First declared Chat candidate returns HTTP 200 with an empty stream; second Responses candidate returns a valid Responses success. Assert two hits in order, final 200 from the second candidate, and a failure observation/runtime cooldown update for the first.
2. First Chat candidate produces a semantic delta. Assert the fallback receives no request even if the stream later terminates.
3. Unit-level preflight classification keeps current semantic, error, content-filter, metadata, and truncated-frame boundaries.
4. One-candidate behaviour remains unchanged because preflight is intentionally skipped without a possible safe replay.

## Compatibility and rollback

- Existing URL construction remains `base /v1` + `/chat/completions` without duplicate `/v1`.
- Existing Responses-to-Chat conversion, model override, auth headers, and response delivery adapter remain unchanged.
- Roll back by reverting the preflight candidate-failure handling change; no persistence or configuration migration is involved.
