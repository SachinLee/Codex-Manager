# Database Guidelines

> Database patterns and conventions for this project.

---

## Overview

<!--
Document your project's database conventions here.

Questions to answer:
- What ORM/query library do you use?
- How are migrations managed?
- What are the naming conventions for tables/columns?
- How do you handle transactions?
-->

(To be filled by the team)

---

## Query Patterns

<!-- How should queries be written? Batch operations? -->

(To be filled by the team)

---

## Migrations

### Scenario: SQLite table rebuild migrations

#### 1. Scope / Trigger
- Trigger: a migration rebuilds a table with `ALTER TABLE ... RENAME TO ...`,
  `CREATE TABLE ...`, and `INSERT INTO new_table (...) SELECT ... FROM old_table`.
- This is required for legacy `request_logs` compaction and any future migration
  that drops columns or rewrites table shape.

#### 2. Signatures
- DB owner: `crates/core/src/storage/*.rs`
- Migration helper: storage init / compat migration path.
- SQL shape:
  ```sql
  INSERT INTO table_name (col_a, col_b, col_c)
  SELECT old_a, NULL, old_c FROM table_name_legacy;
  ```

#### 3. Contracts
- The `INSERT INTO (...)` column list and the `SELECT ...` value list must have
  exactly the same count and order.
- New columns with no legacy source must receive an explicit `NULL` or default
  expression in the `SELECT` list.
- If a new column is inserted between existing columns, update every rebuild
  `SELECT` list at the same position; do not append it at the end unless the
  `INSERT` column list also appends it.

#### 4. Validation & Error Matrix
- Column/value count mismatch -> SQLite init fails with
  `N values for M columns`.
- Misordered `NULL` placeholders -> migration succeeds but data shifts into the
  wrong semantic column.
- Missing old-table fixture -> breakage is only found on user databases.

#### 5. Good/Base/Bad Cases
- Good: `client_reasoning_effort` is new, so the legacy rebuild `SELECT` includes
  a `NULL` before `reasoning_effort`.
- Base: appended nullable column appears at the end of both the `INSERT` column
  list and the `SELECT` list.
- Bad: `INSERT` includes both `client_reasoning_effort` and `reasoning_effort`,
  while `SELECT` only supplies one value for the pair.

#### 6. Tests Required
- Add or update a migration regression that creates the old table shape, inserts
  at least one row, runs `Storage::init()`, and asserts:
  - init succeeds,
  - removed legacy columns are gone,
  - newly expected columns exist,
  - key row values are preserved in their intended columns.

#### 7. Wrong vs Correct

Wrong:
```sql
INSERT INTO request_logs (..., client_reasoning_effort, reasoning_effort, ...)
SELECT ..., reasoning_effort, ... -- missing value for client_reasoning_effort
```

Correct:
```sql
INSERT INTO request_logs (..., client_reasoning_effort, reasoning_effort, ...)
SELECT ..., NULL, reasoning_effort, ...
```

When reviewing, count both lists after formatting changes. The exact position of
each placeholder matters as much as the total count.

## Scenario: Normalized cache-write token accounting

### 1. Scope / Trigger

- Trigger: A provider exposes cache-write tokens separately from cache reads, or a price rule gains a new token-category price.
- Applies to `model_price_rules`, `request_token_stats`, request-token rollups, RPC read models, and wallet raw-usage payloads.

### 2. Signatures

- `model_price_rules.cache_write_price_per_1m REAL NULL`
- `model_price_rules.long_context_cache_write_price_per_1m REAL NULL`
- `request_token_stats.cache_write_input_tokens INTEGER NULL`
- Rollup columns: `request_token_stat_rollups.cache_write_input_tokens` and `request_token_daily_rollups.cache_write_input_tokens`.

### 3. Contracts

- `input_tokens` is normalized total input. Cache reads and cache writes are mutually exclusive subsets of it.
- Normalize every calculation as `read = clamp(cached, 0, total)`, `write = clamp(cache_write, 0, total - read)`, and `plain = total - read - write`.
- A missing generic cache-write price falls back to the ordinary input price for a compatible estimate, but callers must retain the `partial` price status.
- If `total_tokens` is absent, derive it as `input_tokens + output_tokens`; do not subtract cache reads or writes because both already belong to total input.
- New migration columns are additive and default old records to `NULL`/`0`; never recalculate historical costs without recoverable token detail.

### 4. Validation & Error Matrix

