use super::*;
use codexmanager_core::storage::{ManagedModelV2Upsert, Storage};

fn assert_close(actual: f64, expected: f64) {
    let delta = (actual - expected).abs();
    assert!(
        delta < 0.000_000_1,
        "expected {expected}, got {actual}, delta {delta}"
    );
}

fn prices() -> (Storage, Vec<CatalogModelPrice>) {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let prices = load_catalog_prices(&storage).expect("load V2 prices");
    (storage, prices)
}

#[test]
fn catalog_prices_are_exact_and_missing_prices_do_not_fallback() {
    let (_storage, prices) = prices();
    assert_eq!(prices.len(), 8);
    let mini = resolve_model_price_from_catalog(&prices, "gpt-5.4-mini", 0).expect("mini");
    assert_eq!(mini.provider, "openai");
    assert_close(mini.input_price_per_1m, 0.75);
    assert_close(mini.cached_input_price_per_1m, 0.075);
    assert_close(mini.output_price_per_1m, 4.5);
    assert!(resolve_model_price_from_catalog(&prices, "gpt-5.4-mini-snapshot", 0).is_none());
    let sol = resolve_model_price_from_catalog(&prices, "gpt-5.6-sol", 0).expect("sol");
    assert_close(sol.input_price_per_1m, 5.0);
    assert_close(sol.cached_input_price_per_1m, 5.0);
    assert_close(sol.output_price_per_1m, 30.0);
    let terra = resolve_model_price_from_catalog(&prices, "gpt-5.6-terra", 0).expect("terra");
    assert_close(terra.input_price_per_1m, 2.5);
    assert_close(terra.cached_input_price_per_1m, 2.5);
    assert_close(terra.output_price_per_1m, 15.0);
    let luna = resolve_model_price_from_catalog(&prices, "gpt-5.6-luna", 0).expect("luna");
    assert_close(luna.input_price_per_1m, 1.0);
    assert_close(luna.cached_input_price_per_1m, 1.0);
    assert_close(luna.output_price_per_1m, 6.0);
    assert!(resolve_model_price_from_catalog(&prices, "codex-auto-review", 0).is_none());
    assert!(resolve_model_price_from_catalog(&prices, "unknown-provider-model", 0).is_none());
}

#[test]
fn catalog_price_switches_at_272k_boundary() {
    let (_storage, prices) = prices();
    let standard = resolve_model_price_from_catalog(&prices, "gpt-5.4", 271_999).expect("standard");
    assert_close(standard.input_price_per_1m, 2.5);
    assert_close(standard.output_price_per_1m, 15.0);
    let long = resolve_model_price_from_catalog(&prices, "gpt-5.4", 272_000).expect("long");
    assert_close(long.input_price_per_1m, 5.0);
    assert_close(long.cached_input_price_per_1m, 0.5);
    assert_close(long.output_price_per_1m, 22.5);
}

#[test]
fn catalog_cost_uses_cached_subset_once() {
    let (_storage, prices) = prices();
    let cost = estimate_cost_with_catalog(&prices, Some("gpt-5.4"), 1_000, 400, 100);
    assert_eq!(cost.price_status, "ok");
    assert_eq!(cost.provider.as_deref(), Some("openai"));
    assert_close(cost.cost_usd.expect("cost"), 0.0031);
}

#[test]
fn request_log_estimate_does_not_double_count_cache_write_tokens() {
    let (storage, _) = prices();
    let without_cache_write = estimate_cost_usd_for_log(
        &storage,
        Some("gpt-5.4"),
        Some(1_000),
        Some(200),
        None,
        Some(100),
    );
    let with_cache_write = estimate_cost_usd_for_log(
        &storage,
        Some("gpt-5.4"),
        Some(1_000),
        Some(200),
        Some(100),
        Some(100),
    );

    assert_close(with_cache_write, without_cache_write);
}

#[test]
fn zero_balance_is_known_and_positive_balance_uses_gpt56_price() {
    let (_storage, prices) = prices();
    assert_eq!(
        estimate_remaining_tokens_from_usd_with_catalog(&prices, "gpt-5.6-sol", 0.0),
        Some(0)
    );
    assert_eq!(
        estimate_remaining_tokens_from_usd_with_catalog(&prices, "gpt-5.6-sol", 1.0),
        Some(80_000)
    );
}

#[test]
fn price_edits_are_read_from_db_without_runtime_cache() {
    let (storage, prices) = prices();
    let before = resolve_model_price_from_catalog(&prices, "gpt-5.4-mini", 0).expect("before");
    assert_close(before.input_price_per_1m, 0.75);

    let mut model = storage
        .get_managed_model_v2("gpt-5.4-mini")
        .expect("read model")
        .expect("model");
    model.price.price_status = "custom".to_string();
    model.price.input_microusd_per_1m = Some(2_000_000);
    model.price.cached_input_microusd_per_1m = Some(2_000_000);
    model.price_tiers[0].input_microusd_per_1m = 2_000_000;
    model.price_tiers[0].cached_input_microusd_per_1m = 2_000_000;
    storage
        .upsert_managed_model_v2(&ManagedModelV2Upsert {
            previous_slug: Some(model.slug.clone()),
            model,
        })
        .expect("update model price");

    let refreshed = load_catalog_prices(&storage).expect("reload V2 prices");
    let after = resolve_model_price_from_catalog(&refreshed, "gpt-5.4-mini", 0).expect("after");
    assert_close(after.input_price_per_1m, 2.0);
}

#[test]
fn wildcard_match_remains_available_for_billing_multipliers() {
    assert!(wildcard_matches("gpt-*-mini", "gpt-5.4-mini"));
    assert!(!wildcard_matches("gpt-*-mini", "gpt-5.4"));
}
