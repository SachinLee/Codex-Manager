# Implementation Plan: Aggregate API Session Affinity

## Overview

Ordered implementation of session-level Aggregate API source affinity to reduce cache churn during failover. Each phase builds on the previous and includes verification before proceeding.

## Phase 1: Storage Foundation

### 1.1 Migration

**File**: `crates/core/migrations/138_aggregate_api_bindings.sql`

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

**Verification**: `cargo test -p codexmanager-core` migration tests pass.

### 1.2 Storage Helpers

**File**: `crates/core/src/storage/aggregate_api_bindings.rs`

**Struct**:
```rust
#[derive(Debug, Clone)]
pub struct AggregateApiBinding {
    pub platform_key_hash: String,
    pub protocol_type: String,
    pub model: String,
    pub cache_affinity_route_id_hash: String,
    pub bound_aggregate_api_id: String,
    pub bound_at: i64,
    pub reason: Option<String>,
}
```

**Methods** (add to `impl Storage`):

1. **`get_aggregate_api_binding`**
   ```rust
   pub fn get_aggregate_api_binding(
       &self,
       platform_key_hash: &str,
       protocol_type: &str,
       model: &str,
       cache_affinity_route_id_hash: &str,
   ) -> rusqlite::Result<Option<AggregateApiBinding>>
   ```
   - SQL: SELECT with 4-part WHERE matching PK.
   - Returns `Some(binding)` on hit, `None` on miss.

2. **`upsert_aggregate_api_binding`**
   ```rust
   pub fn upsert_aggregate_api_binding(
       &self,
       binding: &AggregateApiBinding,
   ) -> rusqlite::Result<()>
   ```
   - SQL: INSERT ... ON CONFLICT DO UPDATE.
   - Updates `bound_aggregate_api_id`, `bound_at`, `reason` on conflict.

3. **`delete_stale_aggregate_api_bindings`**
   ```rust
   pub fn delete_stale_aggregate_api_bindings(
       &self,
       before_timestamp: i64,
   ) -> rusqlite::Result<usize>
   ```
   - SQL: DELETE WHERE bound_at < ?1.
   - Returns affected row count.

**Module registration**: Add `pub mod aggregate_api_bindings;` to `crates/core/src/storage/mod.rs` and re-export struct.

**Verification**:
```bash
cargo test -p codexmanager-core storage::aggregate_api_bindings
```

### 1.3 Storage Tests

**File**: `crates/core/src/storage/tests/aggregate_api_bindings_tests.rs`

**Test cases**:

1. **`upsert_creates_new_binding`**
   - Insert binding, verify get returns it.

2. **`upsert_updates_existing_binding`**
   - Insert binding A, upsert with new api_id, verify get returns updated.

3. **`binding_partitioned_by_protocol`**
   - Upsert same hash + model but different protocol, verify separate bindings.

4. **`binding_partitioned_by_model`**
   - Upsert same hash + protocol but different model, verify separate bindings.

5. **`binding_partitioned_by_key_hash`**
   - Upsert same affinity hash but different platform_key_hash, verify separate.

6. **`cleanup_deletes_stale_bindings`**
   - Insert bindings with old bound_at, call cleanup, verify deleted.

**Run**: `cargo test -p codexmanager-core aggregate_api_bindings_tests`

---

## Phase 2: Configuration

### 2.1 App Setting

**File**: `crates/core/migrations/139_aggregate_api_session_affinity_setting.sql`

```sql
INSERT INTO app_settings (key, value, updated_at)
VALUES ('aggregate_api_session_affinity_enabled', '0', (strftime('%s', 'now')))
ON CONFLICT (key) DO NOTHING;
```

**Verification**: Existing `get_app_setting` tests cover retrieval; manual check via sqlite3.

### 2.2 Gateway Setting Reader

**File**: `crates/service/src/gateway/mod.rs` (or new `settings.rs`)

**Function**:
```rust
pub(crate) fn aggregate_api_session_affinity_enabled(storage: &Storage) -> bool {
    storage
        .get_app_setting("aggregate_api_session_affinity_enabled")
        .ok()
        .flatten()
        .and_then(|v| v.parse::<i32>().ok())
        .map(|v| v != 0)
        .unwrap_or(false)
}
```

**Verification**: Unit test with mock storage returning "0", "1", None.

---

## Phase 3: Gateway Affinity Logic

### 3.1 Affinity Routing Module

**File**: `crates/service/src/gateway/routing/aggregate_api_affinity.rs`

**Functions**:

