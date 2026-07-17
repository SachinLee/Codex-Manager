use codexmanager_core::storage::{now_ts, ModelPriceRule, Storage};
use std::sync::{Mutex, OnceLock};

pub(crate) const PRICE_SEED_VERSION: &str = "2026-07-15";

#[derive(Debug, Clone, Copy)]
struct PriceSeed {
    provider: &'static str,
    model_pattern: &'static str,
    input_price_per_1m: f64,
    cached_input_price_per_1m: Option<f64>,
    output_price_per_1m: f64,
    long_context_threshold_tokens: Option<i64>,
    long_context_input_price_per_1m: Option<f64>,
    long_context_cached_input_price_per_1m: Option<f64>,
    long_context_output_price_per_1m: Option<f64>,
    source_url: &'static str,
}

#[derive(Debug, Clone)]
struct EnabledPriceRuleCache {
    db_path: String,
    rules: Vec<ModelPriceRule>,
}

static ENABLED_PRICE_RULE_CACHE: OnceLock<Mutex<Option<EnabledPriceRuleCache>>> = OnceLock::new();

#[derive(Debug, Clone)]
pub(crate) struct ModelPriceMatch {
    pub(crate) rule_id: String,
    pub(crate) model_pattern: String,
    pub(crate) price_source: String,
    pub(crate) match_quality: &'static str,
    pub(crate) billing_mode: &'static str,
    pub(crate) context_band: &'static str,
    pub(crate) long_context_threshold_tokens: Option<i64>,
    pub(crate) long_context_threshold_inclusive: bool,
    pub(crate) provider: String,
    pub(crate) short_input_price_per_1m: f64,
    pub(crate) short_cached_input_price_per_1m: f64,
    pub(crate) short_cache_write_price_per_1m: f64,
    pub(crate) short_output_price_per_1m: f64,
    pub(crate) input_price_per_1m: f64,
    pub(crate) cached_input_price_per_1m: f64,
    pub(crate) cache_write_price_per_1m: f64,
    pub(crate) output_price_per_1m: f64,
    pub(crate) cache_write_price_is_explicit: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct CostEstimate {
    pub(crate) provider: Option<String>,
    pub(crate) cost_usd: Option<f64>,
    pub(crate) price_status: &'static str,
    pub(crate) billing_mode: Option<&'static str>,
    pub(crate) context_band: &'static str,
    pub(crate) long_context_threshold_tokens: Option<i64>,
    pub(crate) long_context_threshold_inclusive: bool,
    pub(crate) matched_rule_id: Option<String>,
    pub(crate) matched_pattern: Option<String>,
    pub(crate) price_source: Option<String>,
    pub(crate) match_quality: Option<&'static str>,
    pub(crate) plain_input_cost_usd: Option<f64>,
    pub(crate) cached_input_cost_usd: Option<f64>,
    pub(crate) cache_write_cost_usd: Option<f64>,
    pub(crate) output_cost_usd: Option<f64>,
    pub(crate) short_baseline_cost_usd: Option<f64>,
    pub(crate) long_context_uplift_usd: Option<f64>,
    pub(crate) cost_source: Option<&'static str>,
    pub(crate) provider_cost_usd_ticks: Option<i64>,
    pub(crate) provider_cost_usd: Option<f64>,
    pub(crate) local_estimated_cost_usd: Option<f64>,
    pub(crate) pricing_variance_usd: Option<f64>,
}

impl CostEstimate {
    fn missing() -> Self {
        Self {
            provider: None,
            cost_usd: None,
            price_status: "missing",
            billing_mode: None,
            context_band: "unknown",
            long_context_threshold_tokens: None,
            long_context_threshold_inclusive: false,
            matched_rule_id: None,
            matched_pattern: None,
            price_source: None,
            match_quality: None,
            plain_input_cost_usd: None,
            cached_input_cost_usd: None,
            cache_write_cost_usd: None,
            output_cost_usd: None,
            short_baseline_cost_usd: None,
            long_context_uplift_usd: None,
            cost_source: None,
            provider_cost_usd_ticks: None,
            provider_cost_usd: None,
            local_estimated_cost_usd: None,
            pricing_variance_usd: None,
        }
    }

