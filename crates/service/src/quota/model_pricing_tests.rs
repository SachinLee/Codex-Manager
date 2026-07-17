use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

fn test_rule(
    id: &str,
    model_pattern: &str,
    match_type: &str,
    billing_mode: &str,
    priority: i64,
    input: f64,
    cached: Option<f64>,
    output: f64,
) -> ModelPriceRule {
    ModelPriceRule {
        id: id.to_string(),
        provider: "test".to_string(),
        model_pattern: model_pattern.to_string(),
        match_type: match_type.to_string(),
        billing_mode: billing_mode.to_string(),
        currency: "USD".to_string(),
        unit: "per_1m_tokens".to_string(),
        input_price_per_1m: Some(input),
        cached_input_price_per_1m: cached,
        cache_write_price_per_1m: None,
        output_price_per_1m: Some(output),
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
        source: "test".to_string(),
        source_url: None,
        seed_version: None,
        enabled: true,
        priority,
        created_at: 0,
        updated_at: 0,
    }
}

fn assert_close(actual: f64, expected: f64) {
    let delta = (actual - expected).abs();
    assert!(
        delta < 0.000_000_1,
        "expected {expected}, got {actual}, delta {delta}"
    );
}

fn isolated_test_db_path(name: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let mut path = std::env::temp_dir();
    path.push(format!(
        "codexmanager-{name}-{}-{nanos}.sqlite",
        std::process::id()
    ));
    path.to_string_lossy().into_owned()
}

struct EnvGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(value) = self.previous.as_deref() {
            std::env::set_var(self.key, value);
        } else {
            std::env::remove_var(self.key);
        }
        invalidate_price_rule_cache();
    }
}

#[test]
fn resolves_exact_and_wildcard_database_rules() {
    let rules = vec![
        test_rule(
            "wild",
            "vendor-*-mini",
            "wildcard",
            "standard",
            10,
            1.0,
            Some(0.1),
            2.0,
        ),
        test_rule(
            "exact",
            "vendor-model-mini",
            "exact",
            "standard",
            100,
            3.0,
            Some(0.3),
            4.0,
        ),
    ];
    let exact = resolve_model_price_from_rules(&rules, "vendor-model-mini", 0).expect("exact rule");
    assert_close(exact.input_price_per_1m, 3.0);
    assert_close(exact.cached_input_price_per_1m, 0.3);
    assert_close(exact.output_price_per_1m, 4.0);

    let wildcard =
        resolve_model_price_from_rules(&rules, "vendor-other-mini", 0).expect("wildcard rule");
    assert_close(wildcard.input_price_per_1m, 1.0);
    assert_close(wildcard.output_price_per_1m, 2.0);
}

#[test]
fn resolves_exact_and_snapshot_models() {
    let exact = resolve_model_price("gpt-5.4-mini", 0).expect("exact price");
    assert_eq!(exact.provider, "openai");
    assert_close(exact.input_price_per_1m, 0.75);
    assert_close(exact.cached_input_price_per_1m, 0.075);
    assert_close(exact.output_price_per_1m, 4.5);

    let snapshot = resolve_model_price("gpt-5.4-mini-2026-03-17", 0).expect("snapshot price");
    assert_close(snapshot.input_price_per_1m, 0.75);
    assert_close(snapshot.output_price_per_1m, 4.5);
}

#[test]
fn prefers_more_specific_prefix_for_latest_claude_opus() {
    let latest = resolve_model_price("claude-opus-4.7-20260219", 0).expect("latest opus price");
    assert_eq!(latest.provider, "anthropic");
    assert_close(latest.input_price_per_1m, 5.0);
    assert_close(latest.cached_input_price_per_1m, 0.5);
    assert_close(latest.output_price_per_1m, 25.0);

    let legacy = resolve_model_price("claude-opus-4-20250514", 0).expect("opus 4 price");
    assert_close(legacy.input_price_per_1m, 15.0);
    assert_close(legacy.output_price_per_1m, 75.0);
}

