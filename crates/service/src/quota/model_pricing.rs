use codexmanager_core::storage::{ModelPriceTierV2, Storage};

#[derive(Debug, Clone)]
pub(crate) struct CatalogModelPrice {
    pub(crate) model_slug: String,
    pub(crate) provider: String,
    pub(crate) price_status: String,
    pub(crate) tiers: Vec<ModelPriceTierV2>,
}

#[derive(Debug, Clone)]
pub(crate) struct ModelPriceMatch {
    pub(crate) provider: String,
    pub(crate) input_price_per_1m: f64,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) cached_input_price_per_1m: f64,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) cache_write_price_per_1m: f64,
    pub(crate) output_price_per_1m: f64,
}

#[cfg(test)]
#[derive(Debug, Clone)]
pub(crate) struct CostEstimate {
    pub(crate) provider: Option<String>,
    pub(crate) cost_usd: Option<f64>,
    pub(crate) price_status: &'static str,
}

pub(crate) fn infer_provider(model_pattern: &str) -> &str {
    let normalized = model_pattern.trim().to_ascii_lowercase();
    if normalized.starts_with("claude") {
        "anthropic"
    } else if normalized.starts_with("gemini") {
        "google"
    } else if normalized.starts_with("gpt")
        || normalized.starts_with('o')
        || normalized.starts_with("codex")
    {
        "openai"
    } else {
        "custom"
    }
}

pub(crate) fn wildcard_matches(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == value;
    }
    let mut remainder = value;
    let mut first = true;
    for part in pattern.split('*').filter(|part| !part.is_empty()) {
        if first && !pattern.starts_with('*') {
            let Some(stripped) = remainder.strip_prefix(part) else {
                return false;
            };
            remainder = stripped;
            first = false;
            continue;
        }
        first = false;
        let Some(index) = remainder.find(part) else {
            return false;
        };
        remainder = &remainder[index + part.len()..];
    }
    pattern.ends_with('*') || remainder.is_empty()
}

pub(crate) fn load_catalog_prices(storage: &Storage) -> Result<Vec<CatalogModelPrice>, String> {
    storage
        .list_managed_models_v2(true)
        .map_err(|err| format!("list model catalog V2 prices failed: {err}"))
        .map(|models| {
            models
                .into_iter()
                .map(|model| CatalogModelPrice {
                    provider: model
                        .provider
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or_else(|| infer_provider(model.slug.as_str()).to_string()),
                    model_slug: model.slug,
                    price_status: model.price.price_status,
                    tiers: model.price_tiers,
                })
                .collect()
        })
}

pub(crate) fn resolve_model_price_from_catalog(
    prices: &[CatalogModelPrice],
    model: &str,
    input_tokens: i64,
) -> Option<ModelPriceMatch> {
    resolve_model_price_from_catalog_with_long_context_billing(
        prices,
        model,
        input_tokens,
        crate::app_settings::current_gateway_long_context_billing_enabled(),
    )
}

pub(crate) fn resolve_model_price_from_catalog_with_long_context_billing(
    prices: &[CatalogModelPrice],
    model: &str,
    input_tokens: i64,
    long_context_billing_enabled: bool,
) -> Option<ModelPriceMatch> {
    let normalized = model.trim();
    if normalized.is_empty() || normalized.eq_ignore_ascii_case("unknown") {
        return None;
    }
    let price = prices
        .iter()
        .find(|price| price.model_slug.eq_ignore_ascii_case(normalized))?;
    if price.price_status == "missing" {
        return None;
    }
    let tier = price
        .tiers
        .iter()
        .filter(|tier| {
            tier.min_input_tokens == 0
                || (long_context_billing_enabled && tier.min_input_tokens <= input_tokens.max(0))
        })
        .max_by_key(|tier| tier.min_input_tokens)?;
    Some(ModelPriceMatch {
        provider: price.provider.clone(),
        input_price_per_1m: tier.input_microusd_per_1m as f64 / 1_000_000.0,
        cached_input_price_per_1m: tier.cached_input_microusd_per_1m as f64 / 1_000_000.0,
        cache_write_price_per_1m: tier
            .cache_write_microusd_per_1m
            .unwrap_or(tier.input_microusd_per_1m) as f64
            / 1_000_000.0,
        output_price_per_1m: tier.output_microusd_per_1m as f64 / 1_000_000.0,
    })
}

#[cfg(test)]
fn estimate_cost_from_price(
    price: ModelPriceMatch,
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
) -> CostEstimate {
    let input_total = input_tokens.max(0) as f64;
    let cached_input = (cached_input_tokens.max(0) as f64).min(input_total);
    let billable_input = (input_total - cached_input).max(0.0);
    let output = output_tokens.max(0) as f64;
    let cost = (billable_input / 1_000_000.0) * price.input_price_per_1m
        + (cached_input / 1_000_000.0) * price.cached_input_price_per_1m
        + (output / 1_000_000.0) * price.output_price_per_1m;
    CostEstimate {
        provider: Some(price.provider),
        cost_usd: Some(cost.max(0.0)),
        price_status: "ok",
    }
}