    pub(crate) fn multiplied(mut self, multiplier: f64) -> Self {
        let multiplier = if multiplier.is_finite() && multiplier > 0.0 {
            multiplier
        } else {
            1.0
        };
        for value in [
            &mut self.cost_usd,
            &mut self.plain_input_cost_usd,
            &mut self.cached_input_cost_usd,
            &mut self.cache_write_cost_usd,
            &mut self.output_cost_usd,
            &mut self.short_baseline_cost_usd,
            &mut self.long_context_uplift_usd,
        ] {
            if let Some(cost) = value.as_mut() {
                *cost *= multiplier;
            }
        }
        self
    }
}

const OPENAI_PRICE_SOURCE: &str = "https://developers.openai.com/api/docs/pricing";
const ANTHROPIC_PRICE_SOURCE: &str = "https://docs.claude.com/en/docs/about-claude/pricing";
const GEMINI_PRICE_SOURCE: &str = "https://ai.google.dev/gemini-api/docs/pricing";
const XAI_PRICE_SOURCE: &str = "https://docs.x.ai/developers/pricing";

const PRICE_SEEDS: &[PriceSeed] = &[
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.6-sol",
        input_price_per_1m: 5.0,
        cached_input_price_per_1m: Some(0.5),
        output_price_per_1m: 30.0,
        long_context_threshold_tokens: Some(272_000),
        long_context_input_price_per_1m: Some(10.0),
        long_context_cached_input_price_per_1m: Some(1.0),
        long_context_output_price_per_1m: Some(45.0),
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.6-terra",
        input_price_per_1m: 2.5,
        cached_input_price_per_1m: Some(0.25),
        output_price_per_1m: 15.0,
        long_context_threshold_tokens: Some(272_000),
        long_context_input_price_per_1m: Some(5.0),
        long_context_cached_input_price_per_1m: Some(0.5),
        long_context_output_price_per_1m: Some(22.5),
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.6-luna",
        input_price_per_1m: 1.0,
        cached_input_price_per_1m: Some(0.1),
        output_price_per_1m: 6.0,
        long_context_threshold_tokens: Some(272_000),
        long_context_input_price_per_1m: Some(2.0),
        long_context_cached_input_price_per_1m: Some(0.2),
        long_context_output_price_per_1m: Some(9.0),
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.6",
        input_price_per_1m: 5.0,
        cached_input_price_per_1m: Some(0.5),
        output_price_per_1m: 30.0,
        long_context_threshold_tokens: Some(272_000),
        long_context_input_price_per_1m: Some(10.0),
        long_context_cached_input_price_per_1m: Some(1.0),
        long_context_output_price_per_1m: Some(45.0),
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.5-pro",
        input_price_per_1m: 30.0,
        cached_input_price_per_1m: None,
        output_price_per_1m: 180.0,
        long_context_threshold_tokens: Some(272_000),
        long_context_input_price_per_1m: Some(60.0),
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: Some(270.0),
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.5",
        input_price_per_1m: 5.0,
        cached_input_price_per_1m: Some(0.5),
        output_price_per_1m: 30.0,
        long_context_threshold_tokens: Some(272_000),
        long_context_input_price_per_1m: Some(10.0),
        long_context_cached_input_price_per_1m: Some(1.0),
        long_context_output_price_per_1m: Some(45.0),
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.4-pro",
        input_price_per_1m: 30.0,
        cached_input_price_per_1m: None,
        output_price_per_1m: 180.0,
        long_context_threshold_tokens: Some(272_000),
        long_context_input_price_per_1m: Some(60.0),
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: Some(270.0),
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.4-mini",
        input_price_per_1m: 0.75,
        cached_input_price_per_1m: Some(0.075),
        output_price_per_1m: 4.5,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.4-nano",
        input_price_per_1m: 0.2,
        cached_input_price_per_1m: Some(0.02),
        output_price_per_1m: 1.25,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.4",
        input_price_per_1m: 2.5,
        cached_input_price_per_1m: Some(0.25),
        output_price_per_1m: 15.0,
        long_context_threshold_tokens: Some(272_000),
        long_context_input_price_per_1m: Some(5.0),
        long_context_cached_input_price_per_1m: Some(0.5),
        long_context_output_price_per_1m: Some(22.5),
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.3-codex",
        input_price_per_1m: 1.75,
        cached_input_price_per_1m: Some(0.175),
        output_price_per_1m: 14.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.2-pro",
        input_price_per_1m: 21.0,
        cached_input_price_per_1m: None,
        output_price_per_1m: 168.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.2",
        input_price_per_1m: 1.75,
        cached_input_price_per_1m: Some(0.175),
        output_price_per_1m: 14.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.1",
        input_price_per_1m: 1.25,
        cached_input_price_per_1m: Some(0.125),
        output_price_per_1m: 10.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5-pro",
        input_price_per_1m: 15.0,
        cached_input_price_per_1m: None,
        output_price_per_1m: 120.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5-mini",
        input_price_per_1m: 0.25,
        cached_input_price_per_1m: Some(0.025),
        output_price_per_1m: 2.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5-nano",
        input_price_per_1m: 0.05,
        cached_input_price_per_1m: Some(0.005),
        output_price_per_1m: 0.4,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5",
        input_price_per_1m: 1.25,
        cached_input_price_per_1m: Some(0.125),
        output_price_per_1m: 10.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-4.1",
        input_price_per_1m: 2.0,
        cached_input_price_per_1m: Some(0.5),
        output_price_per_1m: 8.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-4o",
        input_price_per_1m: 2.5,
        cached_input_price_per_1m: Some(1.25),
        output_price_per_1m: 10.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "o4-mini",
        input_price_per_1m: 1.1,
        cached_input_price_per_1m: Some(0.275),
        output_price_per_1m: 4.4,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "o3",
        input_price_per_1m: 2.0,
        cached_input_price_per_1m: Some(0.5),
        output_price_per_1m: 8.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "anthropic",
        model_pattern: "claude-opus-4.7",
        input_price_per_1m: 5.0,
        cached_input_price_per_1m: Some(0.5),
        output_price_per_1m: 25.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: ANTHROPIC_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "anthropic",
        model_pattern: "claude-opus-4.6",
        input_price_per_1m: 5.0,
        cached_input_price_per_1m: Some(0.5),
        output_price_per_1m: 25.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: ANTHROPIC_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "anthropic",
        model_pattern: "claude-opus-4.5",
        input_price_per_1m: 5.0,
        cached_input_price_per_1m: Some(0.5),
        output_price_per_1m: 25.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: ANTHROPIC_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "anthropic",
        model_pattern: "claude-opus-4",
        input_price_per_1m: 15.0,
        cached_input_price_per_1m: Some(1.5),
        output_price_per_1m: 75.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: ANTHROPIC_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "anthropic",
        model_pattern: "claude-sonnet-4",
        input_price_per_1m: 3.0,
        cached_input_price_per_1m: Some(0.3),
        output_price_per_1m: 15.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: ANTHROPIC_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "anthropic",
        model_pattern: "claude-haiku-4",
        input_price_per_1m: 1.0,
        cached_input_price_per_1m: Some(0.1),
        output_price_per_1m: 5.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: ANTHROPIC_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "google",
        model_pattern: "gemini-2.5-pro",
        input_price_per_1m: 1.25,
        cached_input_price_per_1m: Some(0.125),
        output_price_per_1m: 10.0,
        long_context_threshold_tokens: Some(200_000),
        long_context_input_price_per_1m: Some(2.5),
        long_context_cached_input_price_per_1m: Some(0.25),
        long_context_output_price_per_1m: Some(15.0),
        source_url: GEMINI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "google",
        model_pattern: "gemini-2.5-flash",
        input_price_per_1m: 0.3,
        cached_input_price_per_1m: Some(0.03),
        output_price_per_1m: 2.5,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: GEMINI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "google",
        model_pattern: "gemini-2.5-flash-lite",
        input_price_per_1m: 0.1,
        cached_input_price_per_1m: Some(0.01),
        output_price_per_1m: 0.4,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: GEMINI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "xai",
        model_pattern: "grok-4.5",
        input_price_per_1m: 2.0,
        cached_input_price_per_1m: Some(0.5),
        output_price_per_1m: 6.0,
        long_context_threshold_tokens: Some(200_000),
        long_context_input_price_per_1m: Some(4.0),
        long_context_cached_input_price_per_1m: Some(1.0),
        long_context_output_price_per_1m: Some(12.0),
        source_url: XAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "xai",
        model_pattern: "grok-4.5-latest",
        input_price_per_1m: 2.0,
        cached_input_price_per_1m: Some(0.5),
        output_price_per_1m: 6.0,
        long_context_threshold_tokens: Some(200_000),
        long_context_input_price_per_1m: Some(4.0),
        long_context_cached_input_price_per_1m: Some(1.0),
        long_context_output_price_per_1m: Some(12.0),
        source_url: XAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "xai",
        model_pattern: "grok-build-latest",
        input_price_per_1m: 2.0,
        cached_input_price_per_1m: Some(0.5),
        output_price_per_1m: 6.0,
        long_context_threshold_tokens: Some(200_000),
        long_context_input_price_per_1m: Some(4.0),
        long_context_cached_input_price_per_1m: Some(1.0),
        long_context_output_price_per_1m: Some(12.0),
        source_url: XAI_PRICE_SOURCE,
    },
];