#[test]
fn returns_missing_for_unknown_models() {
    assert!(resolve_model_price("unknown-provider-model", 0).is_none());
    let cost = estimate_cost(Some("unknown-provider-model"), 100, 0, 0, 100);
    assert_eq!(cost.price_status, "missing");
    assert!(cost.cost_usd.is_none());
    assert!(cost.provider.is_none());
}

#[test]
fn zero_usd_balance_is_known_zero_tokens() {
    let tokens = estimate_remaining_tokens_from_usd_with_rules(&[], "gpt-5.4-mini", 0.0);
    assert_eq!(tokens, Some(0));
}

#[test]
fn estimates_cost_with_cached_input_discount() {
    let cost = estimate_cost(Some("gpt-5.4"), 1_000, 400, 0, 100);
    assert_eq!(cost.price_status, "ok");
    assert_eq!(cost.provider.as_deref(), Some("openai"));
    assert_close(cost.cost_usd.expect("cost"), 0.0031);
}

#[test]
fn falls_back_cached_input_to_input_price_when_no_discount_exists() {
    let cost = estimate_cost(Some("gpt-5.5-pro"), 1_000, 200, 0, 100);
    assert_eq!(cost.price_status, "ok");
    assert_close(cost.cost_usd.expect("cost"), 0.048);
}

#[test]
fn applies_openai_long_context_pricing_at_threshold() {
    let standard = resolve_model_price("gpt-5.4", 271_999).expect("standard price");
    assert_close(standard.input_price_per_1m, 2.5);
    assert_close(standard.output_price_per_1m, 15.0);

    let boundary = resolve_model_price("gpt-5.4", 272_000).expect("boundary price");
    assert_close(boundary.input_price_per_1m, 2.5);

    let long_context = resolve_model_price("gpt-5.4", 272_001).expect("long context price");
    assert_close(long_context.input_price_per_1m, 5.0);
    assert_close(long_context.cached_input_price_per_1m, 0.5);
    assert_close(long_context.output_price_per_1m, 22.5);
}

#[test]
fn gpt_56_standard_prices_cover_alias_variants_and_context_boundary() {
    let alias = resolve_model_price("gpt-5.6", 272_000).expect("gpt-5.6 alias price");
    assert_close(alias.input_price_per_1m, 5.0);
    assert_close(alias.cached_input_price_per_1m, 0.5);
    assert_close(alias.cache_write_price_per_1m, 6.25);
    assert_close(alias.output_price_per_1m, 30.0);

    let sol = resolve_model_price("gpt-5.6-sol-2026-07-01", 272_001).expect("sol price");
    assert_close(sol.input_price_per_1m, 10.0);
    assert_close(sol.cached_input_price_per_1m, 1.0);
    assert_close(sol.cache_write_price_per_1m, 12.5);
    assert_close(sol.output_price_per_1m, 45.0);

    let terra = resolve_model_price("gpt-5.6-terra", 0).expect("terra price");
    assert_close(terra.input_price_per_1m, 2.5);
    assert_close(terra.cached_input_price_per_1m, 0.25);
    assert_close(terra.cache_write_price_per_1m, 3.125);
    assert_close(terra.output_price_per_1m, 15.0);

    let luna = resolve_model_price("gpt-5.6-luna", 272_001).expect("luna price");
    assert_close(luna.input_price_per_1m, 2.0);
    assert_close(luna.cached_input_price_per_1m, 0.2);
    assert_close(luna.cache_write_price_per_1m, 2.5);
    assert_close(luna.output_price_per_1m, 9.0);
}

