# Implementation Plan

## Phase 0: Discovery

- [x] Inspect current Aggregate API RPC handlers, frontend API wrappers, and aggregate API page structure.
- [x] Inspect current request log schema/projection and gateway cooldown route quality code.
- [x] Inspect current image generation bridge and response finalization path.

## Phase 1: P1 Capability Diagnostics

- [x] Add backend diagnostic result types for upstream capability checks.
- [x] Add a bounded diagnostic executor for Aggregate API targets.
- [x] Add RPC/command handler to run diagnostics for a selected Aggregate API.
- [x] Add frontend API wrapper and minimal UI entry in Aggregate API details/page.
- [x] Add tests for diagnostic result classification and non-mutating behavior.

## Phase 2: P2 Route Evidence / Policy Actions

- [x] Add `RouteEvidence` and `GatewayPolicyAction` domain models.
- [x] Project existing cooldown / quota / rate-limit / transport decisions into evidence summaries.
- [x] Add system-owned cooldown policy action read model with expiration semantics.
- [x] Extend request log projection or gateway log projection with additive route evidence fields.
- [x] Add tests for action expiration and request-log projection defaults.

## Phase 3: P3 Semantic Validation

- [x] Add hosted image generation semantic validator.
- [x] Wire validator into the hosted image generation response path only.
- [x] Return clear semantic failure body when image result is missing.
- [x] Add tests for valid image result, missing output array, missing result, invalid JSON.

## Phase 4: Verification

- [x] Run narrow Rust tests for new modules.
- [x] Run relevant service tests for Aggregate API / gateway image path.
- [x] Run frontend type/build checks if UI changes are included.
- [x] Review `git diff` and ensure no P4+ scope slipped in.

## Validation Commands

Use the narrowest relevant commands first:

```powershell
cargo test -p codexmanager-service capability
cargo test -p codexmanager-service policy
cargo test -p codexmanager-service image
pnpm -C apps run build
```

If shared core schema or storage changes are added:

```powershell
cargo test -p codexmanager-core
```

If gateway behavior changes broadly:

```powershell
cargo test --workspace
```

## Rollback

- Diagnostic UI/API can be removed without affecting existing routing.
- Evidence fields must be additive, so rollback should remove new projections without changing old request-log fields.
- Semantic validation should be guarded to hosted image generation path only; rollback by bypassing that contract.
