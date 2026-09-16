# Diagnose desktop startup migration failure

## Goal

Restore compatible desktop startup for persisted databases whose custom model slug collides with a known built-in seed, beginning with `grok-4.5`, without overwriting user-owned catalog data.

## Background

- The packaged desktop shell initializes the persisted SQLite database before opening the main window. `apps/src-tauri/src/lib.rs:185-196` converts any storage initialization error into `database migration failed; refusing desktop startup`, so a migration error intentionally blocks the EXE rather than starting with potentially inconsistent data.
- The failing user database contains a custom model whose case-insensitive slug is `grok-4.5`, as shown by the dialog. The error is emitted by `crates/core/src/storage/model_catalog_v2.rs:570-586` when a built-in fixture slug resolves to an existing non-builtin row.
- Startup reaches `Storage::seed_missing_builtin_models_v2()` at `crates/core/src/storage/mod.rs:2781`; that routine invokes `seed_missing()` before route and pricing backfills (`crates/core/src/storage/model_catalog_v2.rs:1513-1522`).
- The current branch changed `TOLERATED_CUSTOM_SEED_COLLISIONS` from `[`gpt-image-2`, `grok-4.5`]` to `[`gpt-image-2`, `gpt-6-astra`]` while adding the Astra catalog migration. This removed the compatibility exception for existing custom `grok-4.5` rows without changing `insert_seed()`'s hard-fail behavior.
- The repository already has a regression scenario named `seed_backfills_quota_aggregate_routes_and_missing_grok_prices` (`crates/core/src/storage/model_catalog_v2.rs:3664-3823`) that creates a custom `grok-4.5` row and expects `seed_missing_builtin_models_v2()` to succeed, matching the reported user database shape. The current collision list contradicts that test's intended compatibility contract.
- The service wrapper creates a pre-migration backup and restores it when initialization fails (`crates/service/src/storage/storage_helpers.rs:757-777`), which explains the dialog's promise that the database will not be automatically deleted and provides a recovery point.

## Scope

- In scope: restore the lost `grok-4.5` collision allowance, retain the existing `gpt-image-2` and `gpt-6-astra` allowances, and prove that startup seeding, Grok pricing/route backfill, and user-owned model data remain compatible.
- Out of scope: changing the collision policy for arbitrary custom slugs, creating a new SQLite migration, modifying or deleting user databases, converting custom models into builtins, or changing the Tauri startup/backup error policy.

## Actors and affected systems

- Desktop Tauri startup, service storage initialization, SQLite model catalog V2, built-in model seeding, and users with a custom model slug colliding with a built-in seed.

## Constraints and assumptions

- Slug lookup is case-insensitive (`COLLATE NOCASE`), so any casing variant of `grok-4.5` collides.
- Built-in seeds must not overwrite custom model metadata, routes, or pricing. The compatibility behavior is to tolerate the collision and let custom-model backfills operate, not to silently convert the row into a builtin.
- The screenshot's path and message are treated as the user's observed runtime evidence; the repository analysis explains the matching source path but cannot inspect that external database directly.

## Acceptance Criteria

### AC-001: Upgrade with a custom Grok model starts successfully

- Scenario: a persisted catalog has a custom model whose slug is any case variant of `grok-4.5`.
- Action: desktop storage runs the ordinary `seed_missing_builtin_models_v2()` startup path.
- Expected: initialization succeeds; no `builtin seed slug grok-4.5 is owned by a custom model` error is returned.
- Must not: create a duplicate builtin Grok row or convert the custom row to `origin='builtin'`.
- Verification method: targeted storage regression test.

### AC-002: Seed compatibility preserves custom data and backfills

- Scenario: the custom `grok-4.5` has user-owned metadata/prices plus aggregate quota assignments.
- Action: run startup seeding twice.
- Expected: Grok reasoning/official-estimate backfill and aggregate routes are created only where missing; subsequent seed preserves user-edited metadata, custom prices, routes, and origin.
- Must not: overwrite custom values or lose the aggregate route.
- Verification method: the existing focused `seed_backfills_quota_aggregate_routes_and_missing_grok_prices` behavior test.

### AC-003: Existing Astra compatibility remains covered

- Scenario: a custom model occupies `gpt-6-astra`.
- Action: run the Astra migration and the normal catalog seed path.
- Expected: both operations tolerate the custom row and preserve it.
- Must not: regress the new Astra compatibility exception while restoring Grok.
- Verification method: extend the existing Astra custom-slug regression with the normal seed call.

### AC-004: Release safety

- Scenario: the fixed code is packaged and launched against a previously failing database.
- Action: replace the affected executable and launch it normally.
- Expected: the application reaches normal startup without a database migration dialog; the database remains in place.
- Must not: require the user to delete, edit, or restore the database manually.
- Verification method: focused core tests followed by a packaged-EXE smoke test against a copy of the reported database when available.

## Key decision

- Use the existing explicit allowlist as the compatibility boundary and append `grok-4.5`; do not broaden it to every custom/builtin collision. The allowlist already controls both insertion and catalog smoke validation, so one source of truth maintains the intended safety guard for unknown collisions.

## Open decisions

- None. The existing historical behavior and focused regression test establish the intended compatibility contract.

## Verification evidence

- `cargo test -p codexmanager-core seed_backfills_quota_aggregate_routes_and_missing_grok_prices -- --nocapture` reproduces the failure on the current working tree: the test panics at `model_catalog_v2.rs:3748` with `InvalidParameterName("builtin seed slug grok-4.5 is owned by a custom model")`.
- The same test's setup deletes the builtin Grok row, inserts a user-owned custom row with slug `grok-4.5`, then calls `seed_missing_builtin_models_v2()`. This is a direct repository reproduction of the screenshot's data condition, not an inferred packaging failure.
- A prior merge parent still contains `const TOLERATED_CUSTOM_SEED_COLLISIONS: &[&str] = &["gpt-image-2", "grok-4.5"];`; the current working tree contains `&["gpt-image-2", GPT6_ASTRA_SLUG]`. The regression is therefore a one-entry replacement during the Astra change.