const PRIORITY_PRICE_SEEDS: &[PriceSeed] = &[
    PriceSeed {
        provider: "xai",
        model_pattern: "grok-4.5",
        input_price_per_1m: 4.0,
        cached_input_price_per_1m: Some(1.0),
        output_price_per_1m: 12.0,
        long_context_threshold_tokens: Some(200_000),
        long_context_input_price_per_1m: Some(8.0),
        long_context_cached_input_price_per_1m: Some(2.0),
        long_context_output_price_per_1m: Some(24.0),
        source_url: XAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "xai",
        model_pattern: "grok-4.5-latest",
        input_price_per_1m: 4.0,
        cached_input_price_per_1m: Some(1.0),
        output_price_per_1m: 12.0,
        long_context_threshold_tokens: Some(200_000),
        long_context_input_price_per_1m: Some(8.0),
        long_context_cached_input_price_per_1m: Some(2.0),
        long_context_output_price_per_1m: Some(24.0),
        source_url: XAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "xai",
        model_pattern: "grok-build-latest",
        input_price_per_1m: 4.0,
        cached_input_price_per_1m: Some(1.0),
        output_price_per_1m: 12.0,
        long_context_threshold_tokens: Some(200_000),
        long_context_input_price_per_1m: Some(8.0),
        long_context_cached_input_price_per_1m: Some(2.0),
        long_context_output_price_per_1m: Some(24.0),
        source_url: XAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.6-sol",
        input_price_per_1m: 10.0,
        cached_input_price_per_1m: Some(1.0),
        output_price_per_1m: 60.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.6-terra",
        input_price_per_1m: 5.0,
        cached_input_price_per_1m: Some(0.5),
        output_price_per_1m: 30.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.6-luna",
        input_price_per_1m: 2.0,
        cached_input_price_per_1m: Some(0.2),
        output_price_per_1m: 12.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.6",
        input_price_per_1m: 10.0,
        cached_input_price_per_1m: Some(1.0),
        output_price_per_1m: 60.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.5",
        input_price_per_1m: 12.5,
        cached_input_price_per_1m: Some(1.25),
        output_price_per_1m: 75.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.4-mini",
        input_price_per_1m: 1.5,
        cached_input_price_per_1m: Some(0.15),
        output_price_per_1m: 9.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.4",
        input_price_per_1m: 5.0,
        cached_input_price_per_1m: Some(0.5),
        output_price_per_1m: 30.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
    PriceSeed {
        provider: "openai",
        model_pattern: "gpt-5.3-codex",
        input_price_per_1m: 3.5,
        cached_input_price_per_1m: Some(0.35),
        output_price_per_1m: 28.0,
        long_context_threshold_tokens: None,
        long_context_input_price_per_1m: None,
        long_context_cached_input_price_per_1m: None,
        long_context_output_price_per_1m: None,
        source_url: OPENAI_PRICE_SOURCE,
    },
];

fn official_family_prefix_matches(pattern: &str, model: &str) -> bool {
    model == pattern
        || model
            .strip_prefix(pattern)
            .is_some_and(|suffix| suffix.starts_with('-'))
}

fn seed_cache_write_price(seed: &PriceSeed, long_context: bool) -> Option<f64> {
    match (seed.model_pattern, seed.input_price_per_1m, long_context) {
        ("gpt-5.6" | "gpt-5.6-sol", 5.0, false) => Some(6.25),
        ("gpt-5.6" | "gpt-5.6-sol", 5.0, true) => Some(12.5),
        ("gpt-5.6" | "gpt-5.6-sol", 10.0, false) => Some(12.5),
        ("gpt-5.6-terra", 2.5, false) => Some(3.125),
        ("gpt-5.6-terra", 2.5, true) | ("gpt-5.6-terra", 5.0, false) => Some(6.25),
        ("gpt-5.6-luna", 1.0, false) => Some(1.25),
        ("gpt-5.6-luna", 1.0, true) | ("gpt-5.6-luna", 2.0, false) => Some(2.5),
        _ => None,
    }
}

pub(crate) fn normalize_billing_mode_for_service_tier(service_tier: Option<&str>) -> &'static str {
    match service_tier
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("fast" | "priority") => "priority",
        _ => "standard",
    }
}