1. **`should_apply_aggregate_api_affinity`**
   ```rust
   pub(crate) fn should_apply_aggregate_api_affinity(
       storage: &Storage,
       explicit_aggregate_api_id: Option<&str>,
   ) -> bool {
       explicit_aggregate_api_id.is_none()
           && super::super::aggregate_api_session_affinity_enabled(storage)
   }
   ```

2. **`derive_affinity_route_hash`**
   ```rust
   pub(crate) fn derive_affinity_route_hash(route_id: &str) -> String {
       use sha2::{Digest, Sha256};
       let mut hasher = Sha256::new();
       hasher.update(route_id.as_bytes());
       format!("{:x}", hasher.finalize())
   }
   ```

3. **`reorder_candidates_with_affinity`**
   ```rust
   pub(crate) fn reorder_candidates_with_affinity(
       storage: &Storage,
       platform_key_hash: &str,
       protocol_type: &str,
       model: Option<&str>,
       cache_affinity_route_id_hash: &str,
       mut candidates: Vec<AggregateApi>,
   ) -> Result<Vec<AggregateApi>, String> {
       let model = model.unwrap_or("");
       let binding = storage
           .get_aggregate_api_binding(
               platform_key_hash,
               protocol_type,
               model,
               cache_affinity_route_id_hash,
           )
           .map_err(|e| format!("affinity binding lookup failed: {e}"))?;
       
       if let Some(binding) = binding {
           if let Some(pos) = candidates.iter().position(|c| c.id == binding.bound_aggregate_api_id) {
               let bound_candidate = candidates.remove(pos);
               candidates.insert(0, bound_candidate);
           }
       }
       Ok(candidates)
   }
   ```

**Module registration**: Add `pub mod aggregate_api_affinity;` to `crates/service/src/gateway/routing/mod.rs`.

### 3.2 Integration into Candidate Resolution

**File**: `crates/service/src/gateway/upstream/proxy.rs`

**Location**: After line 720 in `resolve_aggregate_candidates_for_route`

**Code**:
```rust
// After existing candidate filtering
if super::super::routing::aggregate_api_affinity::should_apply_aggregate_api_affinity(
    storage,
    aggregate_api_id,
) {
    // Derive affinity route ID from request context
    // (This requires passing cache_affinity_route_id through function signatures
    //  or re-deriving it here with the same inputs used in conversation_binding.rs)
    if let Some(affinity_route_id) = /* TODO: pass from caller or re-derive */ {
        let affinity_hash = super::super::routing::aggregate_api_affinity::derive_affinity_route_hash(
            affinity_route_id.as_str()
        );
        candidates = super::super::routing::aggregate_api_affinity::reorder_candidates_with_affinity(
            storage,
            /* platform_key_hash from caller context */,
            protocol_type,
            model_for_log,
            &affinity_hash,
            candidates,
        )?;
    }
}
```

**Context threading**: The `cache_affinity_route_id` is currently derived in `local_validation/request.rs:1615` and `1638`. To avoid duplication:
- **Option A**: Pass `cache_affinity_route_id` through `proxy_validated_request` → `resolve_aggregate_candidates_for_route`.
- **Option B**: Re-derive it in `resolve_aggregate_candidates_for_route` using the same inputs (requires passing `RouteConversationId` or its components).

**Recommended**: Option A for consistency; add parameter to `resolve_aggregate_candidates_for_route`.

**Verification**: Unit test in `proxy_tests.rs`:
- Mock storage with binding, verify bound candidate moved to position 0.
- Mock storage without binding, verify order unchanged.

---

## Phase 4: Attempt Tracking and Binding Update

### 4.1 Attempt Tracking Structure

**File**: `crates/service/src/gateway/upstream/protocol/aggregate_api.rs`

**Add near top**:
```rust
#[derive(Debug, Clone)]
pub(crate) struct AggregateApiAttemptRecord {
    pub api_id: String,
    pub position: usize,
    pub outcome: AttemptOutcome,
    pub failure_category: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttemptOutcome {
    Success,
    Failure,
    Skipped,
}
```

### 4.2 Attempt Loop Instrumentation

**Location**: `aggregate_api.rs:2008-2320` (candidate iteration loop)

**Changes**:

1. **Initialize tracking vector before loop**:
   ```rust
   let mut attempt_records: Vec<AggregateApiAttemptRecord> = Vec::new();
   ```

2. **Record skipped candidates** (when cooldown, balance, or capability filter excludes):
   ```rust
   attempt_records.push(AggregateApiAttemptRecord {
       api_id: candidate.id.clone(),
       position: candidate_idx + 1,
       outcome: AttemptOutcome::Skipped,
       failure_category: Some("cooldown"), // or "zero_balance", "capability_mismatch"
   });
   continue;
   ```

