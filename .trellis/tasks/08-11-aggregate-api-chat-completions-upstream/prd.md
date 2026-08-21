# Aggregate API Chat Completions upstream compatibility

## Goal

Allow Aggregate API candidates whose upstream only implements OpenAI Chat Completions to serve existing Responses, Anthropic Messages, client Chat Completions, and applicable Compact flows without client-side changes. CodexManager owns request/response conversion while preserving each original client wire protocol plus existing routing, health, billing, tool-call, and streaming contracts.

## Requirements

### R1. Explicit upstream protocol contract

- Add nullable `upstream_protocol` for OpenAI-compatible Aggregate API candidates with values `responses` and `chat_completions`.
- Keep `provider_type` as the client-family/candidate-eligibility contract; do not overload it with the upstream wire protocol.
- Do not infer runtime protocol from URL or action substrings.
- NULL preserves existing client-dependent behavior: legacy `compatible` continues Responses clients to Responses paths and Anthropic clients to Messages paths; `codex` retains existing Responses behavior. Only explicit values enable the new deterministic upstream contract.
- `chat_completions` is valid only for `codex` and `compatible`; direct service create/update with a non-NULL OpenAI protocol for Claude/Gemini returns a validation error.

### R2. Client transparency

- `/v1/responses` clients receive valid Responses JSON/SSE when the selected upstream uses Chat Completions.
- `/v1/messages` clients receive valid Anthropic Messages JSON/SSE when the selected upstream uses Chat Completions.
- Existing client `/v1/chat/completions` and applicable Compact flows retain their original output contracts through a declared Chat Completions candidate.
- Preserve the original client response protocol across Aggregate candidate selection; canonical Responses payloads/events must not leak to another client protocol.
- Existing Responses, Anthropic-native, Gemini, Compact, and client Chat behavior remains unchanged for candidates without an explicit Chat protocol.

### R3. Faithful request conversion

- Extract and upgrade the existing Responses-to-Chat conversion core into one typed pure converter; migrate existing client-Chat/Compact callers and the Aggregate caller to it.
- Convert text messages, system/developer instructions, function tools, tool choice, `parallel_tool_calls`, assistant tool calls, tool outputs, token limits, supported reasoning effort, representable text format, and streaming usage.
- Default the upstream action to `/v1/chat/completions`; explicit action remains a path override.
- Preserve candidate model override and authentication/static headers.
- Build each candidate body from immutable normalized input; candidate conversion/retry transforms cannot leak.

### R4. Explicit incompatibility for lossy features

- Never silently discard required semantics that Chat Completions cannot represent.
- Preserve field provenance/requiredness. Gateway-injected Anthropic compatibility defaults may be removed by a code-owned safe transform; client-requested thinking/signatures/provider state and parallel-tool constraints remain required.
- `previous_response_id` without complete stateless replay, client-required encrypted/provider-bound reasoning, hosted/custom/non-function tools, unsupported modalities, and non-representable response formats make a candidate incompatible unless an existing safe transform applies.
- Local incompatibility is health-neutral and occurs before transport.
- Optional capability removal remains owned by the existing capability planner.

### R5. Streaming and non-streaming conversion

- Convert Chat JSON to canonical Responses with a typed `Result`; never fall back to raw upstream body.
- Incrementally convert Chat SSE text, optional reasoning, indexed tool-call fragments, finish reasons, `[DONE]`, usage, and structured errors.
- Preserve tool IDs/names, argument order, output indexes, `parallel_tool_calls`, and one terminal lifecycle.
- Define target wire contracts: required fields, monotonic `sequence_number`, Content-Type, ID/index relationships, finish mapping, and terminal shape. Validate with fixed-version official OpenAI and Anthropic SDKs.
- Preserve absent usage fields as absent and explicit upstream zero as zero in billing state; projection-only display defaults cannot alter normalized usage.
- Enforce bounded JSON/SSE body, line, frame, event, choice, tool/index, ID/name, and accumulated argument sizes.
- Malformed, incomplete, cross-protocol, over-limit, or unconvertible responses never become success.