#[test]
fn gpt_56_priority_prices_do_not_invent_long_context_override() {
    let sol = resolve_model_price_for_billing_mode("gpt-5.6-sol", Some("priority"), 272_001)
        .expect("priority sol price");
    assert_close(sol.input_price_per_1m, 10.0);
    assert_close(sol.cached_input_price_per_1m, 1.0);
    assert_close(sol.cache_write_price_per_1m, 12.5);
    assert_close(sol.output_price_per_1m, 60.0);

    let terra = resolve_model_price_for_billing_mode("gpt-5.6-terra", Some("fast"), 272_001)
        .expect("priority terra price");
    assert_close(terra.input_price_per_1m, 5.0);
    assert_close(terra.cached_input_price_per_1m, 0.5);
    assert_close(terra.cache_write_price_per_1m, 6.25);
    assert_close(terra.output_price_per_1m, 30.0);

    let luna = resolve_model_price_for_billing_mode("gpt-5.6-luna", Some("priority"), 272_001)
        .expect("priority luna price");
    assert_close(luna.input_price_per_1m, 2.0);
    assert_close(luna.cached_input_price_per_1m, 0.2);
    assert_close(luna.cache_write_price_per_1m, 2.5);
    assert_close(luna.output_price_per_1m, 12.0);
}

#[test]
fn cache_write_cost_partitions_total_input_without_double_counting() {
    let cost = estimate_cost(Some("gpt-5.6"), 1_000_000, 250_000, 125_000, 100_000);
    assert_eq!(cost.price_status, "ok");
    // 1M input enters the published long-context tier.
    assert_close(cost.cost_usd.expect("cost"), 12.5625);

    let clamped = estimate_cost(Some("gpt-5.6"), 1_000, 900, 500, 0);
    assert_eq!(clamped.price_status, "ok");
    assert_close(clamped.cost_usd.expect("clamped cost"), 0.001075);
}

#[test]
fn long_context_estimate_exposes_breakdown_and_short_price_uplift() {
    let estimate = estimate_cost(Some("gpt-5.6-sol"), 300_000, 200_000, 10_000, 1_000);

    assert_eq!(estimate.context_band, "long");
    assert_eq!(estimate.billing_mode, Some("standard"));
    assert_eq!(estimate.long_context_threshold_tokens, Some(272_000));
    assert_eq!(estimate.matched_pattern.as_deref(), Some("gpt-5.6-sol"));
    assert_close(estimate.plain_input_cost_usd.expect("plain"), 0.9);
    assert_close(estimate.cached_input_cost_usd.expect("cached"), 0.2);
    assert_close(estimate.cache_write_cost_usd.expect("write"), 0.125);
    assert_close(estimate.output_cost_usd.expect("output"), 0.045);
    assert_close(estimate.cost_usd.expect("total"), 1.27);
    assert_close(
        estimate.short_baseline_cost_usd.expect("short baseline"),
        0.6425,
    );
    assert_close(estimate.long_context_uplift_usd.expect("uplift"), 0.6275);

    let priority = estimate_cost_with_rules_for_billing_mode(
        &[],
        Some("gpt-5.6-sol"),
        Some("priority"),
        300_000,
        0,
        0,
        1_000,
    );
    assert_eq!(priority.context_band, "single_tier");
    assert!(priority.long_context_uplift_usd.is_none());
}

#[test]
fn gpt_56_does_not_match_the_older_gpt_5_official_family() {
    let price = resolve_model_price("gpt-5.6-future", 0).expect("new minor family should resolve");
    assert_close(price.input_price_per_1m, 5.0);

    let unknown_minor = resolve_model_price("gpt-5.7", 0);
    assert!(unknown_minor.is_none());
}