- Negative read/write tokens -> clamp to `0`.
- Read plus write greater than total input -> read wins first; clamp write to the remaining input.
- Long-context threshold reached exactly -> keep short price; use the long tier only when `input_tokens > threshold`.
- Legacy RPC payload omits cache-write fields -> deserialize successfully and report `0` tokens.
- Price field is negative or non-finite -> reject the price-rule update at the service boundary.

### 5. Good/Base/Bad Cases

- Good: total input `1000`, read `200`, write `100` records plain `700` and bills each category once.
- Base: a historical row with no write column still displays and rolls up as zero write tokens.
- Bad: deriving fallback total as `input - cached + output`; this undercounts normalized OpenAI input.
- Bad: summing raw cache-write tokens independently after already charging total input; this double-counts cache writes.

### 6. Tests Required

- Storage round-trip and daily/model/key rollup tests assert the cache-write aggregate.
- Price tests cover short/long prices, exact threshold, and read/write overflow clamping.
- Request-log and wallet re-rating tests pass cache-write tokens and the effective service tier.
- RPC serialization tests assert the `cacheWriteInputTokens` camelCase field and compatibility defaults.

### 7. Wrong vs Correct

#### Wrong

```sql
-- `input_tokens` already includes cached input, so this undercounts total usage.
SELECT input_tokens - cached_input_tokens + output_tokens AS total_tokens;
```

#### Correct

```sql
-- Read and write are classifications of total input, not additional input.
SELECT input_tokens + output_tokens AS total_tokens;
```

## Scenario: Retention-safe request-token rollups

### 1. Scope / Trigger

- Trigger: change a request-token migration, retention job, or a time-range usage query.
- Applies to `request_token_stats`, `request_token_stat_hourly_rollups`, and legacy aggregate tables such as `request_token_stat_rollups` / `request_token_daily_rollups`.

### 2. Signatures

- Hourly identity: `(bucket_start, key_id, account_id, model, actual_source_kind, actual_source_id, owner_user_id)`.
- Retention writer: `Storage::rollup_request_token_stats_before(cutoff_ts)`.
- Range reads must select raw rows by `created_at` and hourly rows by `bucket_start` / `bucket_end`; unbounded lifetime summaries may additionally read legacy aggregate rollups.

### 3. Contracts

- Storage initialization and compatibility migration closure must create and normalize the hourly table before a retention job can insert into it.
- Cutoffs are aligned to the hour. Only completely contained hourly buckets participate in a bounded range, preventing partial-bucket overcounting.
- Hourly rows preserve owner and actual-source dimensions, so account, Aggregate API, model, key-model, user, and source reports cannot silently lose historical data after raw-row pruning.
- Cache-write tokens are carried through every raw/hourly/legacy projection and normalize to `0` for pre-column rows.

### 4. Validation & Error Matrix

- Hourly table absent before retention -> migration/schema closure bug; do not silently skip rollup writes.
- Range query reads only raw rows -> historical report becomes incomplete after retention.
- Range intersects only part of an hour -> exclude that aggregate bucket rather than count a full hour outside the requested interval.
- Legacy nullable cache-write value -> read as `0`, never fail an aggregate query.

### 5. Good/Base/Bad Cases

- Good: a 90-day account report unions raw rows with complete hourly buckets and retains the correct owner/source dimensions.
- Base: an all-time key/model report can use legacy aggregate totals after raw data was compacted.
- Bad: using `raw_condition.replace("key_id", "h.key_id")`; aliases and temporary-table predicates must be generated explicitly for both sources.

### 6. Tests Required

- Create a pre-closure database fixture, run `Storage::init()`, compact an old request, and assert the hourly row exists with cache-write/source/owner values.
- Assert account, Aggregate API, model, key-model, user, and source range queries return compacted hourly usage.
- Cover a partial-hour boundary and a large key set using the paired raw/hourly filter helper.

### 7. Wrong vs Correct

#### Wrong

```sql
SELECT * FROM request_token_stats
WHERE created_at >= ?1 AND created_at < ?2;
```

#### Correct

```sql
SELECT ... FROM request_token_stats
WHERE created_at >= ?1 AND created_at < ?2
UNION ALL
SELECT ... FROM request_token_stat_hourly_rollups
WHERE bucket_start >= ?1 AND bucket_end <= ?2;
```

---

## Naming Conventions

<!-- Table names, column names, index names -->

(To be filled by the team)

---

## Common Mistakes

<!-- Database-related mistakes your team has made -->

(To be filled by the team)