pub(crate) fn infer_provider(model_pattern: &str) -> &str {
    let normalized = model_pattern.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return "openai";
    }
    PRICE_SEEDS
        .iter()
        .filter(|seed| official_family_prefix_matches(seed.model_pattern, &normalized))
        .max_by_key(|seed| seed.model_pattern.len())
        .map(|seed| seed.provider)
        .unwrap_or("openai")
}

pub(crate) fn ensure_official_price_seed(storage: &Storage) -> Result<(), String> {
    let now = now_ts();
    let rules = PRICE_SEEDS
        .iter()
        .enumerate()
        .map(|(index, seed)| official_price_seed(seed, "standard", index, now))
        .chain(
            PRIORITY_PRICE_SEEDS
                .iter()
                .enumerate()
                .map(|(index, seed)| official_price_seed(seed, "priority", index, now)),
        )
        .collect::<Vec<_>>();
    storage
        .replace_official_model_price_rules(&rules, PRICE_SEED_VERSION)
        .map_err(|err| format!("replace official model price seeds failed: {err}"))?;
    invalidate_price_rule_cache();
    Ok(())
}

fn official_price_seed_id(seed: &PriceSeed, billing_mode: &str) -> String {
    if billing_mode == "standard" {
        format!("official-{PRICE_SEED_VERSION}-{}", seed.model_pattern)
    } else {
        format!(
            "official-{PRICE_SEED_VERSION}-{billing_mode}-{}",
            seed.model_pattern
        )
    }
}

fn official_price_seed(
    seed: &PriceSeed,
    billing_mode: &str,
    index: usize,
    now: i64,
) -> ModelPriceRule {
    ModelPriceRule {
        id: official_price_seed_id(seed, billing_mode),
        provider: seed.provider.to_string(),
        model_pattern: seed.model_pattern.to_string(),
        match_type: "prefix".to_string(),
        billing_mode: billing_mode.to_string(),
        currency: "USD".to_string(),
        unit: "per_1m_tokens".to_string(),
        input_price_per_1m: Some(seed.input_price_per_1m),
        cached_input_price_per_1m: seed.cached_input_price_per_1m,
        cache_write_price_per_1m: seed_cache_write_price(seed, false),
        output_price_per_1m: Some(seed.output_price_per_1m),
        reasoning_output_price_per_1m: None,
        cache_write_5m_price_per_1m: None,
        cache_write_1h_price_per_1m: None,
        cache_hit_price_per_1m: None,
        long_context_threshold_tokens: seed.long_context_threshold_tokens,
        long_context_threshold_inclusive: seed.provider == "xai",
        long_context_input_price_per_1m: seed.long_context_input_price_per_1m,
        long_context_cached_input_price_per_1m: seed.long_context_cached_input_price_per_1m,
        long_context_cache_write_price_per_1m: seed_cache_write_price(seed, true),
        long_context_output_price_per_1m: seed.long_context_output_price_per_1m,
        source: "official_seed".to_string(),
        source_url: Some(seed.source_url.to_string()),
        seed_version: Some(PRICE_SEED_VERSION.to_string()),
        enabled: true,
        priority: 10_000 - index as i64,
        created_at: now,
        updated_at: now,
    }
}