#[test]
fn estimate_cost_usd_for_log_reuses_cached_enabled_price_rules_until_invalidated() {
    let _lock = crate::test_env_guard();
    invalidate_price_rule_cache();
    let _guard = EnvGuard::set(
        "CODEXMANAGER_DB_PATH",
        &isolated_test_db_path("price-cache-test"),
    );
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let now = now_ts();
    let mut rule = test_rule(
        "cached-rule",
        "cache-model",
        "exact",
        "standard",
        50_000,
        1.0,
        Some(1.0),
        1.0,
    );
    rule.created_at = now;
    rule.updated_at = now;
    storage
        .upsert_model_price_rule(&rule)
        .expect("insert first price rule");

    let first = estimate_cost_usd_for_log(
        &storage,
        Some("cache-model"),
        Some(1_000_000),
        None,
        None,
        None,
    );
    assert_close(first, 1.0);

    rule.input_price_per_1m = Some(2.0);
    rule.cached_input_price_per_1m = Some(2.0);
    rule.updated_at = now + 1;
    storage
        .upsert_model_price_rule(&rule)
        .expect("update price rule");

    let cached = estimate_cost_usd_for_log(
        &storage,
        Some("cache-model"),
        Some(1_000_000),
        None,
        None,
        None,
    );
    assert_close(cached, 1.0);

    invalidate_price_rule_cache();
    let refreshed = estimate_cost_usd_for_log(
        &storage,
        Some("cache-model"),
        Some(1_000_000),
        None,
        None,
        None,
    );
    assert_close(refreshed, 2.0);
}

#[test]
fn priority_billing_mode_prefers_priority_rule() {
    let rules = vec![
        test_rule(
            "standard",
            "tiered-model",
            "exact",
            "standard",
            50,
            1.0,
            Some(0.1),
            2.0,
        ),
        test_rule(
            "priority",
            "tiered-model",
            "exact",
            "priority",
            50,
            3.0,
            Some(0.3),
            6.0,
        ),
    ];

    let price =
        resolve_model_price_from_rules_for_billing_mode(&rules, "tiered-model", Some("fast"), 0)
            .expect("priority rule");

    assert_close(price.input_price_per_1m, 3.0);
    assert_close(price.cached_input_price_per_1m, 0.3);
    assert_close(price.output_price_per_1m, 6.0);
}

#[test]
fn priority_billing_mode_falls_back_to_standard_rule() {
    let rules = vec![test_rule(
        "standard",
        "fallback-model",
        "exact",
        "standard",
        50,
        1.0,
        Some(0.1),
        2.0,
    )];

    let price = resolve_model_price_from_rules_for_billing_mode(
        &rules,
        "fallback-model",
        Some("priority"),
        0,
    )
    .expect("standard fallback");

    assert_close(price.input_price_per_1m, 1.0);
    assert_close(price.cached_input_price_per_1m, 0.1);
    assert_close(price.output_price_per_1m, 2.0);
}

#[test]
fn standard_billing_mode_does_not_use_priority_rule() {
    let rules = vec![test_rule(
        "priority",
        "priority-only-model",
        "exact",
        "priority",
        50,
        3.0,
        Some(0.3),
        6.0,
    )];

    let price =
        resolve_model_price_from_rules_for_billing_mode(&rules, "priority-only-model", None, 0);

    assert!(price.is_none());
}

#[test]
fn official_priority_prices_are_mode_specific() {
    let standard =
        resolve_model_price_for_billing_mode("gpt-5.5", None, 0).expect("standard gpt-5.5");
    assert_close(standard.input_price_per_1m, 5.0);
    assert_close(standard.cached_input_price_per_1m, 0.5);
    assert_close(standard.output_price_per_1m, 30.0);

    let priority =
        resolve_model_price_for_billing_mode("gpt-5.5", Some("fast"), 0).expect("priority gpt-5.5");
    assert_close(priority.input_price_per_1m, 12.5);
    assert_close(priority.cached_input_price_per_1m, 1.25);
    assert_close(priority.output_price_per_1m, 75.0);

    let gpt54 = resolve_model_price_for_billing_mode("gpt-5.4", Some("priority"), 0)
        .expect("priority gpt-5.4");
    assert_close(gpt54.input_price_per_1m, 5.0);
    assert_close(gpt54.cached_input_price_per_1m, 0.5);
    assert_close(gpt54.output_price_per_1m, 30.0);

    let gpt54_mini = resolve_model_price_for_billing_mode("gpt-5.4-mini", Some("fast"), 0)
        .expect("priority gpt-5.4-mini");
    assert_close(gpt54_mini.input_price_per_1m, 1.5);
    assert_close(gpt54_mini.cached_input_price_per_1m, 0.15);
    assert_close(gpt54_mini.output_price_per_1m, 9.0);

    let codex = resolve_model_price_for_billing_mode("gpt-5.3-codex", Some("priority"), 0)
        .expect("priority gpt-5.3-codex");
    assert_close(codex.input_price_per_1m, 3.5);
    assert_close(codex.cached_input_price_per_1m, 0.35);
    assert_close(codex.output_price_per_1m, 28.0);
}