#[cfg(test)]
pub(crate) fn estimate_cost_with_catalog(
    prices: &[CatalogModelPrice],
    model: Option<&str>,
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
) -> CostEstimate {
    let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) else {
        return CostEstimate {
            provider: None,
            cost_usd: None,
            price_status: "missing",
        };
    };
    let Some(price) = resolve_model_price_from_catalog(prices, model, input_tokens) else {
        let provider = prices
            .iter()
            .find(|price| price.model_slug.eq_ignore_ascii_case(model))
            .map(|price| price.provider.clone());
        return CostEstimate {
            provider,
            cost_usd: None,
            price_status: "missing",
        };
    };
    estimate_cost_from_price(price, input_tokens, cached_input_tokens, output_tokens)
}

pub(crate) fn estimate_remaining_tokens_from_usd_with_catalog(
    prices: &[CatalogModelPrice],
    model: &str,
    balance_usd: f64,
) -> Option<i64> {
    if !balance_usd.is_finite() || balance_usd < 0.0 {
        return None;
    }
    if balance_usd == 0.0 {
        return Some(0);
    }
    let price = resolve_model_price_from_catalog(prices, model, 0)?;
    let blended_price_per_1m = price.input_price_per_1m * 0.7 + price.output_price_per_1m * 0.3;
    if blended_price_per_1m <= 0.0 {
        return None;
    }
    Some(((balance_usd / blended_price_per_1m) * 1_000_000.0).floor() as i64)
}

/// Estimate a request-log amount from the model catalog v2 only.  The gateway
/// uses this for observability events; final charging remains owned by the
/// immutable v2 charge snapshot path.
pub(crate) fn estimate_cost_usd_for_log(
    storage: &Storage,
    model: Option<&str>,
    input_tokens: Option<i64>,
    cached_input_tokens: Option<i64>,
    cache_write_input_tokens: Option<i64>,
    output_tokens: Option<i64>,
) -> f64 {
    estimate_cost_usd_for_log_with_long_context_billing(
        storage,
        model,
        input_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        output_tokens,
        crate::app_settings::current_gateway_long_context_billing_enabled(),
    )
}

pub(crate) fn estimate_cost_usd_for_log_with_long_context_billing(
    storage: &Storage,
    model: Option<&str>,
    input_tokens: Option<i64>,
    cached_input_tokens: Option<i64>,
    cache_write_input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    long_context_billing_enabled: bool,
) -> f64 {
    let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) else {
        return 0.0;
    };
    let input = input_tokens.unwrap_or(0).max(0);
    let cached = cached_input_tokens.unwrap_or(0).clamp(0, input);
    let cache_write = cache_write_input_tokens
        .unwrap_or(0)
        .clamp(0, input.saturating_sub(cached));
    let output = output_tokens.unwrap_or(0).max(0);
    let Ok(prices) = load_catalog_prices(storage) else {
        return 0.0;
    };
    let Some(price) = resolve_model_price_from_catalog_with_long_context_billing(
        &prices,
        model,
        input,
        long_context_billing_enabled,
    ) else {
        return 0.0;
    };
    // Cache reads and writes are disjoint classifications of total input. The
    // catalog has no dedicated cache-write tier yet, so writes fall back to
    // ordinary input pricing without being counted a second time.
    let plain_input = input.saturating_sub(cached).saturating_sub(cache_write);
    let total = (plain_input.saturating_add(cache_write)) as f64 * price.input_price_per_1m
        + cached as f64 * price.cached_input_price_per_1m
        + output as f64 * price.output_price_per_1m;
    (total / 1_000_000.0).max(0.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AggregateApiSpendPricingState {
    Quoted,
    UnboundedOutput,
    UnpricedModel,
}

impl AggregateApiSpendPricingState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Quoted => "quoted",
            Self::UnboundedOutput => "unbounded_output",
            Self::UnpricedModel => "unpriced_model",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AggregateApiSpendQuote {
    pub pricing_state: AggregateApiSpendPricingState,
    pub microusd: i64,
}

/// Deterministic pre-dispatch quote for one Aggregate API upstream attempt.
/// Cache hits are assumed to be zero for a conservative local reservation;
/// the output bound (when known) is billed at the output rate, and the
/// Aggregate API multiplier is applied once.
pub(crate) fn quote_aggregate_api_attempt_spend(
    storage: &Storage,
    model: Option<&str>,
    input_tokens: i64,
    output_bound_tokens: Option<i64>,
    rate_multiplier_millis: i64,
) -> AggregateApiSpendQuote {
    let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) else {
        return AggregateApiSpendQuote {
            pricing_state: AggregateApiSpendPricingState::UnpricedModel,
            microusd: 0,
        };
    };
    let input = input_tokens.max(0);
    let output = output_bound_tokens.map(|value| value.max(0)).unwrap_or(0);
    let Ok(prices) = load_catalog_prices(storage) else {
        return AggregateApiSpendQuote {
            pricing_state: AggregateApiSpendPricingState::UnpricedModel,
            microusd: 0,
        };
    };
    let Some(price) = resolve_model_price_from_catalog_with_long_context_billing(
        &prices,
        model,
        input,
        crate::app_settings::current_gateway_long_context_billing_enabled(),
    ) else {
        return AggregateApiSpendQuote {
            pricing_state: AggregateApiSpendPricingState::UnpricedModel,
            microusd: 0,
        };
    };
    let raw_usd =
        (input as f64 * price.input_price_per_1m + output as f64 * price.output_price_per_1m)
            / 1_000_000.0;
    let multiplier = (rate_multiplier_millis.max(0) as f64) / 1_000.0;
    let usd = raw_usd * multiplier;
    let microusd = if usd.is_finite() && usd > 0.0 {
        (usd * 1_000_000.0).ceil().max(0.0) as i64
    } else {
        0
    };
    let pricing_state = if output_bound_tokens.is_none() {
        AggregateApiSpendPricingState::UnboundedOutput
    } else {
        AggregateApiSpendPricingState::Quoted
    };
    AggregateApiSpendQuote {
        pricing_state,
        microusd,
    }
}

