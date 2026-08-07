from pathlib import Path
p = Path(r'crates/core/src/storage/model_catalog_v2.rs')
t = p.read_text(encoding='utf-8')

# Extract the mapping migration body into a reusable backfill_routes_from_model_source_mappings
# and call it from seed as well.

old_seed = '''    pub(super) fn seed_missing_builtin_models_v2(&self) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        seed_missing(&self.conn)?;
        // Idempotent repair for upgraded DBs that already recorded 112_model_catalog_v2
        // before aggregate quota assignments / Grok prices were carried over.
        backfill_aggregate_routes_from_quota_assignments(&self.conn)?;
        backfill_missing_grok_prices(&self.conn)?;
        tx.commit()
    }'''

new_seed = '''    pub(super) fn seed_missing_builtin_models_v2(&self) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        seed_missing(&self.conn)?;
        // Idempotent repair for upgraded DBs that already recorded 112_model_catalog_v2
        // before aggregate quota assignments / Grok prices were carried over.
        backfill_routes_from_model_source_mappings(&self.conn)?;
        backfill_aggregate_routes_from_quota_assignments(&self.conn)?;
        backfill_missing_grok_prices(&self.conn)?;
        tx.commit()
    }'''

if old_seed not in t:
    raise SystemExit('seed block not found')
t = t.replace(old_seed, new_seed, 1)

# Rename the first-pass logic out of migrate_legacy_routes into backfill_routes_from_model_source_mappings
old_mig = '''fn migrate_legacy_routes(conn: &Connection) -> Result<()> {
    // First pass: explicit model_source_mappings for aggregate / account sources.
    // Main historically skipped simple 1:1 default mappings; custom branch primarily
    // stored assignments in quota_source_model_assignments, so both must be carried over.
    let now = now_ts();
    if table_exists(conn, "model_source_mappings")?
        && table_exists(conn, "aggregate_apis")?
    {
        let has_prefs = table_exists(conn, "model_source_mapping_preferences")?;
        let sql = if has_prefs {
            "SELECT r.platform_model_slug,r.source_kind,r.source_id,r.upstream_model,
                    r.enabled,r.priority,r.weight
             FROM model_source_mappings r
             LEFT JOIN aggregate_apis a
               ON r.source_kind='aggregate_api' AND a.id=r.source_id
             LEFT JOIN model_source_mapping_preferences pref
               ON pref.source_kind=r.source_kind AND pref.source_id=r.source_id
              AND pref.upstream_model=r.upstream_model
             WHERE r.enabled=1
               AND r.source_kind IN ('aggregate_api','openai_account')
               AND pref.preference IS NULL
               AND (r.source_kind <> 'aggregate_api' OR a.id IS NOT NULL)
             ORDER BY r.priority DESC,r.id ASC"
        } else {
            "SELECT r.platform_model_slug,r.source_kind,r.source_id,r.upstream_model,
                    r.enabled,r.priority,r.weight
             FROM model_source_mappings r
             LEFT JOIN aggregate_apis a
               ON r.source_kind='aggregate_api' AND a.id=r.source_id
             WHERE r.enabled=1
               AND r.source_kind IN ('aggregate_api','openai_account')
               AND (r.source_kind <> 'aggregate_api' OR a.id IS NOT NULL)
             ORDER BY r.priority DESC,r.id ASC"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, bool>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?;
        let mappings = rows.collect::<Result<Vec<_>>>()?;
        drop(stmt);
        for (slug, old_kind, source_id, upstream_model, enabled, priority, weight) in mappings {
            let Some(model_id) = conn
                .query_row(
                    "SELECT id FROM models WHERE slug=?1 COLLATE NOCASE",
                    [&slug],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
            else {
                continue;
            };
            let (source_kind, source_id) = match old_kind.as_str() {
                "openai_account" => ("account_pool", "default".to_string()),
                "aggregate_api" => ("aggregate_api", source_id),
                _ => continue,
            };
            insert_route_if_missing(
                conn,
                &model_id,
                source_kind,
                &source_id,
                &upstream_model,
                enabled,
                priority,
                weight,
                now,
            )?;
        }
    }

    // Second pass: quota_source_model_assignments is the custom-branch primary store
    // for "which models this aggregate API serves". Map each assignment to a v2 route.
    backfill_aggregate_routes_from_quota_assignments(conn)?;
    Ok(())
}'''