### R6. Configuration and cross-layer synchronization

- Add an additive SQLite migration and compatibility `ensure_column` path.
- Synchronize core storage, service create/update/list, RPC, Tauri, Web mapping where needed, typed frontend payloads, normalizers, imports, and modal state.
- UI offers the selector for `codex`/`compatible`; it displays Responses as effective default but preserves stored NULL until explicit selection.
- Changing provider to Claude/Gemini clears the field in UI state; service rejects invalid non-NULL combinations.
- Diagnostics/test connection use declared protocol for body/parser and `action ?? default_path(protocol)` for path.

### R7. Routing, health, usage, observability, and security

- Preserve ordering, route strategy, quota, deadlines, retry budgets, reasoning guard, cooldown, and no-retry-after-delivery.
- Add protocol-aware Aggregate stream preflight before response headers/request ownership are committed, allowing pre-delivery failover but never post-delivery replay.
- Capability facts/evidence use actual upstream protocol `chat_completions`.
- Keep one final request log, bounded protocol/adapter labels, original/adapted paths, and exactly-once mapped usage/cost.
- Upstream errors are untrusted classifier input only. Client/log/trace sinks receive only stable local codes, bounded provider code, status, and trace/request ID.
- Log credential-free normalized URLs only; remove userinfo, fragments, and all query values.
- Outbound and converted-response headers use fresh allowlists; do not forward client auth/session/tenant/proxy headers or upstream cookies/auth challenges/redirect/CORS/content-encoding headers.
- Never log bodies, prompts, arguments, secrets, raw payloads, or extracted upstream error text.

## Acceptance Criteria

- [ ] Non-streaming and streaming `/v1/responses`, `/v1/messages`, and client `/v1/chat/completions` requests routed to a declared Chat candidate return original protocol shapes; applicable Compact behavior remains intact.
- [ ] Streaming target events have required fields, monotonic sequence numbers, stable IDs/indexes, correct finish mapping, and exactly one terminal event; fixed-version official OpenAI/Anthropic SDKs consume real responses.
- [ ] Function tools, forced/auto choice, `parallel_tool_calls=false`, assistant calls, fragmented arguments, long-name restoration, and tool outputs round-trip with stable identity/order.
- [ ] Ordinary Anthropic messages with gateway-injected compatibility defaults succeed; explicit thinking/signatures/provider state and unsupported tools/modalities/formats skip before transport without health penalty.
- [ ] Candidate bodies/transforms are independently rebuilt from immutable original across failover and retry.
- [ ] Legacy NULL `compatible` rows preserve client-dependent passthrough; list→edit→save retains NULL unless explicitly changed.
- [ ] Explicit protocol create/update/list round-trips through SQLite, service RPC, Tauri/Web, frontend normalization, and modal; invalid Claude/Gemini combinations are rejected.
- [ ] Diagnostics use `effective_probe_path = nonempty(action) ? normalize(action) : default_path(protocol)` while body/parser are protocol-selected only.
- [ ] Malformed/empty/cross-protocol/over-limit JSON/SSE and conversion failures never leak raw bodies or become success; failover is possible only before client-visible semantic delivery.
- [ ] Usage tests distinguish missing, partial, and explicit-zero usage and assert correct estimation/source/cost exactly once.
- [ ] Malicious error bodies/headers and query/querypair/custom-action credentials are absent from client response, SQLite, trace, and stdout.
- [ ] Existing Responses/Claude/Gemini/Compact/client-Chat behavior remains green for records without explicit Chat protocol.
- [ ] Full workspace and frontend build/runtime gates pass.

## Out of Scope

- Automatic protocol discovery or URL/action inference.
- New client endpoints or client configuration changes.
- Emulating hosted/custom tools, provider conversation state, audio/image modalities, or unsupported structured-output semantics.
- User-defined JSON transforms.
- Broad routing, pricing, billing, retry, or health redesign.