/// Settled micro-USD amount for a completed upstream attempt using its actual
/// usage tokens. Applies the Aggregate API multiplier once. Used by Guard
/// settlements and known-billable failure settlements; final success uses the
/// authoritative `ChargeSnapshotV2.charged_cost_microusd`.
pub(crate) fn settle_aggregate_api_usage_microusd(
    storage: &Storage,
    model: Option<&str>,
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
    rate_multiplier_millis: i64,
) -> i64 {
    let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) else {
        return 0;
    };
    let input = input_tokens.max(0);
    let cached = cached_input_tokens.max(0).min(input);
    let output = output_tokens.max(0);
    let Ok(prices) = load_catalog_prices(storage) else {
        return 0;
    };
    let Some(price) = resolve_model_price_from_catalog_with_long_context_billing(
        &prices,
        model,
        input,
        crate::app_settings::current_gateway_long_context_billing_enabled(),
    ) else {
        return 0;
    };
    let plain_input = input.saturating_sub(cached);
    let raw_usd = (plain_input as f64 * price.input_price_per_1m
        + cached as f64 * price.cached_input_price_per_1m
        + output as f64 * price.output_price_per_1m)
        / 1_000_000.0;
    let multiplier = (rate_multiplier_millis.max(0) as f64) / 1_000.0;
    let usd = raw_usd * multiplier;
    if usd.is_finite() && usd > 0.0 {
        (usd * 1_000_000.0).ceil().max(0.0) as i64
    } else {
        0
    }
}

#[cfg(test)]
mod spend_quote_tests {
    use super::*;

    fn open_storage() -> Storage {
        let path = std::env::temp_dir().join(format!(
            "codexmanager-pricing-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let storage = Storage::open(&path).expect("open storage");
        storage.init().expect("init storage");
        storage
    }

    #[test]
    fn quote_reserves_known_input_and_output_bound_with_multiplier() {
        let storage = open_storage();
        let quote = quote_aggregate_api_attempt_spend(
            &storage,
            Some("gpt-5.4"),
            1_000_000,
            Some(100_000),
            1_500,
        );
        assert_eq!(quote.pricing_state, AggregateApiSpendPricingState::Quoted);
        assert!(quote.microusd > 0);
    }

    #[test]
    fn quote_marks_unbounded_output_and_unpriced_model() {
        let storage = open_storage();
        let unbounded = quote_aggregate_api_attempt_spend(
            &storage,
            Some("gpt-5.4"),
            100_000,
            None,
            1_000,
        );
        assert_eq!(
            unbounded.pricing_state,
            AggregateApiSpendPricingState::UnboundedOutput
        );
        let unpriced = quote_aggregate_api_attempt_spend(
            &storage,
            Some("not-a-real-model"),
            100_000,
            Some(1_000),
            1_000,
        );
        assert_eq!(
            unpriced.pricing_state,
            AggregateApiSpendPricingState::UnpricedModel
        );
        assert_eq!(unpriced.microusd, 0);
    }
}

#[cfg(test)]
#[path = "model_pricing_tests.rs"]
mod tests;