new_mig = '''fn backfill_routes_from_model_source_mappings(conn: &Connection) -> Result<()> {
    // Explicit model_source_mappings for aggregate / account sources.
    // Main historically skipped simple 1:1 default mappings; custom branch primarily
    // stored assignments in quota_source_model_assignments, so both must be carried over.
    if !(table_exists(conn, "model_source_mappings")? && table_exists(conn, "models")?) {
        return Ok(());
    }
    let now = now_ts();
    let has_prefs = table_exists(conn, "model_source_mapping_preferences")?;
    let has_apis = table_exists(conn, "aggregate_apis")?;
    let sql = if has_prefs && has_apis {
        "SELECT r.platform_model_slug,r.source_kind,r.source_id,r.upstream_model,
                r.enabled,r.priority,r.weight
         FROM model_source_mappings r
         LEFT JOIN aggregate_apis a
           ON r.source_kind='aggregate_api' AND a.id=r.source_id
         LEFT JOIN model_source_mapping_preferences pref
           ON pref.source_kind=r.source_kind AND pref.source_id=r.source_id
          AND pref.upstream_model=r.upstream_model
         WHERE r.enabled=1
           AND r.source_kind IN ('aggregate_api','openai_account')
           AND pref.preference IS NULL
           AND (r.source_kind <> 'aggregate_api' OR a.id IS NOT NULL)
         ORDER BY r.priority DESC,r.id ASC"
    } else if has_apis {
        "SELECT r.platform_model_slug,r.source_kind,r.source_id,r.upstream_model,
                r.enabled,r.priority,r.weight
         FROM model_source_mappings r
         LEFT JOIN aggregate_apis a
           ON r.source_kind='aggregate_api' AND a.id=r.source_id
         WHERE r.enabled=1
           AND r.source_kind IN ('aggregate_api','openai_account')
           AND (r.source_kind <> 'aggregate_api' OR a.id IS NOT NULL)
         ORDER BY r.priority DESC,r.id ASC"
    } else {
        "SELECT r.platform_model_slug,r.source_kind,r.source_id,r.upstream_model,
                r.enabled,r.priority,r.weight
         FROM model_source_mappings r
         WHERE r.enabled=1
           AND r.source_kind IN ('openai_account')
         ORDER BY r.priority DESC,r.id ASC"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, bool>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, i64>(6)?,
        ))
    })?;
    let mappings = rows.collect::<Result<Vec<_>>>()?;
    drop(stmt);
    for (slug, old_kind, source_id, upstream_model, enabled, priority, weight) in mappings {
        let Some(model_id) = conn
            .query_row(
                "SELECT id FROM models WHERE slug=?1 COLLATE NOCASE",
                [&slug],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        else {
            continue;
        };
        let (source_kind, source_id) = match old_kind.as_str() {
            "openai_account" => ("account_pool", "default".to_string()),
            "aggregate_api" => ("aggregate_api", source_id),
            _ => continue,
        };
        insert_route_if_missing(
            conn,
            &model_id,
            source_kind,
            &source_id,
            &upstream_model,
            enabled,
            priority,
            weight,
            now,
        )?;
    }
    Ok(())
}

fn migrate_legacy_routes(conn: &Connection) -> Result<()> {
    backfill_routes_from_model_source_mappings(conn)?;
    // Second pass: quota_source_model_assignments is the custom-branch primary store
    // for "which models this aggregate API serves". Map each assignment to a v2 route.
    backfill_aggregate_routes_from_quota_assignments(conn)?;
    Ok(())
}'''

if old_mig not in t:
    raise SystemExit('migrate_legacy_routes block not found for refactor')
t = t.replace(old_mig, new_mig, 1)
p.write_text(t, encoding='utf-8')
print('refactored mapping backfill into seed path')
