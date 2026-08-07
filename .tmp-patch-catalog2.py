from pathlib import Path
path = Path(r'crates/core/src/storage/model_catalog_v2.rs')
text = path.read_text(encoding='utf-8')
start = text.find('fn insert_route_if_missing')
end = text.find('fn migrate_legacy_groups')
if start < 0 or end < 0:
    raise SystemExit(f'anchors not found {start} {end}')

replacement = r'''fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    conn.query_row(
        "SELECT COUNT(1) FROM sqlite_master WHERE type='table' AND name=?1",
        [name],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count > 0)
}

fn insert_route_if_missing(
    conn: &Connection,
    model_id: &str,
    source_kind: &str,
    source_id: &str,
    upstream_model: &str,
    enabled: bool,
    priority: i64,
    weight: i64,
    now: i64,
) -> Result<()> {
    let route = ModelRouteV2 {
        source_kind: source_kind.to_string(),
        source_id: source_id.to_string(),
        upstream_model: upstream_model.to_string(),
        enabled,
        priority,
        weight: weight.max(1),
        ..Default::default()
    };
    conn.execute(
        "INSERT OR IGNORE INTO model_routes(
           id,model_id,source_kind,source_id,upstream_model,enabled,priority,weight,created_at,updated_at
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)",
        params![
            route_id(model_id, &route),
            model_id,
            route.source_kind,
            route.source_id,
            route.upstream_model,
            route.enabled,
            route.priority,
            route.weight,
            now
        ],
    )?;
    Ok(())
}

fn migrate_legacy_routes(conn: &Connection) -> Result<()> {
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
}

fn backfill_aggregate_routes_from_quota_assignments(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "quota_source_model_assignments")?
        || !table_exists(conn, "aggregate_apis")?
        || !table_exists(conn, "model_routes")?
        || !table_exists(conn, "models")?
    {
        return Ok(());
    }

    let now = now_ts();
    let has_mappings = table_exists(conn, "model_source_mappings")?;
    let sql = if has_mappings {
        "SELECT q.model_slug,q.source_id,
                COALESCE(
                  (SELECT r.upstream_model FROM model_source_mappings r
                    WHERE r.source_kind='aggregate_api' AND r.source_id=q.source_id
                      AND r.platform_model_slug=q.model_slug COLLATE NOCASE
                      AND r.enabled=1
                    ORDER BY r.priority DESC,r.id ASC LIMIT 1),
                  q.model_slug
                ) AS upstream_model,
                COALESCE(
                  (SELECT r.priority FROM model_source_mappings r
                    WHERE r.source_kind='aggregate_api' AND r.source_id=q.source_id
                      AND r.platform_model_slug=q.model_slug COLLATE NOCASE
                      AND r.enabled=1
                    ORDER BY r.priority DESC,r.id ASC LIMIT 1),
                  0
                ) AS priority,
                COALESCE(
                  (SELECT r.weight FROM model_source_mappings r
                    WHERE r.source_kind='aggregate_api' AND r.source_id=q.source_id
                      AND r.platform_model_slug=q.model_slug COLLATE NOCASE
                      AND r.enabled=1
                    ORDER BY r.priority DESC,r.id ASC LIMIT 1),
                  1
                ) AS weight
         FROM quota_source_model_assignments q
         JOIN aggregate_apis a ON a.id=q.source_id
         WHERE q.source_kind='aggregate_api' AND TRIM(q.model_slug) <> ''
         ORDER BY q.source_id ASC, q.model_slug ASC"
    } else {
        "SELECT q.model_slug,q.source_id,q.model_slug AS upstream_model,0 AS priority,1 AS weight
         FROM quota_source_model_assignments q
         JOIN aggregate_apis a ON a.id=q.source_id
         WHERE q.source_kind='aggregate_api' AND TRIM(q.model_slug) <> ''
         ORDER BY q.source_id ASC, q.model_slug ASC"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;
    let assignments = rows.collect::<Result<Vec<_>>>()?;
    drop(stmt);
    for (slug, source_id, upstream_model, priority, weight) in assignments {
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
        insert_route_if_missing(
            conn,
            &model_id,
            "aggregate_api",
            &source_id,
            &upstream_model,
            true,
            priority,
            weight,
            now,
        )?;
    }
    Ok(())
}

/// Official Grok 4.5 Standard rates (USD / 1M tokens):
/// short (<200K): 2.00 / 0.50 / 6.00; long (>=200K): 4.00 / 1.00 / 12.00.
/// Only fills models that currently have price_status='missing' so user edits stay.
fn backfill_missing_grok_prices(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "models")?
        || !table_exists(conn, "model_prices")?
        || !table_exists(conn, "model_price_tiers")?
    {
        return Ok(());
    }

    // microusd per 1M tokens
    const SHORT_INPUT: i64 = 2_000_000;
    const SHORT_CACHED: i64 = 500_000;
    const SHORT_OUTPUT: i64 = 6_000_000;
    const LONG_INPUT: i64 = 4_000_000;
    const LONG_CACHED: i64 = 1_000_000;
    const LONG_OUTPUT: i64 = 12_000_000;
    const LONG_THRESHOLD: i64 = 200_000;

    let now = now_ts();
    let mut stmt = conn.prepare(
        "SELECT m.id,m.slug FROM models m
         JOIN model_prices p ON p.model_id=m.id
         WHERE p.price_status='missing'
           AND (
             lower(m.slug) LIKE 'grok-4.5%'
             OR lower(m.slug) = 'grok-build-latest'
           )",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let targets = rows.collect::<Result<Vec<_>>>()?;
    drop(stmt);
    for (model_id, _slug) in targets {
        let updated = conn.execute(
            "UPDATE model_prices SET
               input_microusd_per_1m=?2,
               cached_input_microusd_per_1m=?3,
               output_microusd_per_1m=?4,
               price_status='estimated',
               price_source='xai_official_grok_4_5',
               updated_at=?5
             WHERE model_id=?1 AND price_status='missing'",
            params![model_id, SHORT_INPUT, SHORT_CACHED, SHORT_OUTPUT, now],
        )?;
        if updated == 0 {
            continue;
        }
        conn.execute(
            "DELETE FROM model_price_tiers WHERE model_id=?1",
            [&model_id],
        )?;
        conn.execute(
            "INSERT INTO model_price_tiers(
               model_id,min_input_tokens,input_microusd_per_1m,
               cached_input_microusd_per_1m,output_microusd_per_1m
             ) VALUES(?1,0,?2,?3,?4)",
            params![model_id, SHORT_INPUT, SHORT_CACHED, SHORT_OUTPUT],
        )?;
        conn.execute(
            "INSERT INTO model_price_tiers(
               model_id,min_input_tokens,input_microusd_per_1m,
               cached_input_microusd_per_1m,output_microusd_per_1m
             ) VALUES(?1,?2,?3,?4,?5)",
            params![model_id, LONG_THRESHOLD, LONG_INPUT, LONG_CACHED, LONG_OUTPUT],
        )?;
    }
    Ok(())
}

'''
path.write_text(text[:start] + replacement + text[end:], encoding='utf-8')
print('rewrote helpers ok', start, end)
