use super::*;

fn collect_query_plan_details_with_params(
    storage: &Storage,
    sql: &str,
    params: Vec<Value>,
) -> Vec<String> {
    let mut stmt = storage.conn.prepare(sql).expect("prepare explain");
    let mut rows = stmt.query(params_from_iter(params)).expect("query explain");
    collect_query_plan_rows(&mut rows)
}

fn collect_query_plan_rows(rows: &mut rusqlite::Rows<'_>) -> Vec<String> {
    let mut details = Vec::new();
    while let Some(row) = rows.next().expect("next explain row") {
        let detail: String = row.get(3).expect("detail");
        details.push(detail.to_ascii_lowercase());
    }
    details
}

fn price_rule(id: &str, model_pattern: &str, source: &str, priority: i64) -> ModelPriceRule {
    ModelPriceRule {
        id: id.to_string(),
        provider: "openai".to_string(),
        model_pattern: model_pattern.to_string(),
        match_type: "exact".to_string(),
        billing_mode: "standard".to_string(),
        currency: "USD".to_string(),
        unit: "per_1m_tokens".to_string(),
        input_price_per_1m: Some(1.0),
        cached_input_price_per_1m: None,
        cache_write_price_per_1m: None,
        output_price_per_1m: Some(2.0),
        reasoning_output_price_per_1m: None,
        cache_write_5m_price_per_1m: None,
        cache_write_1h_price_per_1m: None,
        cache_hit_price_per_1m: None,
        long_context_threshold_tokens: None,
        long_context_threshold_inclusive: false,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_cache_write_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source: source.to_string(),
        source_url: None,
        seed_version: None,
        enabled: true,
        priority,
        created_at: 1,
        updated_at: 1,
    }
}

#[test]
fn count_model_price_rules_for_seed_uses_source_seed_index() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            model_price_rule_count_for_seed_sql()
        ),
        vec![Value::Text("2026-06".to_string())],
    );

    assert!(
        details
            .iter()
            .any(|detail| detail.contains("idx_model_price_rules_source_seed")),
        "expected seed count to use source/seed index, got {details:?}"
    );
}
#[test]
fn find_enabled_custom_exact_model_price_rule_uses_lookup_index() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let details = collect_query_plan_details_with_params(
        &storage,
        &format!(
            "EXPLAIN QUERY PLAN {}",
            enabled_custom_exact_model_price_rule_sql()
        ),
        vec![
            Value::Text("gpt-5".to_string()),
            Value::Text("standard".to_string()),
        ],
    );

    assert!(
        details
            .iter()
            .any(|detail| detail.contains("idx_model_price_rules_custom_exact_lookup")),
        "expected custom exact lookup to use index, got {details:?}"
    );
}

#[test]
fn find_enabled_custom_exact_model_price_rule_filters_in_sql() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let official = price_rule("official", "gpt-5", "official_seed", 30_000);
    let low = price_rule("custom-low", "gpt-5", "custom", 100);
    let high = price_rule("custom-high", "GPT-5", "custom", 200);
    storage
        .upsert_model_price_rule(&official)
        .expect("insert official");
    storage
        .upsert_model_price_rule(&low)
        .expect("insert low custom");
    storage
        .upsert_model_price_rule(&high)
        .expect("insert high custom");

    let rule = storage
        .find_enabled_custom_exact_model_price_rule(" gpt-5 ")
        .expect("find rule")
        .expect("rule exists");

    assert_eq!(rule.id, "custom-high");
}

#[test]
fn model_price_rule_schema_closure_repairs_the_colliding_custom_113_shape() {
    let storage = Storage::open_in_memory().expect("open");
    storage
        .conn
        .execute_batch(include_str!("../../migrations/055_model_price_rules.sql"))
        .expect("create pre-custom-113 price-rule table");
    storage
        .conn
        .execute(
            "INSERT INTO model_price_rules(
                id,provider,model_pattern,match_type,billing_mode,currency,unit,
                input_price_per_1m,cached_input_price_per_1m,output_price_per_1m,
                source,enabled,priority,created_at,updated_at
             ) VALUES('custom-existing','openai','gpt-custom','exact','standard','USD',
                'per_1m_tokens',1.0,0.1,2.0,'custom',1,1,1,1)",
            [],
        )
        .expect("insert custom rule");

    storage
        .ensure_model_price_rules_table()
        .expect("close legacy schema");

    for column in [
        "cache_write_price_per_1m",
        "cache_write_5m_price_per_1m",
        "cache_write_1h_price_per_1m",
        "cache_hit_price_per_1m",
        "long_context_threshold_inclusive",
        "long_context_cache_write_price_per_1m",
    ] {
        assert!(
            storage.has_column("model_price_rules", column).unwrap(),
            "{column}"
        );
    }
    let rule = storage
        .list_enabled_model_price_rules()
        .expect("read repaired custom rule")
        .pop()
        .expect("custom rule remains");
    assert_eq!(rule.id, "custom-existing");
    assert_eq!(rule.cache_write_price_per_1m, None);
    assert!(!rule.long_context_threshold_inclusive);
}

