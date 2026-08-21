# Design — Aggregate API Chat Completions upstream compatibility

## 1. Separate candidate eligibility from upstream protocol

`provider_type` continues to mean client-family eligibility: `codex`, `claude`, `gemini`, or `compatible` (Codex plus Claude). `upstream_protocol` is an orthogonal nullable declaration:

```text
compatible + NULL -> legacy client-dependent passthrough
codex + NULL -> existing Responses behavior
codex/compatible + responses -> deterministic Responses upstream
codex/compatible + chat_completions -> deterministic Chat upstream
claude/gemini + non-NULL OpenAI protocol -> validation error
```

Runtime resolution may derive an effective protocol, but storage/RPC/frontend preserve the raw nullable value. Unchanged legacy rows are never rewritten to `responses`.

## 2. End-to-end flow and response plan

```text
Responses client -------------------------> canonical Responses request
Anthropic client -> existing adapter -----> canonical Responses request
client Chat/Compact -> existing adapter --> canonical Responses request
                                                   |
                                      immutable candidate plan
                                                   |
                 Responses upstream -----+---------+-------- Chat upstream
                                         |                  |
                                         |        shared Responses->Chat converter
                                         |                  |
                                         +---- canonical Responses decoder <--- Chat JSON/SSE
                                                   |
                              original client encoder (Responses/Anthropic/Chat/Compact)
```

Replace Aggregate execution's single-axis adapter with a mandatory two-axis type:

```text
ResponsePlan {
  upstream_decoder: Responses | ChatCompletions | AnthropicMessages | Gemini,
  client_encoder: Responses | AnthropicMessages | ChatCompletions | Compact,
  tool_name_restore_map: ToolNameRestoreMap,
}
```

Thread this through `AggregateProxyRequest`, candidate execution, delivery, request-log context, and fixtures. Existing direct adapters are parsed into a plan at normalization. This is required for Chat→Responses→Anthropic and Chat→Responses→client-Chat without losing original protocol or long tool-name restoration.

## 3. Persistence and validation

Add the next available additive migration (currently expected as `135_aggregate_api_upstream_protocol.sql`) with nullable `aggregate_apis.upstream_protocol`, using the existing compatibility/`ensure_column` pattern. Synchronize storage structs/SQL/decoders/fixtures; service validation and CRUD; camelCase RPC; Tauri/Web command signatures; frontend types, normalization, modal load/save/reset.

Validation is at the service boundary. Non-NULL OpenAI protocol for Claude/Gemini returns one consistent validation error; UI clears it before provider-change submission; storage never silently coerces. SQLite/RPC/frontend retain NULL; only runtime resolver derives effective behavior.

## 4. Request conversion

First extract and upgrade `request_rewrite_chat_completions.rs` into the sole typed pure converter under `gateway/protocol_adapter`; migrate existing client-Chat/Compact callers and Aggregate callers, then remove duplicate conversion logic.

Mapping includes:

| Responses | Chat |
|---|---|
| model | model after override |
| instructions/input messages | system/developer/messages preserving order |
| function tools | function tools |
| tool choice | representable choice |
| parallel_tool_calls | parallel_tool_calls |
| assistant function calls | assistant tool_calls |
| function_call_output | tool message with tool_call_id |
| max_output_tokens | max_completion_tokens |
| reasoning.effort | reasoning_effort |
| representable text.format | response_format |
| client stream | stream + stream_options.include_usage |

Preserve field provenance: remove only gateway-injected compatibility defaults; explicit client thinking/signature/state and parallel-tool constraints remain required. Apply immutable clone, existing safe transforms, model/provider rewrites, shared conversion, then URL/auth/header construction.

## 5. URL, headers, and diagnostics

- Default declared Chat action is `/v1/chat/completions`; explicit action overrides path only.
- Probe path is `nonempty(action) ? normalize(action) : default_path(protocol)`; body shape/parser come only from declared protocol.
- Transport may use query/querypair authentication, but every log/trace URL projection strips userinfo, fragment, and all query values.
- Bridged outbound headers are a fresh allowlist: Content-Type, Accept, controlled User-Agent, candidate auth/static headers. Exclude client Authorization/API-key, Cookie, Forwarded/X-Forwarded/X-Real-IP, session/thread/conversation/turn-state, Anthropic, and OpenAI organization/project headers.
- Converted responses use a fresh header map with target Content-Type and local trace/request IDs only, not upstream Set-Cookie, auth challenges, redirects, CORS, Content-Encoding, cache, length, transfer, connection, or Connection-token headers.

## 6. Canonical response conversion

