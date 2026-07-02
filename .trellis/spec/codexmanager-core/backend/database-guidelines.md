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

---

## Naming Conventions

<!-- Table names, column names, index names -->

(To be filled by the team)

---

## Common Mistakes

<!-- Database-related mistakes your team has made -->

(To be filled by the team)
