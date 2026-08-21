# Restore tokenrhythm upstream

## Goal

Make the configured 基元律动 (`https://tokenrhythm.studio`) Aggregate API usable for normal streamed Codex/Responses traffic through its declared OpenAI Chat Completions upstream, rather than returning a generic 502 before delivery.

## Confirmed Facts

- The Aggregate API is active, has `upstream_protocol=chat_completions`, no custom action, and base URL `https://tokenrhythm.studio/v1`.
- For six requests on 2026-08-13, CodexManager correctly built `https://tokenrhythm.studio/v1/chat/completions`; all requests used `deepseek-v4-flash-0731`, `stream=true`, and `reasoning_effort=high`.
- Those six requests all failed before client delivery with `chat upstream stream ended before producing deliverable content`; `first_response_ms` was absent and the trace contains no `BRIDGE_RESULT`.
- The error is emitted only by `preflight_chat_stream` after its bounded prefix reader reaches `Eof` without a semantic Chat SSE event.
- The candidate list contained later routes: the error can occur only when `candidate_idx + 1 < total_candidates`. Nonetheless, only 基元律动 was recorded as attempted because the `ChatPreflightOutcome::Failover` branch sets `terminal_failure=true` and breaks the outer candidate loop.
- The candidate order is deterministic for `ordered`: 基元律动 sort 404, wawa sort 411, aiswitch sort 415. The persisted 5-minute cooldown for this model had already expired before the six requests.
- The Chat health probe is not representative of this traffic: it sends `stream:false` with a minimal `"hi"` request, while production sends a converted streaming Responses request with `reasoning_effort=high`.
- Historical requests prior to the protocol declaration targeted `/v1/responses` and failed reading invalid UTF-8. One later request was correctly rejected locally for unavailable `previous_response_id` conversation context. Neither proves the current Chat URL is wrong.
- The authorized 2026-08-13 live check sent one minimal authenticated request to the configured Chat endpoint with the configured model, `stream:true`, `stream_options.include_usage:true`, and `reasoning_effort:high`. It returned HTTP 200, `text/event-stream`, and a semantic SSE event in 1,643 ms. No credential, request body, or response content was retained. Therefore the provider and model can produce a valid streaming Chat response; the six failed production requests remain a gateway/request-shape interaction, not proof that 基元律动 is unavailable.

## Requirements

### R1. Recoverable pre-delivery Chat stream failures

- An empty Chat stream, pre-delivery disconnect, idle timeout, or pre-delivery stream read error from a candidate with later eligible candidates must continue to the next candidate.
- The gateway must never replay after a client-visible semantic Chat event was delivered.
- A genuine terminal failure class, such as request-body-too-large, remains terminal.

### R2. Accurate health and observability

- Pre-delivery Chat stream failures that constitute upstream availability failures must be eligible for the existing per-model cooldown/health recording path.
- The final request log must retain the last attempt’s protocol, URL, error, and all actually attempted candidate IDs without leaking body data, credentials, or raw upstream errors.

### R3. Stream diagnosis and compatibility boundary

- Classify a complete pre-delivery non-SSE error payload or valid `data:` error frame as a recoverable candidate failure with a stable local error.
- Preserve the existing behavior for incomplete metadata-only frames: they require more data until EOF/timeout, then fail over if another candidate exists.
- Preserve endpoint, request conversion, model override, and authentication behavior: the authorized live check proved the configured provider accepts the target model and stream form.

### R4. Representative verification

- Add deterministic local regression coverage proving an empty Chat SSE stream fails over to a later candidate, serves the client via that candidate, records both attempts, and records health for the first candidate.
- Add coverage for no later candidate: no preflight replay is possible; preserve current single-candidate delivery semantics.
- Keep the live-provider result as operational evidence only; deterministic tests must not depend on the provider or its credential.

## Acceptance Criteria

- [ ] With two eligible candidates, a first Chat candidate that returns HTTP success and an empty stream causes one request to the next candidate, which can return a successful Responses result.
- [ ] The client receives no partial output from the failed Chat candidate and sees a normal result from the serving fallback candidate.
- [ ] The first candidate’s failure is accounted for by existing cooldown/health logic; the serving candidate remains the terminal request-log source.
- [ ] A semantic Chat delta prevents retry/replay even if the stream later fails.
- [ ] Existing Chat URL construction, Responses-to-Chat conversion, model override, and authentication remain unchanged.
- [ ] The live check result is recorded as operational evidence only; automated tests use local mock upstreams and never require the provider credential.

## Out of Scope

- Auto-detecting provider protocol or changing the configured URL/action.
- Rewriting provider-specific model behavior, request semantics, or pricing.
- Retrying after semantic delivery.
- General gateway routing redesign.
- Altering credentials or making any production configuration change.

