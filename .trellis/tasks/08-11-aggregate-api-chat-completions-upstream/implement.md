# Implementation — Aggregate API Chat Completions upstream compatibility

## Phase 1 — configuration contract

1. Add nullable migration and compatibility column creation.
2. Extend core structs, SQL, row mapping, CRUD, and fixtures.
3. Validate protocol/provider combinations consistently; preserve NULL raw values and runtime effective defaults.
4. Synchronize service/RPC/Tauri/Web/frontend types and modal state.
5. Add migration, legacy-compatible routing, invalid-combination, and create/update/list round-trip tests.

## Phase 2 — shared request conversion and candidate planning

1. Extract/upgrade existing request-rewrite conversion into one typed pure converter; migrate client-Chat/Compact callers.
2. Add upstream protocol resolution and bounded incompatibility reasons.
3. Map messages, tools, tool calls/results, `parallel_tool_calls`, limits, reasoning, format, and stream usage.
4. Preserve provenance for injected vs client-required state.
5. Integrate candidate URL/body/header construction from immutable originals.
6. Carry two-axis `ResponsePlan` and `ToolNameRestoreMap` through Aggregate execution.
7. Test mappings, unsupported semantics, long names, model overrides, default/custom action, and candidate isolation.

## Phase 3 — non-streaming conversion

1. Implement Chat JSON -> `Result<CanonicalResponses, AdapterFailure>`.
2. Reuse canonical client encoders for Responses, Anthropic, Chat, and Compact.
3. Define finish/usage/missing-vs-zero mapping and converted-response header allowlist.
4. Test success, function tools, ordinary Anthropic defaults, explicit thinking rejection, malformed/empty/cross-protocol JSON, error sanitization, and missing/partial/zero usage.

## Phase 4 — streaming conversion and preflight

1. Implement bounded Chat SSE decoder/state machine and semantic event model.
2. Add Aggregate protocol-aware preflight before headers/request ownership.
3. Emit and encode target lifecycle/text/reasoning/tool/usage events with stable fields, sequence numbers, IDs, and one terminal.
4. Cover finish reasons, usage-only frames, fragmented/interleaved tools, `[DONE]`, malformed/over-limit/mixed frames, EOF, disconnects, and strict tool-input parsing.
5. Prove pre-delivery failover and post-delivery no replay for every client encoder.
6. Validate real HTTP/SSE with fixed-version official OpenAI and Anthropic SDKs.

## Phase 5 — diagnostics, capability, observability, UI

1. Use actual upstream protocol in capability facts/evidence and bounded adapter labels.
2. Preserve credential-free original/adapted paths and sanitize errors/headers/URLs at sinks.
3. Feed mapped usage into existing billing exactly once.
4. Make diagnostics honor declared protocol plus explicit action path override.
5. Add selector, effective default display, NULL-preserving edit behavior, invalid-provider validation, and create/edit browser coverage.

## Phase 6 — verification and cleanup

1. Run focused core/service gateway/HTTP bridge/request-log/RPC/frontend tests.
2. Run local mock smoke matrix: Responses, Anthropic Messages, client Chat, and applicable Compact; stream/non-stream; tools; pre/post-delivery failures.
3. Run workspace tests and frontend runtime/build/desktop gates.
4. Review adapter loss, terminal duplication, silent drops, health penalties, limits, header/error/URL leaks, billing source, and legacy rows.
5. Update user-facing localized docs if the selector is documented.
6. Remove obsolete action-substring runtime inference and temporary scaffolding only after smoke success.

## Validation commands

```powershell
cargo test -p codexmanager-core aggregate_api
cargo test -p codexmanager-core migration
cargo test -p codexmanager-service protocol_adapter
cargo test -p codexmanager-service aggregate_api
cargo test -p codexmanager-service http_bridge
cargo test -p codexmanager-service request_log
cargo test -p codexmanager-web
cargo test --workspace
pnpm -C apps run test:runtime
pnpm -C apps run build
pnpm -C apps run build:desktop
```

The smoke harness must start the real service/web runtime with deterministic local Chat mock and exercise: Responses JSON/SSE, Anthropic Messages JSON/SSE, client Chat JSON/SSE, applicable Compact behavior, fragmented function calls and usage, pre-delivery failover, post-delivery failure without replay, malicious error/header/query credentials, and official SDK consumption.

## Rollback

Schema is additive/nullable. Old binaries ignore the field; new binaries preserve NULL legacy behavior. Rollback sets affected candidates to NULL legacy mode, explicit Responses, or disabled; no schema rollback and no automatic Responses↔Chat fallback.

## Review gates

- Protocol fidelity: complete target wire contracts, no silent required-field loss, exact lifecycle/tool/finish ordering.
- Client transparency: original response protocol and tool-name mapping survive Aggregate routing.
- Data integrity: additive migration, NULL raw/effective distinction, and legacy compatible passthrough.
- Reliability: preflight boundary, no retry after delivery, malformed streams cannot become success.
- Billing: missing vs explicit-zero usage semantics and exactly-once normalization.
- Security/privacy: fresh header maps, sanitized URLs, stable errors only, no secrets/prompts/arguments/raw bodies.
- API parity: storage, service, RPC, Tauri, Web, frontend types, normalizer, and UI synchronized.
- Maintainability: one shared request converter, one canonical response/event model, no duplicated Chat→Responses and Chat→Anthropic state machines.