3. **Record failed attempts** (after transport or upstream error classification around line 2406):
   ```rust
   let failure_category = classify_aggregate_api_failure(&err); // new helper
   attempt_records.push(AggregateApiAttemptRecord {
       api_id: candidate.id.clone(),
       position: candidate_idx + 1,
       outcome: AttemptOutcome::Failure,
       failure_category: Some(failure_category),
   });
   ```

4. **Record success** (after successful bridge delivery around line 2760):
   ```rust
   attempt_records.push(AggregateApiAttemptRecord {
       api_id: candidate.id.clone(),
       position: candidate_idx + 1,
       outcome: AttemptOutcome::Success,
       failure_category: None,
   });
   ```

5. **Return attempt records** with the outcome:
   - Extend `AggregateAttemptOutcome::Responded` variant to include `attempt_records: Vec<AggregateApiAttemptRecord>`.

### 4.3 Failure Classification Helper

**Function** (add to `aggregate_api.rs`):
```rust
fn classify_aggregate_api_failure(error_message: &str) -> String {
    if error_message.contains("rate_limit_exceeded") || error_message.contains("rate-limited") {
        "rate_limit_exceeded".to_string()
    } else if error_message.contains("temporarily unavailable") || error_message.contains("503") {
        "upstream_unavailable".to_string()
    } else if error_message.contains("timeout") {
        "timeout".to_string()
    } else if error_message.contains("Transport error") || error_message.contains("connect") {
        "transport_error".to_string()
    } else if error_message.contains("authentication") || error_message.contains("401") {
        "authentication_error".to_string()
    } else {
        "unknown_error".to_string()
    }
}
```

### 4.4 Binding Update on Success

**Location**: After successful response, around `aggregate_api.rs:2760`

**Code**:
```rust
// After recording success attempt
if super::super::routing::aggregate_api_affinity::should_apply_aggregate_api_affinity(storage, /* explicit_api_id */) {
    if let Some(affinity_route_id_hash) = /* context from caller */ {
        let final_api_id = &candidate.id;
        // Check if binding exists and differs
        let needs_update = storage
            .get_aggregate_api_binding(platform_key_hash, protocol_type, model, affinity_route_id_hash)
            .ok()
            .flatten()
            .map(|b| b.bound_aggregate_api_id != *final_api_id)
            .unwrap_or(true); // No binding = create
        
        if needs_update {
            let binding = AggregateApiBinding {
                platform_key_hash: platform_key_hash.to_string(),
                protocol_type: protocol_type.to_string(),
                model: model.unwrap_or("").to_string(),
                cache_affinity_route_id_hash: affinity_route_id_hash.to_string(),
                bound_aggregate_api_id: final_api_id.clone(),
                bound_at: chrono::Utc::now().timestamp(),
                reason: Some(if initial_binding { "initial_success" } else { "failover_convergence" }),
            };
            let _ = storage.upsert_aggregate_api_binding(&binding); // Log error, don't fail request
        }
    }
}
```

**Context threading**: Requires passing `platform_key_hash`, `protocol_type`, `model`, `affinity_route_id_hash` to the aggregate_api attempt function.

---

## Phase 5: Request Log Extension

### 5.1 Migration

**File**: `crates/core/migrations/140_request_logs_aggregate_api_attempts.sql`

```sql
ALTER TABLE request_logs ADD COLUMN aggregate_api_attempts TEXT;
```

### 5.2 Serialization Helper

**File**: `crates/service/src/gateway/trace_log.rs` (or new `request_log_helpers.rs`)

**Function**:
```rust
pub(crate) fn serialize_aggregate_api_attempts(
    attempts: &[super::upstream::protocol::aggregate_api::AggregateApiAttemptRecord],
) -> String {
    serde_json::to_string(&attempts.iter().map(|a| {
        serde_json::json!({
            "api_id": a.api_id,
            "position": a.position,
            "outcome": match a.outcome {
                super::upstream::protocol::aggregate_api::AttemptOutcome::Success => "success",
                super::upstream::protocol::aggregate_api::AttemptOutcome::Failure => "failure",
                super::upstream::protocol::aggregate_api::AttemptOutcome::Skipped => "skipped",
            },
            "category": a.failure_category,
        })
    }).collect::<Vec<_>>()).unwrap_or_else(|_| "[]".to_string())
}
```

### 5.3 Log Finalization Update