pub(crate) fn load_enabled_price_rules(storage: &Storage) -> Result<Vec<ModelPriceRule>, String> {
    ensure_official_price_seed(storage)?;
    storage
        .list_enabled_model_price_rules()
        .map_err(|err| format!("list enabled model price rules failed: {err}"))
}

pub(crate) fn invalidate_price_rule_cache() {
    let mut cache = crate::lock_utils::lock_recover(
        ENABLED_PRICE_RULE_CACHE.get_or_init(|| Mutex::new(None)),
        "enabled_price_rule_cache",
    );
    *cache = None;
}

fn current_price_rule_cache_db_path() -> Option<String> {
    let db_path = std::env::var("CODEXMANAGER_DB_PATH").ok()?;
    let db_path = db_path.trim();
    if db_path.is_empty() || db_path == "<unset>" {
        return None;
    }
    Some(db_path.to_string())
}

fn load_enabled_price_rules_cached(storage: &Storage) -> Result<Vec<ModelPriceRule>, String> {
    let Some(db_path) = current_price_rule_cache_db_path() else {
        return load_enabled_price_rules(storage);
    };

    let cache_lock = ENABLED_PRICE_RULE_CACHE.get_or_init(|| Mutex::new(None));
    {
        let cache = crate::lock_utils::lock_recover(cache_lock, "enabled_price_rule_cache");
        if let Some(cached) = cache.as_ref().filter(|cached| cached.db_path == db_path) {
            return Ok(cached.rules.clone());
        }
    }

    let rules = load_enabled_price_rules(storage)?;
    let mut cache = crate::lock_utils::lock_recover(cache_lock, "enabled_price_rule_cache");
    *cache = Some(EnabledPriceRuleCache {
        db_path,
        rules: rules.clone(),
    });
    Ok(rules)
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

fn rule_matches(rule: &ModelPriceRule, normalized_model: &str) -> bool {
    let pattern = rule.model_pattern.trim().to_ascii_lowercase();
    if pattern.is_empty() {
        return false;
    }
    match rule.match_type.trim().to_ascii_lowercase().as_str() {
        "exact" => normalized_model == pattern,
        "glob" | "wildcard" => wildcard_matches(&pattern, normalized_model),
        "prefix" | "" if rule.source == "official_seed" => {
            official_family_prefix_matches(&pattern, normalized_model)
        }
        "prefix" | "" => normalized_model.starts_with(&pattern),
        _ if rule.source == "official_seed" => {
            official_family_prefix_matches(&pattern, normalized_model)
        }
        _ => normalized_model.starts_with(&pattern),
    }
}

fn rule_matches_billing_mode(rule: &ModelPriceRule, billing_mode: &str) -> bool {
    let normalized = rule.billing_mode.trim().to_ascii_lowercase();
    match billing_mode {
        "priority" => normalized == "priority",
        _ => normalized.is_empty() || normalized == "standard",
    }
}

fn price_from_rule(
    rule: &ModelPriceRule,
    input_tokens: i64,
    billing_mode: &'static str,
) -> Option<ModelPriceMatch> {
    if !rule.enabled
        || !rule.currency.eq_ignore_ascii_case("USD")
        || !rule.unit.eq_ignore_ascii_case("per_1m_tokens")
    {
        return None;
    }

    let short_input = rule.input_price_per_1m?;
    let short_cached = rule
        .cached_input_price_per_1m
        .or(rule.cache_hit_price_per_1m)
        .unwrap_or(short_input);
    let short_cache_write = rule.cache_write_price_per_1m.unwrap_or(short_input);
    let short_output = rule.output_price_per_1m?;
    let mut input = short_input;
    let mut cached = short_cached;
    let mut cache_write = short_cache_write;
    let mut cache_write_price_is_explicit = rule.cache_write_price_per_1m.is_some();
    let mut output = short_output;
    let is_long_context = rule.long_context_threshold_tokens.is_some_and(|threshold| {
        if rule.long_context_threshold_inclusive {
            input_tokens >= threshold
        } else {
            input_tokens > threshold
        }
    });

    if is_long_context {
        input = rule.long_context_input_price_per_1m.unwrap_or(input);
        cached = rule.long_context_cached_input_price_per_1m.unwrap_or(input);
        cache_write = rule.long_context_cache_write_price_per_1m.unwrap_or(input);
        cache_write_price_is_explicit = rule.long_context_cache_write_price_per_1m.is_some()
            || rule.cache_write_price_per_1m.is_some();
        output = rule.long_context_output_price_per_1m.unwrap_or(output);
    }

    Some(ModelPriceMatch {
        rule_id: rule.id.clone(),
        model_pattern: rule.model_pattern.clone(),
        price_source: rule.source.clone(),
        match_quality: if rule.match_type.eq_ignore_ascii_case("exact") {
            "exact"
        } else if rule.source == "official_seed" {
            "family"
        } else {
            "fallback"
        },
        billing_mode,
        context_band: if is_long_context {
            "long"
        } else if billing_mode == "priority" {
            "single_tier"
        } else {
            "short"
        },
        long_context_threshold_tokens: rule.long_context_threshold_tokens,
        long_context_threshold_inclusive: rule.long_context_threshold_inclusive,
        provider: rule.provider.clone(),
        short_input_price_per_1m: short_input,
        short_cached_input_price_per_1m: short_cached,
        short_cache_write_price_per_1m: short_cache_write,
        short_output_price_per_1m: short_output,
        input_price_per_1m: input,
        cached_input_price_per_1m: cached,
        cache_write_price_per_1m: cache_write,
        output_price_per_1m: output,
        cache_write_price_is_explicit,
    })
}

pub(crate) fn resolve_model_price_from_rules(
    rules: &[ModelPriceRule],
    model: &str,
    input_tokens: i64,
) -> Option<ModelPriceMatch> {
    resolve_model_price_from_rules_by_mode(rules, model, input_tokens, "standard")
}

fn resolve_model_price_from_rules_by_mode(
    rules: &[ModelPriceRule],
    model: &str,
    input_tokens: i64,
    billing_mode: &str,
) -> Option<ModelPriceMatch> {
    let normalized = model.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized == "unknown" {
        return None;
    }

    let matched = rules
        .iter()
        .filter(|rule| rule_matches_billing_mode(rule, billing_mode))
        .filter(|rule| rule_matches(rule, &normalized))
        .max_by_key(|rule| (rule.priority, rule.model_pattern.len() as i64))?;

    let pricing_billing_mode = if billing_mode == "priority" {
        "priority"
    } else {
        "standard"
    };
    price_from_rule(matched, input_tokens, pricing_billing_mode)
}

pub(crate) fn resolve_model_price_from_rules_for_billing_mode(
    rules: &[ModelPriceRule],
    model: &str,
    service_tier: Option<&str>,
    input_tokens: i64,
) -> Option<ModelPriceMatch> {
    let billing_mode = normalize_billing_mode_for_service_tier(service_tier);
    if billing_mode == "priority" {
        return resolve_model_price_from_rules_by_mode(rules, model, input_tokens, "priority")
            .or_else(|| {
                resolve_model_price_from_rules_by_mode(rules, model, input_tokens, "standard")
            });
    }
    resolve_model_price_from_rules_by_mode(rules, model, input_tokens, "standard")
}

pub(crate) fn resolve_model_price(model: &str, input_tokens: i64) -> Option<ModelPriceMatch> {
    resolve_model_price_from_seeds(PRICE_SEEDS, model, input_tokens, "standard")
}

fn resolve_model_price_from_seeds(
    seeds: &[PriceSeed],
    model: &str,
    input_tokens: i64,
    billing_mode: &'static str,
) -> Option<ModelPriceMatch> {
    let normalized = model.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized == "unknown" {
        return None;
    }

    let matched = seeds
        .iter()
        .filter(|seed| official_family_prefix_matches(seed.model_pattern, &normalized))
        .max_by_key(|seed| seed.model_pattern.len())?;

    let short_input = matched.input_price_per_1m;
    let short_cached = matched
        .cached_input_price_per_1m
        .unwrap_or(matched.input_price_per_1m);
    let short_output = matched.output_price_per_1m;
    let mut long_context = false;
    let mut input = short_input;
    let mut cached = short_cached;
    let mut output = short_output;

    if matched.long_context_threshold_tokens.is_some_and(|threshold| {
        if matched.provider == "xai" {
            input_tokens >= threshold
        } else {
            input_tokens > threshold
        }
    })
    {
        long_context = true;
        input = matched
            .long_context_input_price_per_1m
            .unwrap_or(matched.input_price_per_1m);
        cached = matched
            .long_context_cached_input_price_per_1m
            .unwrap_or(input);
        output = matched
            .long_context_output_price_per_1m
            .unwrap_or(matched.output_price_per_1m);
    }

    let short_cache_write = seed_cache_write_price(matched, false).unwrap_or(short_input);
    let cache_write_price_per_1m = seed_cache_write_price(matched, long_context).unwrap_or(input);

    Some(ModelPriceMatch {
        rule_id: official_price_seed_id(matched, billing_mode),
        model_pattern: matched.model_pattern.to_string(),
        price_source: "official_seed".to_string(),
        match_quality: if normalized == matched.model_pattern {
            "exact"
        } else {
            "family"
        },
        billing_mode,
        context_band: if long_context {
            "long"
        } else if billing_mode == "priority" {
            "single_tier"
        } else {
            "short"
        },
        long_context_threshold_tokens: matched.long_context_threshold_tokens,
        long_context_threshold_inclusive: matched.provider == "xai",
        provider: matched.provider.to_string(),
        short_input_price_per_1m: short_input,
        short_cached_input_price_per_1m: short_cached,
        short_cache_write_price_per_1m: short_cache_write,
        short_output_price_per_1m: short_output,
        input_price_per_1m: input,
        cached_input_price_per_1m: cached,
        cache_write_price_per_1m,
        output_price_per_1m: output,
        cache_write_price_is_explicit: seed_cache_write_price(matched, long_context).is_some(),
    })
}

pub(crate) fn resolve_model_price_for_billing_mode(
    model: &str,
    service_tier: Option<&str>,
    input_tokens: i64,
) -> Option<ModelPriceMatch> {
    if normalize_billing_mode_for_service_tier(service_tier) == "priority" {
        return resolve_model_price_from_seeds(
            PRIORITY_PRICE_SEEDS,
            model,
            input_tokens,
            "priority",
        )
        .or_else(|| resolve_model_price(model, input_tokens));
    }
    resolve_model_price(model, input_tokens)
}

fn estimate_cost_from_price(
    price: ModelPriceMatch,
    input_tokens: i64,
    cached_input_tokens: i64,
    cache_write_input_tokens: i64,
    output_tokens: i64,
    total_tokens: Option<i64>,
    reasoning_output_tokens: Option<i64>,
) -> CostEstimate {
    let input_total = input_tokens.max(0) as f64;
    let cached_input = (cached_input_tokens.max(0) as f64).min(input_total);
    let cache_write_input =
        (cache_write_input_tokens.max(0) as f64).min(input_total - cached_input);
    let billable_input = (input_total - cached_input - cache_write_input).max(0.0);
    let raw_output_tokens = output_tokens.max(0);
    let normalized_output_tokens = if price.provider == "xai" {
        let input_tokens = input_tokens.max(0);
        match total_tokens.filter(|total| *total >= input_tokens) {
            Some(total) => raw_output_tokens.max(total.saturating_sub(input_tokens)),
            None => raw_output_tokens.saturating_add(reasoning_output_tokens.unwrap_or(0).max(0)),
        }
    } else {
        raw_output_tokens
    };
    let output = normalized_output_tokens as f64;
    let plain_input_cost = (billable_input / 1_000_000.0) * price.input_price_per_1m;
    let cached_input_cost = (cached_input / 1_000_000.0) * price.cached_input_price_per_1m;
    let cache_write_cost = (cache_write_input / 1_000_000.0) * price.cache_write_price_per_1m;
    let output_cost = (output / 1_000_000.0) * price.output_price_per_1m;
    let cost = plain_input_cost + cached_input_cost + cache_write_cost + output_cost;
    let short_baseline_cost = (billable_input / 1_000_000.0) * price.short_input_price_per_1m
        + (cached_input / 1_000_000.0) * price.short_cached_input_price_per_1m
        + (cache_write_input / 1_000_000.0) * price.short_cache_write_price_per_1m
        + (output / 1_000_000.0) * price.short_output_price_per_1m;
    let price_status = if cache_write_input > 0.0 && !price.cache_write_price_is_explicit {
        "partial"
    } else {
        "ok"
    };
    let long_context_uplift = if price.context_band == "long" && price_status == "ok" {
        Some((cost - short_baseline_cost).max(0.0))
    } else {
        None
    };

    CostEstimate {
        provider: Some(price.provider),
        cost_usd: Some(cost.max(0.0)),
        price_status,
        billing_mode: Some(price.billing_mode),
        context_band: price.context_band,
        long_context_threshold_tokens: price.long_context_threshold_tokens,
        long_context_threshold_inclusive: price.long_context_threshold_inclusive,
        matched_rule_id: Some(price.rule_id),
        matched_pattern: Some(price.model_pattern),
        price_source: Some(price.price_source),
        match_quality: Some(price.match_quality),
        plain_input_cost_usd: Some(plain_input_cost),
        cached_input_cost_usd: Some(cached_input_cost),
        cache_write_cost_usd: Some(cache_write_cost),
        output_cost_usd: Some(output_cost),
        short_baseline_cost_usd: if price.context_band == "long" && price_status == "ok" {
            Some(short_baseline_cost)
        } else {
            None
        },
        long_context_uplift_usd: long_context_uplift,
        cost_source: Some("local_estimate"),
        provider_cost_usd_ticks: None,
        provider_cost_usd: None,
        local_estimated_cost_usd: Some(cost.max(0.0)),
        pricing_variance_usd: None,
    }
}

pub(crate) fn estimate_cost(
    model: Option<&str>,
    input_tokens: i64,
    cached_input_tokens: i64,
    cache_write_input_tokens: i64,
    output_tokens: i64,
) -> CostEstimate {
    let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) else {
        return CostEstimate::missing();
    };
    let Some(price) = resolve_model_price(model, input_tokens.max(0)) else {
        return CostEstimate::missing();
    };

    estimate_cost_from_price(
        price,
        input_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        output_tokens,
        None,
        None,
    )
}

