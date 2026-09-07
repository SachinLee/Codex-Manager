# Implementation plan: GPT-6 Astra model catalog data

### Slice 1: AC-001/AC-002 - Add the builtin model fixture

- Behavior: fresh and existing initialized SQLite catalogs expose `gpt-6-astra` as a visible builtin model.
- Code boundary: `crates/core/seeds/model_catalog_v2_2026_07_10.json`; existing `seed_missing_builtin_models_v2()` path.
- Test seam: core model catalog initialization and listing tests using `Storage::open_in_memory()`.
- RED: add/adjust focused assertions for the 11-model fixture and Astra fields before the fixture entry exists; capture the expected failure if practical.
- Implementation: add Astra with priority 0, official capabilities, context values, base/long tiers, source URL, and route-compatible slug; update revision/source metadata.
- GREEN: run the focused `codexmanager-core` model catalog test target and verify Astra appears after `Storage::init()`.
- Validation: inspect the generated model, price, tier, and route rows through the existing storage API; verify a second `init()` remains idempotent and preserves user-edited rows.
- Dependencies: none.
- Rollback: revert the fixture and its focused assertions.

### Slice 2: AC-003 - Prove long-context billing boundaries

- Behavior: 272,000 input tokens use base rates; 272,001 input tokens use the official doubled/1.5x rates, including cache-write pricing.
- Code boundary: existing `select_model_price_tier_v2()` behavior and focused model catalog/billing tests; no new pricing selector.
- Test seam: `Storage::select_model_price_tier_v2("gpt-6-astra", input_tokens)`.
- RED: add the boundary assertions before the fixture entry exists; capture the expected missing-model failure if practical.
- Implementation: use `min_input_tokens=272001` and official rate values in the fixture; add only the minimal assertions needed to defend the boundary and rates.
- GREEN: run the targeted core billing/model catalog tests.
- Validation: assert base and long tiers, all four price categories, and the model-level base price match.
- Dependencies: Slice 1.
- Rollback: revert the new Astra test rows/assertions.

### Slice 3: AC-004 - Update the configured runtime database

- Behavior: the user-provided application SQLite database contains Astra immediately, without requiring a UI import.
- Code boundary: `C:\Users\shuan\AppData\Roaming\com.codexmanager.desktop\codexmanager.db`; fixture remains the durable source for future initialization.
- Test seam: read-only SQLite queries against `models`, `model_prices`, `model_price_tiers`, `model_routes`, and `model_catalog_v2_meta` after the write.
- RED: current database query shows no `gpt-6-astra`, `builtin_revision=8`, and no Astra price/route rows.
- Implementation: stop/confirm absence of the Codex Manager process, create a timestamped backup, execute one transaction that inserts the builtin model, base/long prices, default account-pool route, and revision metadata with `INSERT OR IGNORE`/guarded updates, then commit.
- GREEN: read-only queries show the expected Astra model, prices, tiers, route, and updated revision; existing model count and unrelated rows remain intact.
- Validation: verify database integrity and keep the backup path in task evidence. If the database is locked or the write cannot be safely completed, do not force it; record the exact blocker.
- Dependencies: Slices 1–2; the fixture values and direct SQL must match.
- Rollback: close the application and restore the timestamped backup only if post-write verification fails.
