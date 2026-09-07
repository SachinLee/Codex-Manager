# Design: Aggregate API Session Affinity for Cache Stability

## Overview

This design implements session-level Aggregate API source affinity to reduce cache churn during failover. It reuses the existing `cache_affinity_route_id` partitioning (platform_key_hash + protocol + model + affinity key source), stores successful Aggregate API bindings, and reorders candidates to prefer the last successful source while respecting health/balance/capability filters.

## Storage Schema

### New Table: `aggregate_api_bindings`

```sql
CREATE TABLE IF NOT EXISTS aggregate_api_bindings (
    platform_key_hash TEXT NOT NULL,
    protocol_type TEXT NOT NULL,
    model TEXT NOT NULL,
    cache_affinity_route_id_hash TEXT NOT NULL,
    bound_aggregate_api_id TEXT NOT NULL,
    bound_at INTEGER NOT NULL,
    reason TEXT,
    PRIMARY KEY (platform_key_hash, protocol_type, model, cache_affinity_route_id_hash)
);

CREATE INDEX IF NOT EXISTS idx_aggregate_api_bindings_cleanup
    ON aggregate_api_bindings(bound_at DESC);
```

**Rationale**:
- Composite PK ensures one binding per (key identity + protocol + model + affinity ID).
- `cache_affinity_route_id_hash`: SHA256 of the existing `cache_affinity_route_id()` output; avoids storing raw session_id or prompt_cache_key.
- `bound_at`: binding creation timestamp for TTL-based cleanup.
- `reason`: optional audit string ("initial_success", "failover_convergence"); never contains secrets.

**TTL Policy**: Default 24 hours matching prompt cache behavior. Cleanup via periodic maintenance task; configurable upper bound (e.g., 7 days max).

### Request Log Extension

Add column to `request_logs`:

```sql
ALTER TABLE request_logs ADD COLUMN aggregate_api_attempts TEXT;
```

**Schema** (JSON array):
```json
[
  {
    "api_id": "agg-openai-primary",
    "position": 1,
    "outcome": "failure",
    "category": "rate_limit_exceeded"
  },
  {
    "api_id": "agg-anthropic-fallback",
    "position": 2,
    "outcome": "success",
    "category": null
  }
]
```

**Backward Compatibility**: Existing `attempted_aggregate_api_ids_json` remains; new column populated when affinity is enabled. Readers tolerate NULL.

**Failure Categories** (normalized from diverse upstream errors):
- `rate_limit_exceeded`
- `upstream_unavailable`
- `transport_error`
- `timeout`
- `authentication_error`
- `unknown_error`

Mapping extracted from error classification logic in `aggregate_api.rs:2406-2550`.

## Configuration

### App Setting

New row in `app_settings`:

```sql
INSERT INTO app_settings (key, value, updated_at)
VALUES ('aggregate_api_session_affinity_enabled', '0', <timestamp>)
ON CONFLICT (key) DO NOTHING;
```

- `"1"` = enabled
- `"0"` = disabled (default)

Read in gateway hot path via `Storage::get_app_setting()`.

## Data Flow

### 1. Affinity-Enabled Request Entry

**Location**: `crates/service/src/gateway/upstream/proxy.rs:resolve_aggregate_candidates_for_route` (line 381)

**Current behavior**: Returns filtered/ordered Aggregate API candidates.

**New behavior**:
1. After existing candidate filtering (cooldown, balance, capability), check if:
   - Setting enabled
   - `aggregate_api_id` is `None` (not explicitly specified)
   - Cache affinity route ID available
2. If yes, call new helper `reorder_candidates_with_affinity(storage, platform_key_hash, protocol_type, model, cache_affinity_route_id_hash, candidates)`.
3. Helper:
   - Looks up `bound_aggregate_api_id` from `aggregate_api_bindings`.
   - If found and `bound_aggregate_api_id` exists in `candidates`, moves it to position 0.
   - Returns reordered candidates.

**Integration point**:
```rust
// In resolve_aggregate_candidates_for_route, after line 720:
if should_apply_aggregate_api_affinity(storage, aggregate_api_id) {
    let affinity_route_id = /* derive from request context */;
    let affinity_hash = sha256_hex(affinity_route_id.as_bytes());
    candidates = reorder_candidates_with_affinity(
        storage,
        platform_key_hash,
        protocol_type,
        model_for_log,
        &affinity_hash,
        candidates,
    )?;
}
```