pub(crate) fn estimate_cost_with_rules(
    rules: &[ModelPriceRule],
    model: Option<&str>,
    input_tokens: i64,
    cached_input_tokens: i64,
    cache_write_input_tokens: i64,
    output_tokens: i64,
) -> CostEstimate {
    estimate_cost_with_rules_for_billing_mode(
        rules,
        model,
        None,
        input_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        output_tokens,
    )
}

pub(crate) fn estimate_cost_with_rules_for_billing_mode(
    rules: &[ModelPriceRule],
    model: Option<&str>,
    service_tier: Option<&str>,
    input_tokens: i64,
    cached_input_tokens: i64,
    cache_write_input_tokens: i64,
    output_tokens: i64,
) -> CostEstimate {
    let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) else {
        return CostEstimate::missing();
    };

    let Some(price) = resolve_model_price_from_rules_for_billing_mode(
        rules,
        model,
        service_tier,
        input_tokens.max(0),
    )
    .or_else(|| resolve_model_price_for_billing_mode(model, service_tier, input_tokens.max(0))) else {
        return CostEstimate::missing();
    };

    estimate_cost_from_price(
        price,
        input_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        output_tokens,
        None,
        None,
    )
}

pub(crate) fn estimate_remaining_tokens_from_usd_with_rules(
    rules: &[ModelPriceRule],
    model: &str,
    balance_usd: f64,
) -> Option<i64> {
    if !balance_usd.is_finite() || balance_usd < 0.0 {
        return None;
    }
    let price = resolve_model_price_from_rules(rules, model, 0)
        .or_else(|| resolve_model_price(model, 0))?;
    if balance_usd == 0.0 {
        return Some(0);
    }
    let blended_price_per_1m = price.input_price_per_1m * 0.7 + price.output_price_per_1m * 0.3;
    if blended_price_per_1m <= 0.0 {
        return None;
    }
    Some(((balance_usd / blended_price_per_1m) * 1_000_000.0).floor() as i64)
}

