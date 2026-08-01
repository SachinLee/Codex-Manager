# Technical Design: OMP Request-Log Session Titles

## Boundary

This change is a read-model projection for the request-log page. It does not participate in request admission, routing, upstream headers, request-log persistence, Codex session mutation, or OMP lifecycle management.

## Data flow

```text
OMP JSONL metadata (title slot + session header)
  -> service requestlog session-title index (bounded cache)
  -> requestlog/sessionTitles RPC (admin-only)
  -> service_requestlog_session_titles Tauri/Web transport
  -> serviceClient.listRequestLogSessionTitles()
  -> logs page ID -> SessionTitleRef map and title search
```

The existing `request_logs.session_id` remains the sole join key. No schema change or historical backfill is required.

## Service design

### New module

Add `crates/service/src/requestlog/session_titles.rs` and expose a small read-only API from `requestlog/mod.rs` / `lib.rs`.

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RequestLogSessionTitle {
    session_id: String,
    title: Option<String>,
    cwd: Option<String>,
    source: RequestLogSessionSource, // codex | omp
}
```

`list_request_log_session_titles(limit)` returns the union of:

1. Existing `codex_session::list_sessions_with_options(default_codex_db_path(), ...)` mapped to source `codex`.
2. OMP metadata scanned from the default OMP session root, mapped to source `omp`.

If IDs collide, `codex` wins. OMP IDs are UUIDv7 and should not collide with Codex IDs; choosing a deterministic precedence prevents display churn.

### OMP location and parser

- Resolve `HOME` / platform home, `PI_CONFIG_DIR`, and `OMP_PROFILE` consistently with OMP's default directory model. The unconfigured root is `~/.omp/agent/sessions`; a profile uses OMP's profile agent subdirectory.
- Enumerate direct regular `.jsonl` children only. Retain a verified sessions-directory handle while scanning; on Windows enumerate through that handle and open a filename relative to it with reparse-point protection. On Unix scan through the held descriptor path and open final files with `O_NOFOLLOW`. Directory-handle access that cannot be anchored safely degrades to an empty OMP index.
- Enforce bounds: maximum directory entries, file count, and bytes per metadata line. The parser reads only the bounded OMP title slot and session header, stopping at the second newline. It never reads the transcript body or uses the first user message as a fallback.
- Accept a record only when the header has a UUID-shaped bounded ID and the title slot/header title is valid UTF-8 metadata. Invalid JSON, missing metadata, I/O failures, oversized files, and permission errors are skipped without failing the RPC.

### Cache

Use a process-local, mutex-protected cache keyed by the resolved file path with `{mtime, size, parsed metadata}` and a short refresh interval aligned with the logs page (five seconds). A refresh securely enumerates the active directory and reparses only changed/new metadata entries; removed entries are pruned. The cache is never persisted.

The parser accepts an injected root / clock in tests; production resolves the root from the current user environment.

### Authorization

The new RPC is admin-only. OMP titles are user-authored local metadata; exposing a global OMP session index to member/API-key actors could reveal unrelated local sessions. Non-admin callers receive an empty list or an existing permission-shaped response, selected to match the project's RPC convention during implementation. Existing request-log authorization remains unchanged.

## RPC and transport contract

| Layer | Contract |
|---|---|
| Service RPC | `requestlog/sessionTitles`, optional `{ limit?: number }`, returns `RequestLogSessionTitle[]` |
| Tauri command | `service_requestlog_session_titles(addr?, limit?)` forwarding to the RPC |
| Tauri registry | Registers the command |
| Web transport | `service_requestlog_session_titles -> requestlog/sessionTitles` |
| Typed frontend client | `serviceClient.listRequestLogSessionTitles({ limit? })` |
| UI | Replaces logs-page-only `codexLauncherClient.listSessions()` title lookup |

The existing `codexSession/*` RPCs remain untouched so OMP records cannot reach delete, move, archive, or undo operations.

## UI behavior

The logs page uses the response for both the table map and `session_title` search. `SessionInfoCell` receives the same shape needed today (`sessionId`, `title`, `cwd`); source may be shown in the tooltip only if it fits existing presentation patterns. Unmatched IDs retain the current fallback text.

## Failure and compatibility behavior

| Condition | Result |
|---|---|
| OMP root absent/unreadable | Codex titles still return; OMP IDs are unmatched |
| OMP title generated/renamed after a request | Becomes visible after cache/UI refresh |
| Invalid / partially written JSONL | Skip only that file |
| Missing request-log session ID | No inference, no backfill |
| HTTP, WebSocket, Aggregate API logs | No behavioral change; their existing session-ID persistence remains the prerequisite |
| Remote service host | Reads only that host's OMP directory; no remote transcript access |

## Security and privacy invariants

- No transcript message, prompt, tool input/output, secret, or raw JSONL body is returned or persisted.
- The resolver is read-only and does not alter OMP files.
- OMP title data is not forwarded upstream.
- Non-admin actors must not receive global OMP title metadata.

## Rollback

Remove the title-index lookup call or disable the OMP source. Existing logs remain valid because the only persistent join key is the unchanged `session_id`.