#[test]
fn find_enabled_custom_exact_model_price_rule_filters_billing_mode() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let standard = price_rule("custom-standard", "gpt-5", "custom", 100);
    let mut priority = price_rule("custom-priority", "gpt-5", "custom", 100);
    priority.billing_mode = "priority".to_string();
    storage
        .upsert_model_price_rule(&standard)
        .expect("insert standard");
    storage
        .upsert_model_price_rule(&priority)
        .expect("insert priority");

    let standard_rule = storage
        .find_enabled_custom_exact_model_price_rule_for_billing_mode("gpt-5", "standard")
        .expect("find standard")
        .expect("standard exists");
    let priority_rule = storage
        .find_enabled_custom_exact_model_price_rule_for_billing_mode("gpt-5", "priority")
        .expect("find priority")
        .expect("priority exists");

    assert_eq!(standard_rule.id, "custom-standard");
    assert_eq!(priority_rule.id, "custom-priority");
}

#[test]
fn enabled_model_price_rule_pattern_lookup_uses_index() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let sql = enabled_model_price_rule_patterns_for_patterns_chunk_sql(
        "LOWER(TRIM(model_pattern)) IN ('gpt-5', 'claude-test')",
    );
    let details = collect_query_plan_details_with_params(
        &storage,
        &format!("EXPLAIN QUERY PLAN {sql}"),
        vec![Value::Integer(1)],
    );

    assert!(
        details
            .iter()
            .any(|detail| detail.contains("idx_model_price_rules_enabled_pattern_lookup")),
        "expected enabled pattern lookup to use index, got {details:?}"
    );
    assert!(
        !details
            .iter()
            .any(|detail| detail.contains("use temp b-tree for order by")),
        "enabled pattern lookup chunk should avoid per-chunk ORDER BY temp sorting, got {details:?}"
    );
}

#[test]
fn list_enabled_model_price_rule_patterns_filters_in_sql() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let mut enabled = price_rule("enabled", " GPT-5 ", "official_seed", 1);
    let mut disabled = price_rule("disabled", "claude-disabled", "custom", 1);
    disabled.enabled = false;
    storage
        .upsert_model_price_rule(&enabled)
        .expect("insert enabled");
    storage
        .upsert_model_price_rule(&disabled)
        .expect("insert disabled");

    let patterns = storage
        .list_enabled_model_price_rule_patterns_for_patterns(&[
            "gpt-5".to_string(),
            "CLAUDE-DISABLED".to_string(),
            "missing".to_string(),
        ])
        .expect("list patterns");

    assert_eq!(patterns, vec!["gpt-5".to_string()]);

    enabled.enabled = false;
    storage
        .upsert_model_price_rule(&enabled)
        .expect("disable enabled");
    let patterns = storage
        .list_enabled_model_price_rule_patterns_for_patterns(&["gpt-5".to_string()])
        .expect("list disabled patterns");
    assert!(patterns.is_empty());
}

#[test]
fn replacing_official_price_rules_disables_stale_seeds_without_touching_custom_rules() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let mut old_seed = price_rule("old-seed", "gpt-5", "official_seed", 10_000);
    old_seed.seed_version = Some("2026-05-11".to_string());
    storage
        .upsert_model_price_rule(&old_seed)
        .expect("insert old seed");

    let custom = price_rule("custom-rule", "gpt-5", "custom", 20_000);
    storage
        .upsert_model_price_rule(&custom)
        .expect("insert custom rule");

    let mut current_seed = price_rule("current-seed", "gpt-5.6", "official_seed", 10_000);
    current_seed.seed_version = Some("2026-07-10".to_string());
    storage
        .replace_official_model_price_rules(&[current_seed.clone()], "2026-07-10")
        .expect("replace official seeds");

    let enabled = storage
        .list_enabled_model_price_rules()
        .expect("list enabled rules");
    assert!(enabled.iter().any(|rule| rule.id == current_seed.id));
    assert!(enabled.iter().any(|rule| rule.id == custom.id));
    assert!(!enabled.iter().any(|rule| rule.id == old_seed.id));
}