**Location**: `trace_log.rs` function that writes final request log (likely `log_request_final` or similar)

**Changes**:
1. Add parameter `aggregate_api_attempts: Option<Vec<AggregateApiAttemptRecord>>`.
2. Serialize and insert into `aggregate_api_attempts` column:
   ```rust
   let attempts_json = aggregate_api_attempts
       .as_ref()
       .map(|a| serialize_aggregate_api_attempts(a))
       .unwrap_or_else(|| "null".to_string());
   // Include in INSERT statement
   ```

3. **Backward compatibility**: Continue populating `attempted_aggregate_api_ids_json` by extracting IDs from `attempts`.

**Verification**: Manual test with affinity enabled, verify `aggregate_api_attempts` column contains structured JSON.

---

## Phase 6: Testing

### 6.1 Storage Tests

Already covered in Phase 1.3.

### 6.2 Gateway Integration Tests

**File**: `crates/service/src/gateway/upstream/proxy_tests.rs`

**Test cases**:

1. **`affinity_reorders_bound_candidate_to_first`**
   - Setup: Mock storage with binding to "agg-api-2", candidates ["agg-api-1", "agg-api-2", "agg-api-3"].
   - Enable affinity setting.
   - Resolve candidates.
   - Assert: order is ["agg-api-2", "agg-api-1", "agg-api-3"].

2. **`affinity_preserves_order_when_no_binding`**
   - Setup: No binding, candidates ["agg-api-1", "agg-api-2"].
   - Enable affinity.
   - Assert: order unchanged.

3. **`affinity_preserves_order_when_bound_source_unavailable`**
   - Setup: Binding to "agg-api-cooldown", candidates after filter = ["agg-api-1", "agg-api-2"] (bound source excluded).
   - Enable affinity.
   - Assert: order unchanged (bound source not in candidates).

4. **`affinity_disabled_preserves_order`**
   - Setup: Binding exists, setting OFF.
   - Assert: order unchanged.

5. **`failover_updates_binding_on_success`**
   - Setup: Binding to "agg-api-A", mock "agg-api-A" failure + "agg-api-B" success.
   - After request completion, query binding.
   - Assert: binding updated to "agg-api-B".

6. **`binding_partitioned_by_model`**
   - Setup: Binding for model "gpt-5.4" to "agg-api-1".
   - Resolve candidates for model "gpt-5.5".
   - Assert: no reorder (different model partition).

**Run**: `cargo test -p codexmanager-service proxy_tests`

### 6.3 End-to-End Simulation

**Manual test scenario**:

1. Enable setting: `UPDATE app_settings SET value = '1' WHERE key = 'aggregate_api_session_affinity_enabled';`
2. Configure 3 Aggregate APIs: A (primary), B (fallback), C (tertiary).
3. Send request with `session_id=test-session-1`, model `gpt-5.6-terra`:
   - Expected: No binding, A first, A succeeds → binding created.
4. Send second request same session:
   - Expected: Binding found, A reordered to first, A succeeds.
5. Put A in cooldown (simulate by disabling or manual health record).
6. Send third request same session:
   - Expected: A skipped, B first, B succeeds → binding updated to B.
7. Send fourth request same session:
   - Expected: B reordered to first, B succeeds.

**Verification**:
```sql
SELECT * FROM aggregate_api_bindings WHERE cache_affinity_route_id_hash = '<hash>';
SELECT aggregate_api_attempts FROM request_logs WHERE session_id = 'test-session-1';
```

---

## Phase 7: Observability (Optional for MVP)

### 7.1 Trace Log Events

**File**: `crates/service/src/gateway/upstream/proxy.rs` and `aggregate_api.rs`

**Events** (use existing `log::debug!` or `log::info!` pattern):

1. **Affinity hit**:
   ```rust
   log::debug!(
       "event=AGGREGATE_AFFINITY_HIT trace_id={} bound_api_id={}",
       trace_id, bound_api_id
   );
   ```

2. **Affinity miss**:
   ```rust
   log::debug!(
       "event=AGGREGATE_AFFINITY_MISS trace_id={} reason={}",
       trace_id, "no_binding" // or "bound_source_unavailable"
   );
   ```

3. **Rebinding**:
   ```rust
   log::info!(
       "event=AGGREGATE_AFFINITY_REBIND trace_id={} old_api_id={} new_api_id={}",
       trace_id, old_api_id, new_api_id
   );
   ```

---

## Phase 8: Deployment and Rollback

### 8.1 Deployment Steps