Non-streaming is `Chat JSON -> Result<CanonicalResponses, AdapterFailure> -> client encoder`; streaming is `Chat SSE -> canonical semantic events -> client encoder`. Existing `Option` raw-body fallback is not allowed for the new path; conversion failures are typed `Result` values returned to candidate handling.

Carry `ToolNameRestoreMap` from local request adaptation through both encoders. Arguments remain opaque untrusted strings; never execute, repair, or log them. For Anthropic `tool_use.input`, strict parse at completion; malformed JSON is a protocol failure, never `{}` fallback.

## 7. Streaming state machine and preflight

State tracks response identity/model/time, output/content lifecycle, tool calls keyed by `(choice_index, tool_index)`, stable IDs/names/indexes, finish reason/status, usage with absent fields preserved, and error/incomplete state.

Aggregate stream preflight must run before response headers/request ownership are committed. It buffers a bounded prefix and drives the decoder until first client semantic event, protocol error, terminal result, timeout, or disconnect. Success returns warmed state plus remaining stream; pre-delivery failure returns candidate control; after first semantic event no replay.

Hard limits apply before allocation/appending: JSON/error body, SSE line/frame/total bytes, event count, choice count, tool count/index, ID/name bytes, per-call and total arguments. Reject unsupported content type, mixed Responses/Anthropic envelopes, `[DONE]` followed by data, identity changes, duplicate IDs, and sparse unbounded indexes.

Finish mapping, shared by JSON/SSE:

```text
stop          -> Responses completed / Anthropic end_turn
 tool_calls   -> Responses completed / Anthropic tool_use
length        -> Responses incomplete(max_output_tokens) / Anthropic max_tokens
content_filter/unknown -> normalized failure before delivery, target terminal error after delivery
```

Record finish reason, absorb usage-only chunk, and emit exactly one terminal at `[DONE]`; valid `[DONE]` without finish implies stop; EOF without `[DONE]` is incomplete failure. Define required fields, monotonic `sequence_number`, IDs/indexes, Content-Type, and terminal shape for target wire events. Fixed-version official OpenAI/Anthropic SDKs are required oracles in addition to internal golden fixtures.

## 8. Feature and tool compatibility

Support function schemas, representable tool choice, parallel-tool preservation (`parallel_tool_calls` / Anthropic `disable_parallel_tool_use`), assistant calls/outputs, multiple indexed calls, fragmented arguments, and long-name restoration. Unsupported target enforcement is typed incompatibility.

Ordinary Anthropic requests may contain gateway-injected reasoning/include defaults; remove those with a dedicated safe transform. Explicit client thinking/signature continuity remains incompatible unless replayable. Hosted tools, custom/non-function tools, provider state without safe replay, audio/image modalities, and non-representable structured output remain incompatible.

## 9. Usage, billing, and observability

Map prompt/completion tokens and available cache/reasoning detail through existing normalized usage. Missing remains `None`; explicit zero remains `Some(0)`. Client wire projection may add display zeros only; billing fallback estimation remains available when upstream usage is absent.

Keep one final request log; preserve original/adapted paths, bounded protocol/adapter labels, candidate identity, local status/error codes, bounded provider code, and trace/request IDs. Never persist free-text upstream errors, bodies, deltas, tool arguments, auth headers, cookies, or raw URLs with query values. Capability scope uses actual upstream `chat_completions`.

## 10. Failure and rollback

Local incompatibility skips health-neutrally. Local conversion/serialization errors are typed local gateway failures with no raw fallback. Upstream 4xx/5xx/SSE errors are bounded classifier input and expose only stable local code/message/trace ID. Malformed/cross-protocol/over-limit streams fail over only while preflight proves no semantic delivery; after delivery there is no replay.

Schema is additive/nullable. Old binaries ignore the field; new binaries preserve NULL legacy behavior. Rollback sets affected candidates to NULL legacy mode, explicit Responses, or disabled; no schema rollback and no automatic Responses↔Chat fallback.

## 11. Review gates

- Protocol fidelity: complete target wire contracts, no silent required-field loss, exact lifecycle/tool/finish ordering.
- Client transparency: original response protocol and tool-name mapping survive Aggregate routing.
- Data integrity: additive migration, NULL raw/effective distinction, and legacy compatible passthrough.
- Reliability: preflight boundary, no retry after delivery, malformed streams cannot become success.
- Billing: missing vs explicit-zero usage semantics and exactly-once normalization.
- Security/privacy: fresh header maps, sanitized URLs, stable errors only, no secrets/prompts/arguments/raw bodies.
- API parity: storage, service, RPC, Tauri, Web, frontend types, normalizer, and UI synchronized.
- Maintainability: one shared request converter, one canonical response/event model, no duplicated Chat→Responses and Chat→Anthropic state machines.