### 2. Attempt Loop Tracking

**Location**: `crates/service/src/gateway/upstream/protocol/aggregate_api.rs:2008-2320` (candidate iteration loop)

**Current behavior**: Iterates candidates, records success/failure to memory cooldown and health system.

**New behavior**:
- Track each attempt in a thread-local or request-scoped vector:
  ```rust
  struct AggregateApiAttempt {
      api_id: String,
      position: usize,
      outcome: AttemptOutcome, // Success, Failure, Skipped
      failure_category: Option<String>,
  }
  ```
- On candidate skip (cooldown, balance block): record `Skipped`.
- On transport/upstream error: classify into failure category and record `Failure`.
- On success: record `Success`.

### 3. Binding Update on Success

**Location**: `crates/service/src/gateway/upstream/protocol/aggregate_api.rs:2760-2800` (after successful bridge delivery)

**New behavior**:
1. Check if affinity enabled and affinity route ID available.
2. If final successful `candidate.id` differs from initial bound source (or no prior binding), upsert:
   ```rust
   storage.upsert_aggregate_api_binding(AggregateApiBinding {
       platform_key_hash,
       protocol_type,
       model,
       cache_affinity_route_id_hash,
       bound_aggregate_api_id: candidate.id.clone(),
       bound_at: now_timestamp(),
       reason: Some(if initial_binding { "initial_success" } else { "failover_convergence" }),
   })?;
   ```

### 4. Request Log Serialization

**Location**: `crates/service/src/gateway/trace_log.rs` (request finalization)

**New behavior**:
- Serialize `Vec<AggregateApiAttempt>` to JSON.
- Write to `request_logs.aggregate_api_attempts`.
- Preserve `attempted_aggregate_api_ids_json` for backward compatibility (extract IDs from attempts).

## Gateway Module Organization

### New Modules

1. **`crates/core/src/storage/aggregate_api_bindings.rs`**
   - `get_aggregate_api_binding(platform_key_hash, protocol, model, affinity_hash) -> Option<AggregateApiBinding>`
   - `upsert_aggregate_api_binding(binding)`
   - `delete_stale_aggregate_api_bindings(before_timestamp)`

2. **`crates/service/src/gateway/routing/aggregate_api_affinity.rs`**
   - `should_apply_aggregate_api_affinity(storage, explicit_api_id) -> bool`
   - `reorder_candidates_with_affinity(storage, ..., candidates) -> Vec<AggregateApi>`
   - `derive_affinity_route_hash(route_id: &str) -> String` (SHA256 hex)

### Modified Modules

1. **`crates/service/src/gateway/upstream/proxy.rs`**
   - Call affinity reorder in `resolve_aggregate_candidates_for_route` after line 720.

2. **`crates/service/src/gateway/upstream/protocol/aggregate_api.rs`**
   - Track attempts in candidate loop (lines 2008-2320).
   - Upsert binding on success (after line 2760).

3. **`crates/service/src/gateway/trace_log.rs`**
   - Extend `log_request_final` to accept attempt summary and serialize to new column.

## Alternatives Considered

### Alternative 1: Retry/Wait on Transient Failures

**Rejected**: Data shows primary cause is persistent source unavailability (e.g., `sort=105` zero success in 35 attempts) and rate limits, not short transient blips. Adding retry delay would worsen latency without cache recovery guarantee.

### Alternative 2: Cross-Provider Cache Key Alignment

**Rejected**: OpenAI documentation does not guarantee cache sharing across different API keys or accounts. Experimentation would require upstream cooperation and risks leaking session data across security boundaries.

### Alternative 3: Store Raw `session_id`/`prompt_cache_key`

**Rejected**: Violates data minimization. Existing `cache_affinity_route_id` already provides stable hashed partition; storing raw identifiers adds no routing value and increases audit surface.

### Alternative 4: Always Bind on First Request

**Rejected**: Would interfere with existing `ordered`/`balanced` strategies when affinity is disabled. Opt-in design preserves current behavior as default.

## Data Flow Diagram