#[test]
fn grok_4_5_uses_xai_prices_at_the_inclusive_200k_boundary() {
    for alias in ["grok-4.5", "grok-4.5-latest", "grok-build-latest"] {
        let short = resolve_model_price(alias, 199_999).expect("short xai price");
        assert_eq!(short.provider, "xai");
        assert_eq!(short.context_band, "short");
        assert!(short.long_context_threshold_inclusive);
        assert_close(short.input_price_per_1m, 2.0);
        assert_close(short.cached_input_price_per_1m, 0.5);
        assert_close(short.output_price_per_1m, 6.0);

        let long = resolve_model_price(alias, 200_000).expect("long xai price");
        assert_eq!(long.context_band, "long");
        assert_close(long.input_price_per_1m, 4.0);
        assert_close(long.cached_input_price_per_1m, 1.0);
        assert_close(long.output_price_per_1m, 12.0);
    }
}

#[test]
fn grok_4_5_priority_prices_are_double_the_standard_price() {
    let priority = resolve_model_price_for_billing_mode("grok-4.5", Some("priority"), 200_000)
        .expect("priority xai price");
    assert_eq!(priority.provider, "xai");
    assert_eq!(priority.context_band, "long");
    assert_close(priority.input_price_per_1m, 8.0);
    assert_close(priority.cached_input_price_per_1m, 2.0);
    assert_close(priority.output_price_per_1m, 24.0);
}

#[test]
fn non_xai_long_context_thresholds_remain_strictly_exclusive() {
    let at_threshold = resolve_model_price("gpt-5.6", 272_000).expect("gpt price");
    assert_eq!(at_threshold.context_band, "short");
    assert!(!at_threshold.long_context_threshold_inclusive);

    let over_threshold = resolve_model_price("gpt-5.6", 272_001).expect("gpt price");
    assert_eq!(over_threshold.context_band, "long");
}

#[test]
fn official_grok_seed_outranks_a_zero_cost_aggregate_placeholder() {
    let mut zero_placeholder = test_rule(
        "aggregate-placeholder",
        "grok-4.5",
        "exact",
        "standard",
        -10,
        0.0,
        Some(0.0),
        0.0,
    );
    zero_placeholder.source = "aggregate_api_sync".to_string();
    let official_seed = PRICE_SEEDS
        .iter()
        .enumerate()
        .find(|(_, seed)| seed.model_pattern == "grok-4.5")
        .map(|(index, seed)| official_price_seed(seed, "standard", index, 1))
        .expect("official grok seed");

    let price = resolve_model_price_from_rules(&[zero_placeholder, official_seed], "grok-4.5", 0)
        .expect("resolved grok price");
    assert_eq!(price.provider, "xai");
    assert_eq!(price.price_source, "official_seed");
    assert_close(price.input_price_per_1m, 2.0);
}

#[test]
fn grok_local_fallback_bills_reasoning_once() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    let without_total = estimate_cost_for_log_with_usage_and_tier(
        &storage,
        Some("grok-4.5"),
        None,
        Some(1_000),
        Some(0),
        Some(0),
        Some(100),
        None,
        Some(50),
    );
    assert_close(
        without_total.output_cost_usd.expect("output with reasoning"),
        0.0009,
    );

    let with_total = estimate_cost_for_log_with_usage_and_tier(
        &storage,
        Some("grok-4.5"),
        None,
        Some(1_000),
        Some(0),
        Some(0),
        Some(100),
        Some(1_200),
        Some(50),
    );
    assert_close(
        with_total.output_cost_usd.expect("output from total"),
        0.0012,
    );
}