1. **Apply migrations**: Run `138_`, `139_`, `140_` migrations on production database.
2. **Deploy binary**: New version with affinity code.
3. **Verify default state**: Confirm setting is `"0"`, no behavior change.
4. **Gradual rollout**: Enable for test API keys first:
   - Monitor `aggregate_api_attempts` logs.
   - Check cache rate improvement in test traffic.
5. **Full rollout**: Set `aggregate_api_session_affinity_enabled = '1'` globally.

### 8.2 Rollback

**Immediate**: `UPDATE app_settings SET value = '0' WHERE key = 'aggregate_api_session_affinity_enabled';` — no code change needed.

**Full rollback**: Revert to previous binary; migrations are additive and safe to leave in place.

### 8.3 Maintenance

**Binding cleanup cron** (add to existing maintenance tasks):
```rust
// Daily or weekly
let threshold = Utc::now().timestamp() - (24 * 3600); // 24 hours
storage.delete_stale_aggregate_api_bindings(threshold)?;
```

---

## Verification Commands

### During Development

```bash
# Storage tests
cargo test -p codexmanager-core aggregate_api_bindings

# Gateway tests
cargo test -p codexmanager-service proxy_tests
cargo test -p codexmanager-service aggregate_api

# Full service suite
cargo test -p codexmanager-service
```

### Post-Deployment

```sql
-- Check bindings created
SELECT COUNT(*) FROM aggregate_api_bindings;

-- Sample binding
SELECT * FROM aggregate_api_bindings LIMIT 5;

-- Check attempt logs
SELECT 
    trace_id, 
    aggregate_api_attempts 
FROM request_logs 
WHERE aggregate_api_attempts IS NOT NULL 
LIMIT 5;

-- Cache rate by attempt count (post-affinity)
SELECT 
    CASE WHEN json_array_length(aggregate_api_attempts) = 1 THEN 'single' ELSE 'failover' END AS attempt_type,
    AVG(CASE WHEN cached_input_tokens > 0 THEN 1.0 ELSE 0.0 END) AS cache_rate
FROM request_logs
WHERE aggregate_api_attempts IS NOT NULL
GROUP BY attempt_type;
```

---

## Implementation Checklist

- [ ] Phase 1: Storage foundation (migration, helpers, tests)
- [ ] Phase 2: Configuration (setting, reader)
- [ ] Phase 3: Gateway affinity logic (routing module, candidate reorder)
- [ ] Phase 4: Attempt tracking (instrumentation, classification, binding update)
- [ ] Phase 5: Request log extension (migration, serialization, finalization)
- [ ] Phase 6: Testing (storage tests, gateway tests, e2e simulation)
- [ ] Phase 7: Observability (trace events)
- [ ] Phase 8: Deployment preparation (runbook, rollback plan)

---

## Acceptance Criteria Mapping

| AC | Implementation Phase | Verification Method |
|---|---|---|
| Second request prefers last successful source | Phase 3.2 | Gateway test `affinity_reorders_bound_candidate_to_first` |
| Failover succeeds when bound source unavailable | Phase 4.2 | Gateway test `affinity_preserves_order_when_bound_source_unavailable` + manual |
| Disabled/no-binding preserves order | Phase 3.1, 6.2 | Tests `affinity_disabled_preserves_order`, `affinity_preserves_order_when_no_binding` |
| Cross-key/protocol/model isolation | Phase 1.3 | Storage tests `binding_partitioned_by_*` |
| No secrets in binding/log | Phase 1.1, 5.1 | Schema review + manual log inspection |
| Migration upgradeable, default unchanged | Phase 1.1, 2.1 | Migration tests + setting default check |
| Attempt log distinguishes position/category | Phase 5.2 | Manual log inspection + SQL query |
| Tests pass | All phases | `cargo test --workspace` |

---

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Context threading complexity | Pass `cache_affinity_route_id` explicitly; prefer Option A threading strategy |
| Binding update failure breaks requests | Wrap upsert in error handling; log but don't fail request on binding write error |
| Performance regression on binding lookup | Single indexed query; measure latency in dev; acceptable vs upstream 100ms+ |
| Stale bindings accumulate | Phase 8.3 cleanup cron; monitor table size |

---

## Timeline Estimate

- **Phase 1-2**: 4 hours (storage + config)
- **Phase 3**: 3 hours (routing logic + context threading)
- **Phase 4**: 4 hours (attempt tracking + binding update)
- **Phase 5**: 2 hours (log extension)
- **Phase 6**: 3 hours (testing)
- **Phase 7-8**: 2 hours (observability + deployment prep)

**Total**: ~18 hours (2-3 days with testing and review).
