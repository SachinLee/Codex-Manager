# Implementation Approval

**Date**: 2026-09-08  
**Approver**: User (sachin)  
**Status**: ✅ Approved for implementation

## Approval Context

User request: "批准实施" (Approve implementation)

## Planning Artifacts Reviewed

- [x] `prd.md` - Production diagnosis evidence and repair boundary
- [x] `design.md` - Technical design with policy decision table, seam analysis, invariants
- [x] `implement.md` - 7-slice TDD implementation plan

## Key Decisions Approved

1. **Unified classification**: Reuse `classify_upstream_failure()` for both HTTP non-2xx and SSE terminal errors
2. **Single modification seam**: `aggregate_api.rs:3208` insertion point after capacity recovery
3. **Zero-delivery safety**: `pending_failover_request.is_some()` check prevents unsafe replay
4. **Error code extraction**: New helper reuses existing JSON parsing patterns
5. **Policy**:
   - `rate_limit_exceeded` → CandidateFailover (direct rotation)
   - `server_error` → RetrySameCandidate with existing budget
   - Already-delivered streams → no retry
6. **Quality profile**: Critical (affects public gateway behavior and billing)

## Implementation Authorization

Task may now transition to `in_progress` and begin TDD implementation following the approved 7-slice plan in `implement.md`.

## Notes

- Cache correlation acknowledged: multi-candidate paths show lower cache because they represent fallback scenarios (correct behavior, not a constraint)
- All existing paths preserved (HTTP non-2xx, Chat preflight, capacity recovery, reasoning guard)
- Bounded traversal and retry budgets maintained
- Regression test suite must pass