```
┌──────────────────────────────────────────────────────────────────┐
│ 1. Request arrives with cache-affinity route ID                  │
│    (derived from session_id or prompt_cache_key)                 │
└────────────────────────┬─────────────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────────────┐
│ 2. resolve_aggregate_candidates_for_route                        │
│    - Filter by cooldown, balance, capability                     │
│    - If affinity enabled: lookup binding                         │
│    - If bound source in candidates: move to position 0           │
└────────────────────────┬─────────────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────────────┐
│ 3. Candidate attempt loop (aggregate_api.rs)                     │
│    - Try position 0 (bound source if affinity hit)               │
│    - On failure: record attempt + fallback to next candidate     │
│    - On success: record attempt + proceed                        │
└────────────────────────┬─────────────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────────────┐
│ 4. Success: upsert binding if final source ≠ initial binding     │
│    - Future requests from this affinity ID start with new source │
└────────────────────────┬─────────────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────────────┐
│ 5. Log finalization: serialize attempts to request_logs          │
│    - aggregate_api_attempts: structured JSON                     │
│    - attempted_aggregate_api_ids_json: backward compat           │
└──────────────────────────────────────────────────────────────────┘
```

## Security and Privacy

- **No raw session IDs**: Only hashed route ID stored.
- **No prompts**: Binding table contains only API IDs and timestamps.
- **No secrets**: Failure categories are normalized; raw upstream errors remain in local trace log.
- **Partition by key identity**: Different API keys never share bindings.

## Observability

**During Development**: Existing `gateway-trace.log` events augmented:
- `event=AGGREGATE_AFFINITY_HIT` when binding found and source reordered.
- `event=AGGREGATE_AFFINITY_MISS` when no binding or bound source unavailable.
- `event=AGGREGATE_AFFINITY_REBIND` when failover success updates binding.

**Production**: `request_logs.aggregate_api_attempts` provides structured post-hoc analysis without introducing new metrics endpoints.

## Testing Strategy

### Storage Tests (`crates/core/src/storage/tests/aggregate_api_bindings_tests.rs`)

1. **Binding CRUD**
   - Upsert creates new binding.
   - Upsert updates existing binding (same PK).
   - Lookup returns bound API ID.
   - Cleanup deletes stale bindings (bound_at < threshold).

2. **Partitioning**
   - Same affinity hash + different protocol → separate bindings.
   - Same affinity hash + different model → separate bindings.
   - Same affinity hash + different key hash → separate bindings.

### Gateway Tests (`crates/service/src/gateway/upstream/proxy_tests.rs`)

1. **Affinity-Enabled Source Reordering**
   - Given: binding exists, bound source in candidates
   - When: resolve candidates with affinity enabled
   - Then: bound source at position 0

2. **Affinity Miss: No Binding**
   - Given: no binding exists
   - When: resolve candidates
   - Then: original order preserved

3. **Affinity Miss: Bound Source Unavailable**
   - Given: binding exists, bound source in cooldown
   - When: resolve candidates
   - Then: bound source excluded by filter, next candidate first

4. **Failover Rebinding**
   - Given: binding to source A, source A fails, source B succeeds
   - When: next request
   - Then: binding updated to B, next attempt starts with B

5. **Affinity Disabled Path**
   - Given: setting OFF
   - When: resolve candidates
   - Then: original order, no binding lookup

### Integration Test Scenario

Simulate 3-source failover:
1. Request 1: No binding, sources ordered [A, B, C], A succeeds → bind A.
2. Request 2: Binding A, reorder [A, B, C], A succeeds → no rebind.
3. Request 3: Binding A, A in cooldown, reorder excludes A → [B, C], B succeeds → rebind B.
4. Request 4: Binding B, reorder [B, A, C] (A recovered), B succeeds.

**Verification**: Request 2 and 4 hit bound source on first attempt; request 3 triggers rebind.

## Rollback and Safety

- **Migration is additive**: New table and column; existing queries unaffected.
- **Default OFF**: No behavior change until setting toggled.
- **Settings toggle**: Instant disable via `app_settings` update; no code deploy needed.
- **No breaking changes**: Existing `attempted_aggregate_api_ids_json` continues to work.

## Performance Considerations

- **Binding lookup**: Single indexed query per affinity-enabled request; negligible compared to upstream latency.
- **Binding upsert**: Single write on successful response; already in transaction path.
- **Attempt tracking**: In-memory vector during request; serialized once at finalization.
- **Cleanup**: Periodic batch delete of stale bindings; not in hot path.

## Open Questions

None. All PRD acceptance criteria mapped to implementation points.