pub(crate) fn estimate_cost_usd_for_log(
    storage: &Storage,
    model: Option<&str>,
    input_tokens: Option<i64>,
    cached_input_tokens: Option<i64>,
    cache_write_input_tokens: Option<i64>,
    output_tokens: Option<i64>,
) -> f64 {
    estimate_cost_usd_for_log_with_tier(
        storage,
        model,
        None,
        input_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        output_tokens,
    )
}

pub(crate) fn estimate_cost_usd_for_log_with_tier(
    storage: &Storage,
    model: Option<&str>,
    service_tier: Option<&str>,
    input_tokens: Option<i64>,
    cached_input_tokens: Option<i64>,
    cache_write_input_tokens: Option<i64>,
    output_tokens: Option<i64>,
) -> f64 {
    estimate_cost_for_log_with_tier(
        storage,
        model,
        service_tier,
        input_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        output_tokens,
    )
    .cost_usd
    .unwrap_or(0.0)
}

pub(crate) fn estimate_cost_for_log_with_tier(
    storage: &Storage,
    model: Option<&str>,
    service_tier: Option<&str>,
    input_tokens: Option<i64>,
    cached_input_tokens: Option<i64>,
    cache_write_input_tokens: Option<i64>,
    output_tokens: Option<i64>,
) -> CostEstimate {
    estimate_cost_for_log_with_usage_and_tier(
        storage,
        model,
        service_tier,
        input_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        output_tokens,
        None,
        None,
    )
}

