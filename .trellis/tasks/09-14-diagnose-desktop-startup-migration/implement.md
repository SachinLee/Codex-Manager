# Implementation plan: Restore custom model seed compatibility

## Profile

Standard. The production delta is small, but it changes persistent-database startup behavior and must preserve user-owned catalog records.

### Slice 1: AC-001, AC-002 - Restore the explicit Grok collision allowance

- Behavior: a custom `grok-4.5` no longer blocks normal startup seeding; it remains a custom row while its existing missing-data backfills still run.
- Code boundary: `crates/core/src/storage/model_catalog_v2.rs` only.
- Test seam: `seed_backfills_quota_aggregate_routes_and_missing_grok_prices`.
- RED: the current focused test fails with `builtin seed slug grok-4.5 is owned by a custom model`.
- Implementation: append the literal `grok-4.5` to `TOLERATED_CUSTOM_SEED_COLLISIONS`; keep `gpt-image-2` and `GPT6_ASTRA_SLUG` unchanged. Do not alter `insert_seed`, migration ordering, or backfill logic.
- GREEN: `cargo test -p codexmanager-core seed_backfills_quota_aggregate_routes_and_missing_grok_prices -- --exact`.
- Validation: confirm the test covers a second seed after user price/metadata edits; run `cargo test -p codexmanager-core --lib model_catalog_v2`.
- Dependencies: none.
- Rollback: revert the single allowlist entry; this reintroduces the known startup failure and is not a safe release rollback for affected users.

### Slice 2: AC-003 - Preserve Astra startup compatibility

- Behavior: an existing custom `gpt-6-astra` survives both the dedicated Astra migration and the later ordinary catalog seed.
- Code boundary: the existing custom-Astra regression in `crates/core/src/storage/model_catalog_v2.rs`; production policy remains the single allowlist from Slice 1.
- Test seam: `astra_migration_preserves_custom_slug_and_user_edited_sol_metadata`.
- RED: add a normal seed invocation after the custom-Astra migration; this would fail if the Astra allowance is accidentally removed.
- Implementation: assert the existing custom Astra model remains unchanged after the seed call; do not duplicate the allowlist or add a special migration branch.
- GREEN: `cargo test -p codexmanager-core astra_migration_preserves_custom_slug_and_user_edited_sol_metadata -- --exact`.
- Validation: re-run the full model-catalog unit module from Slice 1.
- Dependencies: Slice 1, because both scenarios depend on the shared collision policy.
- Rollback: remove only the test extension if a test setup issue is discovered; do not weaken the production assertion.

### Slice 3: AC-004 - Verify release-path behavior

- Behavior: the repaired desktop executable starts against a database copy containing a custom `grok-4.5` without user intervention.
- Code boundary: no additional production files.
- Test seam: actual packaged desktop executable plus its normal persisted SQLite initialization.
- Preparation: preserve the reported database and use a copy; never test by deleting or manually mutating the user’s live database.
- Validation: run `cargo test -p codexmanager-core --lib model_catalog_v2`; then package with the project’s normal Windows rebuild command and launch the resulting EXE against the database copy. Confirm no migration dialog appears and the app reaches the main surface.
- Dependencies: Slices 1-2.
- Rollback: retain the pre-migration backup / original database copy; no schema rollback is needed.

## Change boundary

Expected changed files:

- `crates/core/src/storage/model_catalog_v2.rs`: restore the lost compatibility literal and strengthen the existing Astra startup regression.

Explicitly unchanged:

- SQLite migration files and schema versioning.
- Tauri startup, diagnostic dialogs, and backup/restore behavior.
- Frontend, service APIs, routing policy, model fixtures, and user database contents.

## Final verification matrix

| Requirement | Proof |
| --- | --- |
| AC-001 | Focused custom-Grok seed test passes. |
| AC-002 | Same test proves backfills and second-seed preservation. |
| AC-003 | Focused custom-Astra migration-plus-seed test passes. |
| AC-004 | Packaged EXE smoke test against a database copy, when that database is available. |
