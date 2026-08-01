# Implementation Plan: OMP Request-Log Session Titles

## Preconditions

- Implement only after approval of this plan and `task.py start`.
- Preserve the existing request-log `session_id` contract; do not alter request forwarding, schema, or OMP.
- Add no new dependency unless the workspace already has the required capability; prefer bounded `std::fs` traversal.

## Ordered work

1. **Service read model and tests**
   - Add `requestlog/session_titles.rs` with serializable source/type definitions, OMP root resolution, bounded metadata-prefix parser, cache, and Codex+OMP union.
   - Keep path traversal testable through injected roots; never scan actual user files in tests.
   - Add fixtures/tests for valid title-slot/header metadata, header-title fallback if supported, no title, malformed JSON, non-JSONL, symlink skipping, missing root, stale cache refresh, cache pruning, union precedence, result limit, and no transcript fallback.

2. **RPC authorization and dispatch**
   - Add `requestlog/sessionTitles` to `rpc_dispatch/requestlog.rs`, enforce the chosen admin-only behavior, and add dispatch coverage.
   - Export only the narrow read function; do not add it to `codex_session` mutation APIs.
   - Add the new RPC name to the declared-method surface if the dispatcher maintains one.

3. **Desktop and web transport**
   - Add `service_requestlog_session_titles` to `apps/src-tauri/src/commands/requestlog.rs` and command registry.
   - Add the Web command mapping in `apps/src/lib/api/transport-web-commands/misc.ts`.
   - Add a typed `RequestLogSessionTitle` and `serviceClient.listRequestLogSessionTitles()` wrapper using `withAddr()`.

4. **Logs-page integration**
   - Replace the logs-page-only `codexLauncherClient.listSessions({ limit: 2000 })` lookup with the request-log session-title RPC.
   - Use the same title list for the ID map and `buildRequestLogSearchQuery`.
   - Preserve existing Codex session management hooks/pages; they continue using `codexLauncherClient`.
   - Keep present unmatched/no-title UI fallbacks and optionally expose source only in the existing tooltip.

5. **Cross-layer tests and validation**
   - Run narrow Rust tests for the new module and RPC behavior.
   - Run the request-log/session regression tests relevant to HTTP, WebSocket, and Aggregate API session ID finalization.
   - Run `pnpm -C apps run build` and `pnpm -C apps run test:runtime` because the UI/transport contract changes.
   - Smoke-test the local service with an OMP session fixture or live OMP request: verify matching `request_logs.session_id` yields the title, then rename the title and verify refresh behavior.

## Risk gates

- Do not merge OMP entries into `codexSession/list`; this would expose them to destructive session operations.
- Do not include an OMP title for non-admin RPC actors.
- Do not use the first message as a title fallback; that would parse user content and violate the privacy boundary.
- Do not consider `prompt_cache_key` a title source. It remains only the pre-existing request-log session-ID fallback.
- Do not consider successful compilation proof of title lookup. Verify an ID-to-title match and failure degradation.

## Rollback point

After service tests but before transport wiring, the resolver is isolated and removable. After transport wiring, reverting the one UI query restores the prior behavior without database rollback.