pub(crate) fn estimate_cost_for_log_with_usage_and_tier(
    storage: &Storage,
    model: Option<&str>,
    service_tier: Option<&str>,
    input_tokens: Option<i64>,
    cached_input_tokens: Option<i64>,
    cache_write_input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    total_tokens: Option<i64>,
    reasoning_output_tokens: Option<i64>,
) -> CostEstimate {
    let input = input_tokens.unwrap_or(0);
    let cached = cached_input_tokens.unwrap_or(0);
    let cache_write = cache_write_input_tokens.unwrap_or(0);
    let output = output_tokens.unwrap_or(0);
    load_enabled_price_rules_cached(storage)
        .ok()
        .filter(|rules| !rules.is_empty())
        .map(|rules| {
            let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) else {
                return CostEstimate::missing();
            };
            let Some(price) = resolve_model_price_from_rules_for_billing_mode(
                &rules,
                model,
                service_tier,
                input.max(0),
            )
            .or_else(|| resolve_model_price_for_billing_mode(model, service_tier, input.max(0))) else {
                return CostEstimate::missing();
            };
            estimate_cost_from_price(
                price,
                input,
                cached,
                cache_write,
                output,
                total_tokens,
                reasoning_output_tokens,
            )
        })
        .unwrap_or_else(|| {
            let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) else {
                return CostEstimate::missing();
            };
            let Some(price) =
                resolve_model_price_for_billing_mode(model, service_tier, input.max(0))
            else {
                return CostEstimate::missing();
            };
            estimate_cost_from_price(
                price,
                input,
                cached,
                cache_write,
                output,
                total_tokens,
                reasoning_output_tokens,
            )
        })
}

#[cfg(test)]
#[path = "model_pricing_tests.rs"]
mod tests;
