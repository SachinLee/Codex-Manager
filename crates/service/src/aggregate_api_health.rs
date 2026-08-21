use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use codexmanager_core::rpc::types::{
    AggregateApiHealthConfigResult, AggregateApiHealthDetailResult, AggregateApiHealthEventResult,
    AggregateApiHealthListResult, AggregateApiHealthStateResult, AggregateApiProbeCostListResult,
    AggregateApiProbeCostSummaryResult, AggregateApiTestResult,
};
use codexmanager_core::storage::{
    now_ts, AggregateApiHealthConfig, AggregateApiHealthEvent, AggregateApiHealthState,
    AggregateApiProbeCost,
};

use crate::storage_helpers::open_storage;

const FAILURE_THRESHOLD: i64 = 5;
const TRANSIENT_COOLDOWN_SECS: i64 = 5 * 60;
const AUTH_COOLDOWN_SECS: i64 = 30 * 60;
const MODEL_UNSUPPORTED_COOLDOWN_SECS: i64 = 12 * 60 * 60;
const DEGRADED_LATENCY_MS: i64 = 6_000;
const MAX_AUTOMATIC_PROBES_PER_DAY: i64 = 288;

fn optional_scope(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn config_result(config: AggregateApiHealthConfig) -> AggregateApiHealthConfigResult {
    AggregateApiHealthConfigResult {
        aggregate_api_id: config.aggregate_api_id,
        enabled: config.enabled,
        probe_interval_secs: config.probe_interval_secs,
        probe_timeout_ms: config.probe_timeout_ms,
        probe_model: config.probe_model,
        last_scheduled_at: config.last_scheduled_at,
        next_probe_at: config.next_probe_at,
    }
}

fn state_result(state: AggregateApiHealthState) -> AggregateApiHealthStateResult {
    AggregateApiHealthStateResult {
        aggregate_api_id: state.aggregate_api_id,
        upstream_model: state.upstream_model,
        protocol: state.protocol,
        state: state.state,
        consecutive_failures: state.consecutive_failures,
        failure_threshold: state.failure_threshold,
        cooldown_until: state.cooldown_until,
        last_observed_at: state.last_observed_at,
        last_probe_at: state.last_probe_at,
        last_success_at: state.last_success_at,
        last_failure_at: state.last_failure_at,
        latency_ms: state.last_latency_ms,
        http_status: state.last_http_status,
        error_category: state.last_error_category,
        error_reason: state.last_error_reason,
        observation_source: state.last_observation_source,
        active_probe_enabled: false,
        probe_model: None,
        available_probe_models: Vec::new(),
    }
}

fn available_probe_models(
    storage: &codexmanager_core::storage::Storage,
    api_id: &str,
) -> Vec<String> {
    let Ok(models) = storage.list_managed_models_v2(true) else {
        return Vec::new();
    };
    let mut values = models
        .into_iter()
        .flat_map(|model| model.routes)
        .filter(|route| {
            route.enabled
                && route.source_kind == "aggregate_api"
                && route.source_id == api_id
                && !route.upstream_model.trim().is_empty()
        })
        .map(|route| route.upstream_model.trim().to_string())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn event_result(event: AggregateApiHealthEvent) -> AggregateApiHealthEventResult {
    AggregateApiHealthEventResult {
        aggregate_api_id: event.aggregate_api_id,
        upstream_model: event.upstream_model,
        protocol: event.protocol,
        trigger: event.trigger,
        outcome: event.outcome,
        state_before: event.state_before,
        state_after: event.state_after,
        error_category: event.error_category,
        http_status: event.http_status,
        latency_ms: event.latency_ms,
        reason: event.reason,
        observed_at: event.observed_at,
        cooldown_until: event.cooldown_until,
    }
}

fn sanitize_reason(reason: Option<&str>) -> Option<String> {
    let reason = reason?.trim();
    if reason.is_empty() {
        return None;
    }
    let lower = reason.to_ascii_lowercase();
    if lower.contains("authorization")
        || lower.contains("api key")
        || lower.contains("bearer ")
        || lower.contains("token=")
    {
        return Some("upstream returned a credential-related error".to_string());
    }
    Some(reason.chars().take(240).collect())
}

/// 读取聚合 API 的连续失败冻结开关；读取失败或 API 不存在时按默认开启处理。
fn aggregate_api_consecutive_freeze_enabled_or(
    storage: &codexmanager_core::storage::Storage,
    api_id: &str,
) -> bool {
    storage
        .aggregate_api_consecutive_freeze_enabled(api_id)
        .ok()
        .flatten()
        .unwrap_or(true)
}

fn failure_category(status: Option<i64>, reason: Option<&str>) -> &'static str {
    match status {
        Some(401 | 403) => "auth",
        Some(404) => "model_not_supported",
        Some(429) => "rate_limited",
        Some(408 | 500..=599) => "transient",
        _ if reason.is_some_and(|message| {
            let lower = message.to_ascii_lowercase();
            lower.contains("timeout")
                || lower.contains("timed out")
                || lower.contains("dns")
                || lower.contains("tls")
        }) =>
        {
            "transient"
        }
        _ => "other_upstream",
    }
}

fn trigger_label(trigger: &str) -> &str {
    match trigger {
        "scheduled_probe" | "half_open" | "manual_probe" => trigger,
        _ => "passive",
    }
}

pub(crate) fn record_observation_with_storage(
    storage: &codexmanager_core::storage::Storage,
    api_id: &str,
    model: Option<&str>,
    protocol: Option<&str>,
    trigger: &str,
    ok: bool,
    status: Option<i64>,
    latency_ms: Option<i64>,
    reason: Option<&str>,
) {
    let now = now_ts();
    let category = (!ok).then(|| failure_category(status, reason).to_string());
    let source_scoped = category.as_deref() == Some("auth");
    let state_model = if source_scoped {
        None
    } else {
        optional_scope(model)
    };
    let state_protocol = if source_scoped {
        None
    } else {
        optional_scope(protocol)
    };
    let existing = storage
        .aggregate_api_health_state(api_id, state_model.as_deref(), state_protocol.as_deref())
        .ok()
        .flatten();
    if ok
        && trigger == "passive"
        && existing
            .as_ref()
            .is_some_and(|state| state.state == "healthy" && state.last_error_category.is_none())
    {
        return;
    }
    let before = existing
        .as_ref()
        .map(|state| state.state.as_str())
        .unwrap_or("unknown")
        .to_string();
    let mut state = existing.unwrap_or(AggregateApiHealthState {
        aggregate_api_id: api_id.to_string(),
        upstream_model: state_model.clone(),
        protocol: state_protocol.clone(),
        state: "unknown".to_string(),
        consecutive_failures: 0,
        consecutive_successes: 0,
        failure_threshold: FAILURE_THRESHOLD,
        cooldown_until: None,
        half_open_at: None,
        last_observed_at: None,
        last_probe_at: None,
        last_success_at: None,
        last_failure_at: None,
        last_latency_ms: None,
        last_http_status: None,
        last_error_category: None,
        last_error_reason: None,
        last_observation_source: None,
        updated_at: now,
    });
    state.last_observed_at = Some(now);
    state.last_http_status = status;
    state.last_latency_ms = latency_ms;
    state.last_observation_source = Some(trigger_label(trigger).to_string());
    state.updated_at = now;
    if matches!(trigger, "scheduled_probe" | "manual_probe" | "half_open") {
        state.last_probe_at = Some(now);
    }
    if ok {
        state.consecutive_failures = 0;
        state.consecutive_successes = state.consecutive_successes.saturating_add(1);
        state.cooldown_until = None;
        state.half_open_at = None;
        state.last_success_at = Some(now);
        state.last_error_category = None;
        state.last_error_reason = None;
        state.state = if latency_ms.unwrap_or_default() > DEGRADED_LATENCY_MS {
            "degraded"
        } else {
            "healthy"
        }
        .to_string();
    } else {
        state.consecutive_successes = 0;
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        state.last_failure_at = Some(now);
        state.last_error_category = category.clone();
        state.last_error_reason = sanitize_reason(reason);
        let cooldown_secs = match category.as_deref() {
            Some("auth") => Some(AUTH_COOLDOWN_SECS),
            Some("model_not_supported") => Some(MODEL_UNSUPPORTED_COOLDOWN_SECS),
            Some("rate_limited") => Some(TRANSIENT_COOLDOWN_SECS),
            _ if state.consecutive_failures >= FAILURE_THRESHOLD
                && aggregate_api_consecutive_freeze_enabled_or(storage, api_id) =>
            {
                Some(TRANSIENT_COOLDOWN_SECS)
            }
            _ => None,
        };
        if let Some(seconds) = cooldown_secs {
            state.state = "cooldown".to_string();
            state.cooldown_until = Some(now + seconds);
            state.half_open_at = state.cooldown_until;
        } else {
            state.state = "degraded".to_string();
        }
    }
    let event = AggregateApiHealthEvent {
        aggregate_api_id: api_id.to_string(),
        upstream_model: state_model,
        protocol: state_protocol,
        trigger: trigger_label(trigger).to_string(),
        outcome: if ok { "success" } else { "failure" }.to_string(),
        state_before: before,
        state_after: state.state.clone(),
        error_category: category,
        http_status: status,
        latency_ms,
        reason: sanitize_reason(reason),
        observed_at: now,
        cooldown_until: state.cooldown_until,
    };
    if let Err(error) = storage.save_aggregate_api_health_observation(&state, &event) {
        log::warn!(
            "event=aggregate_api_health_persist_failed api_id={} error={}",
            api_id,
            error
        );
    }
}

pub(crate) fn record_observation(
    api_id: &str,
    model: Option<&str>,
    protocol: Option<&str>,
    trigger: &str,
    ok: bool,
    status: Option<i64>,
    latency_ms: Option<i64>,
    reason: Option<&str>,
) {
    let Some(storage) = open_storage() else {
        return;
    };
    record_observation_with_storage(
        &storage, api_id, model, protocol, trigger, ok, status, latency_ms, reason,
    );
}

fn health_state_blocks_routing(
    state: &AggregateApiHealthState,
    now: i64,
    consecutive_freeze_enabled: bool,
) -> bool {
    if !matches!(state.state.as_str(), "cooldown" | "unhealthy")
        || !state.cooldown_until.map_or(true, |until| until > now)
    {
        return false;
    }
    if state.state == "unhealthy" || consecutive_freeze_enabled {
        return true;
    }
    // The setting only disables the generic threshold-based cooldown. Keep
    // explicit auth, model-support, and rate-limit cooling behavior intact.
    matches!(
        state.last_error_category.as_deref(),
        Some("auth" | "model_not_supported" | "rate_limited")
    )
}

pub(crate) fn is_routing_blocked_with_storage(
    storage: &codexmanager_core::storage::Storage,
    api_id: &str,
    model: Option<&str>,
) -> bool {
    // Passive observations remain visible for every source, but a persisted
    // health cooldown may only remove routes when proactive monitoring was
    // explicitly enabled for that source. The legacy in-memory cooldown still
    // protects the current process from repeated failures.
    if !storage
        .aggregate_api_health_config(api_id)
        .map(|config| config.enabled)
        .unwrap_or(false)
    {
        return false;
    }
    let now = now_ts();
    let consecutive_freeze_enabled = aggregate_api_consecutive_freeze_enabled_or(storage, api_id);
    let source_blocked = storage
        .aggregate_api_health_state(api_id, None, None)
        .ok()
        .flatten()
        .is_some_and(|state| health_state_blocks_routing(&state, now, consecutive_freeze_enabled));
    if source_blocked {
        return true;
    }
    storage
        .aggregate_api_health_state(api_id, model, None)
        .ok()
        .flatten()
        .is_some_and(|state| health_state_blocks_routing(&state, now, consecutive_freeze_enabled))
}

pub(crate) fn list_health() -> Result<AggregateApiHealthListResult, String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let mut items = Vec::new();
    for api in storage
        .list_aggregate_apis()
        .map_err(|error| error.to_string())?
    {
        let config = storage
            .aggregate_api_health_config(&api.id)
            .map_err(|error| error.to_string())?;
        let available_models = available_probe_models(&storage, &api.id);
        let states = storage
            .list_aggregate_api_health_states(&api.id)
            .map_err(|error| error.to_string())?;
        let state = states
            .iter()
            .find(|state| state.upstream_model.is_none() && state.protocol.is_none())
            .cloned()
            .or_else(|| {
                states
                    .into_iter()
                    .max_by_key(|state| state.last_observed_at.unwrap_or_default())
            })
            .unwrap_or(AggregateApiHealthState {
                aggregate_api_id: api.id.clone(),
                upstream_model: None,
                protocol: None,
                state: "unknown".to_string(),
                consecutive_failures: 0,
                consecutive_successes: 0,
                failure_threshold: FAILURE_THRESHOLD,
                cooldown_until: None,
                half_open_at: None,
                last_observed_at: None,
                last_probe_at: None,
                last_success_at: None,
                last_failure_at: None,
                last_latency_ms: None,
                last_http_status: None,
                last_error_category: None,
                last_error_reason: None,
                last_observation_source: None,
                updated_at: 0,
            });
        let mut result = state_result(state);
        result.active_probe_enabled = config.enabled;
        result.probe_model = config.probe_model;
        result.available_probe_models = available_models;
        items.push(result);
    }
    Ok(AggregateApiHealthListResult { items })
}

pub(crate) fn get_health(
    api_id: &str,
    limit: i64,
) -> Result<AggregateApiHealthDetailResult, String> {
    if api_id.trim().is_empty() {
        return Err("aggregate api id required".to_string());
    }
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    if !storage
        .aggregate_api_exists(api_id)
        .map_err(|error| error.to_string())?
    {
        return Err("aggregate api not found".to_string());
    }
    let states = storage
        .list_aggregate_api_health_states(api_id)
        .map_err(|error| error.to_string())?;
    let summary = states
        .iter()
        .find(|state| state.upstream_model.is_none() && state.protocol.is_none())
        .cloned()
        .or_else(|| {
            states
                .iter()
                .cloned()
                .max_by_key(|state| state.last_observed_at.unwrap_or_default())
        })
        .unwrap_or(AggregateApiHealthState {
            aggregate_api_id: api_id.to_string(),
            upstream_model: None,
            protocol: None,
            state: "unknown".to_string(),
            consecutive_failures: 0,
            consecutive_successes: 0,
            failure_threshold: FAILURE_THRESHOLD,
            cooldown_until: None,
            half_open_at: None,
            last_observed_at: None,
            last_probe_at: None,
            last_success_at: None,
            last_failure_at: None,
            last_latency_ms: None,
            last_http_status: None,
            last_error_category: None,
            last_error_reason: None,
            last_observation_source: None,
            updated_at: 0,
        });
    let config = storage
        .aggregate_api_health_config(api_id)
        .map_err(|error| error.to_string())?;
    let mut summary = state_result(summary);
    summary.active_probe_enabled = config.enabled;
    summary.probe_model = config.probe_model.clone();
    summary.available_probe_models = available_probe_models(&storage, api_id);
    Ok(AggregateApiHealthDetailResult {
        summary,
        config: config_result(config),
        states: states.into_iter().map(state_result).collect(),
        events: storage
            .list_aggregate_api_health_events(api_id, limit)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(event_result)
            .collect(),
    })
}

pub(crate) fn update_health_config(
    api_id: &str,
    enabled: bool,
    interval_secs: Option<i64>,
    timeout_ms: Option<i64>,
    probe_model: Option<&str>,
) -> Result<AggregateApiHealthConfigResult, String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    if !storage
        .aggregate_api_exists(api_id)
        .map_err(|error| error.to_string())?
    {
        return Err("aggregate api not found".to_string());
    }
    let mut config = storage
        .aggregate_api_health_config(api_id)
        .map_err(|error| error.to_string())?;
    if let Some(value) = interval_secs {
        if !(60..=86_400).contains(&value) {
            return Err("probe interval must be between 60 and 86400 seconds".to_string());
        }
        config.probe_interval_secs = value;
    }
    if let Some(value) = timeout_ms {
        if !(1_000..=60_000).contains(&value) {
            return Err("probe timeout must be between 1000 and 60000 ms".to_string());
        }
        config.probe_timeout_ms = value;
    }
    config.enabled = enabled;
    config.probe_model = optional_scope(probe_model);
    config.next_probe_at = enabled.then_some(now_ts());
    config.updated_at = now_ts();
    storage
        .upsert_aggregate_api_health_config(&config)
        .map_err(|error| error.to_string())?;
    Ok(config_result(config))
}

fn probe_health_with_trigger(
    api_id: &str,
    model: Option<&str>,
    trigger: &str,
) -> Result<AggregateApiTestResult, String> {
    let result = crate::aggregate_api::test_aggregate_api_connection_with_model(api_id, model)?;
    if let Some(storage) = open_storage() {
        record_probe_cost(&storage, api_id, model, trigger, result.ok);
        record_observation_with_storage(
            &storage,
            api_id,
            model,
            None,
            trigger,
            result.ok,
            result.status_code,
            Some(result.latency_ms),
            result.message.as_deref(),
        );
    } else {
        record_observation(
            api_id,
            model,
            None,
            trigger,
            result.ok,
            result.status_code,
            Some(result.latency_ms),
            result.message.as_deref(),
        );
    }
    Ok(result)
}

const ESTIMATED_PROBE_INPUT_TOKENS: i64 = 100;
const ESTIMATED_PROBE_OUTPUT_TOKENS: i64 = 16;

fn record_probe_cost(
    storage: &codexmanager_core::storage::Storage,
    api_id: &str,
    model: Option<&str>,
    trigger: &str,
    ok: bool,
) {
    let Ok(model) = crate::aggregate_api::resolve_aggregate_probe_model(storage, api_id, model)
    else {
        return;
    };
    let Ok(api) = storage.find_aggregate_api_by_id(api_id) else {
        return;
    };
    let Some(api) = api else {
        return;
    };
    let pricing = storage
        .list_managed_models_v2(true)
        .ok()
        .and_then(|models| {
            models.into_iter().find_map(|managed| {
                let route_matches = managed.routes.iter().any(|route| {
                    route.enabled
                        && route.source_kind == "aggregate_api"
                        && route.source_id == api_id
                        && route.upstream_model.eq_ignore_ascii_case(model.as_str())
                });
                route_matches.then_some(managed)
            })
        });
    let (pricing_model, price_source, input_price, output_price, estimated_cost) = pricing
        .and_then(|managed| {
            if managed.price.price_status == "missing" {
                return None;
            }
            let tier = managed
                .price_tiers
                .iter()
                .filter(|tier| tier.min_input_tokens == 0)
                .max_by_key(|tier| tier.min_input_tokens)?;
            let multiplier = (api.cost_multiplier.max(0.0) * 1_000.0).round() as i64;
            let charge = codexmanager_core::storage::compute_charge_v2(
                ESTIMATED_PROBE_INPUT_TOKENS,
                0,
                0,
                ESTIMATED_PROBE_OUTPUT_TOKENS,
                tier,
                multiplier,
            )
            .ok()?;
            Some((
                Some(managed.slug),
                managed.price.price_source,
                Some(tier.input_microusd_per_1m),
                Some(tier.output_microusd_per_1m),
                Some(charge.charged_cost_microusd),
            ))
        })
        .unwrap_or((None, None, None, None, None));
    let _ = storage.insert_aggregate_api_probe_cost(&AggregateApiProbeCost {
        aggregate_api_id: api_id.to_string(),
        upstream_model: model,
        trigger: trigger.to_string(),
        outcome: if ok { "success" } else { "failure" }.to_string(),
        estimated_input_tokens: ESTIMATED_PROBE_INPUT_TOKENS,
        estimated_output_tokens: ESTIMATED_PROBE_OUTPUT_TOKENS,
        pricing_model,
        price_source,
        input_microusd_per_1m: input_price,
        output_microusd_per_1m: output_price,
        rate_multiplier_millis: Some((api.cost_multiplier.max(0.0) * 1_000.0).round() as i64),
        estimated_cost_microusd: estimated_cost,
        created_at: now_ts(),
    });
}

pub(crate) fn list_probe_costs(
    start_ts: i64,
    end_ts: i64,
) -> Result<AggregateApiProbeCostListResult, String> {
    if start_ts <= 0 || end_ts <= start_ts {
        return Err("endTs must be greater than a positive startTs".to_string());
    }
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let items = storage
        .summarize_aggregate_api_probe_costs_between(start_ts, end_ts)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|item| AggregateApiProbeCostSummaryResult {
            aggregate_api_id: item.aggregate_api_id,
            probe_count: item.probe_count,
            priced_probe_count: item.priced_probe_count,
            unknown_cost_probe_count: item.unknown_cost_probe_count,
            scheduled_probe_count: item.scheduled_probe_count,
            half_open_probe_count: item.half_open_probe_count,
            manual_probe_count: item.manual_probe_count,
            estimated_cost_usd: item.estimated_cost_microusd as f64 / 1_000_000.0,
        })
        .collect();
    Ok(AggregateApiProbeCostListResult {
        items,
        start_ts,
        end_ts,
    })
}

pub(crate) fn probe_health(
    api_id: &str,
    model: Option<&str>,
) -> Result<AggregateApiTestResult, String> {
    probe_health_with_trigger(api_id, model, "manual_probe")
}

pub(crate) fn reset_health(
    api_id: &str,
    model: Option<&str>,
    protocol: Option<&str>,
) -> Result<AggregateApiHealthStateResult, String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    storage
        .reset_aggregate_api_health_state(api_id, model, protocol)
        .map_err(|error| error.to_string())?;
    crate::gateway::gateway_reset_aggregate_api_runtime_status(api_id);
    Ok(AggregateApiHealthStateResult {
        aggregate_api_id: api_id.to_string(),
        upstream_model: optional_scope(model),
        protocol: optional_scope(protocol),
        state: "unknown".to_string(),
        failure_threshold: FAILURE_THRESHOLD,
        ..Default::default()
    })
}

static HEALTH_SCHEDULER_STARTED: OnceLock<()> = OnceLock::new();
static AUTOMATIC_PROBE_COUNTS: OnceLock<Mutex<HashMap<(String, i64), i64>>> = OnceLock::new();

fn reserve_automatic_probe(api_id: &str, now: i64) -> bool {
    let day = now.div_euclid(24 * 60 * 60);
    let counts = AUTOMATIC_PROBE_COUNTS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut counts = crate::lock_utils::lock_recover(counts, "aggregate_api_health_probe_counts");
    counts.retain(|(_, recorded_day), _| *recorded_day == day);
    let count = counts.entry((api_id.to_string(), day)).or_default();
    if *count >= MAX_AUTOMATIC_PROBES_PER_DAY {
        return false;
    }
    *count += 1;
    true
}

pub(crate) fn ensure_aggregate_api_health_polling() {
    if HEALTH_SCHEDULER_STARTED.set(()).is_err() {
        return;
    }
    thread::Builder::new()
        .name("aggregate-api-health".to_string())
        .spawn(|| loop {
            if let Some(storage) = open_storage() {
                let now = now_ts();
                if let Ok(configs) = storage.list_enabled_aggregate_api_health_configs(now) {
                    for config in configs.into_iter().take(2) {
                        if !reserve_automatic_probe(&config.aggregate_api_id, now) {
                            log::warn!(
                                "event=aggregate_api_health_probe_daily_cap api_id={}",
                                config.aggregate_api_id
                            );
                            let _ = storage.update_aggregate_api_health_schedule(
                                &config.aggregate_api_id,
                                now,
                                now + 24 * 60 * 60,
                            );
                            continue;
                        }
                        let model = config.probe_model.clone();
                        let trigger = storage
                            .aggregate_api_health_state(&config.aggregate_api_id, None, None)
                            .ok()
                            .flatten()
                            .is_some_and(|state| {
                                state.cooldown_until.is_some_and(|until| until <= now)
                            });
                        let _ = probe_health_with_trigger(
                            &config.aggregate_api_id,
                            model.as_deref(),
                            if trigger {
                                "half_open"
                            } else {
                                "scheduled_probe"
                            },
                        );
                        let degraded = storage
                            .aggregate_api_health_state(&config.aggregate_api_id, None, None)
                            .ok()
                            .flatten()
                            .is_some_and(|state| {
                                state.state == "degraded" || state.state == "recovering"
                            });
                        let cooldown_until = storage
                            .aggregate_api_health_state(&config.aggregate_api_id, None, None)
                            .ok()
                            .flatten()
                            .and_then(|state| {
                                (state.state == "cooldown").then_some(state.cooldown_until)
                            })
                            .flatten();
                        let interval = if degraded {
                            config.probe_interval_secs.min(300)
                        } else {
                            config.probe_interval_secs
                        };
                        let next = cooldown_until
                            .filter(|until| *until > now)
                            .unwrap_or_else(|| now + interval + (now % (interval / 10).max(1)));
                        let _ = storage.update_aggregate_api_health_schedule(
                            &config.aggregate_api_id,
                            now,
                            next,
                        );
                    }
                }
            }
            thread::sleep(Duration::from_secs(15));
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use codexmanager_core::storage::{AggregateApi, Storage};

    fn aggregate_api(id: &str) -> AggregateApi {
        AggregateApi {
            id: id.to_string(),
            provider_type: "codex".to_string(),
            supplier_name: None,
            sort: 0,
            url: "https://example.test/v1".to_string(),
            auth_type: "apikey".to_string(),
            auth_params_json: None,
            action: None,
            model_override: None,
            cost_multiplier: 1.0,
            daily_spend_limit_usd: None,
            status: "active".to_string(),
            created_at: 0,
            updated_at: 0,
            last_test_at: None,
            last_test_status: None,
            last_test_error: None,
            balance_query_enabled: false,
            balance_query_template: None,
            balance_query_base_url: None,
            balance_query_user_id: None,
            balance_query_config_json: None,
            last_balance_at: None,
            last_balance_status: None,
            last_balance_error: None,
            last_balance_json: None,
            enable_consecutive_failure_freeze: true,
            upstream_protocol: None,
        }
    }

    #[test]
    fn persisted_cooldown_only_blocks_when_proactive_monitoring_is_enabled() {
        let storage = Storage::open_in_memory().expect("open storage");
        storage.init().expect("initialize storage");
        let api_id = "aggregate-health-gate";
        let model = "gpt-5.6-terra";
        storage
            .insert_aggregate_api(&aggregate_api(api_id))
            .expect("insert aggregate api");

        let now = now_ts();
        let mut config = storage
            .aggregate_api_health_config(api_id)
            .expect("read health config");
        config.enabled = false;
        storage
            .upsert_aggregate_api_health_config(&config)
            .expect("save disabled health config");
        let state = AggregateApiHealthState {
            aggregate_api_id: api_id.to_string(),
            upstream_model: Some(model.to_string()),
            protocol: None,
            state: "cooldown".to_string(),
            consecutive_failures: FAILURE_THRESHOLD,
            consecutive_successes: 0,
            failure_threshold: FAILURE_THRESHOLD,
            cooldown_until: Some(now + 60),
            half_open_at: Some(now + 60),
            last_observed_at: Some(now),
            last_probe_at: Some(now),
            last_success_at: None,
            last_failure_at: Some(now),
            last_latency_ms: None,
            last_http_status: Some(503),
            last_error_category: Some("transient".to_string()),
            last_error_reason: Some("upstream unavailable".to_string()),
            last_observation_source: Some("passive".to_string()),
            updated_at: now,
        };
        let event = AggregateApiHealthEvent {
            aggregate_api_id: api_id.to_string(),
            upstream_model: Some(model.to_string()),
            protocol: None,
            trigger: "passive".to_string(),
            outcome: "failure".to_string(),
            state_before: "degraded".to_string(),
            state_after: "cooldown".to_string(),
            error_category: Some("transient".to_string()),
            http_status: Some(503),
            latency_ms: None,
            reason: Some("upstream unavailable".to_string()),
            observed_at: now,
            cooldown_until: Some(now + 60),
        };
        storage
            .save_aggregate_api_health_observation(&state, &event)
            .expect("save health state");

        assert!(!is_routing_blocked_with_storage(
            &storage,
            api_id,
            Some(model)
        ));

        config.enabled = true;
        storage
            .upsert_aggregate_api_health_config(&config)
            .expect("save enabled health config");
        assert!(is_routing_blocked_with_storage(
            &storage,
            api_id,
            Some(model)
        ));

        storage
            .update_aggregate_api_consecutive_freeze(api_id, false)
            .expect("disable consecutive freeze");
        assert!(!is_routing_blocked_with_storage(
            &storage,
            api_id,
            Some(model)
        ));

        // 分类冷却不受连续失败冻结开关影响。
        record_observation_with_storage(
            &storage,
            api_id,
            Some(model),
            None,
            "passive",
            false,
            Some(401),
            None,
            None,
        );
        assert!(is_routing_blocked_with_storage(
            &storage,
            api_id,
            Some(model)
        ));
    }

    #[test]
    fn consecutive_generic_failures_respect_freeze_switch() {
        let storage = Storage::open_in_memory().expect("open storage");
        storage.init().expect("initialize storage");
        let api_id = "aggregate-health-freeze-switch";
        let model = "gpt-5.6-terra";

        // 开关关闭：连续达到阈值后仍不进入 cooldown（状态保持 degraded）。
        storage
            .insert_aggregate_api(&AggregateApi {
                enable_consecutive_failure_freeze: false,
                ..aggregate_api(api_id)
            })
            .expect("insert aggregate api with freeze disabled");
        for _ in 0..=FAILURE_THRESHOLD {
            record_observation_with_storage(
                &storage,
                api_id,
                Some(model),
                None,
                "passive",
                false,
                Some(502),
                None,
                Some("upstream 502"),
            );
        }
        let state = storage
            .aggregate_api_health_state(api_id, Some(model), None)
            .expect("read health state")
            .expect("health state exists");
        assert_eq!(state.consecutive_failures, FAILURE_THRESHOLD + 1);
        assert_eq!(state.state, "degraded");
        assert!(state.cooldown_until.is_none());

        // 分类冷却不受开关影响：auth 失败仍进入 cooldown。
        record_observation_with_storage(
            &storage,
            api_id,
            Some(model),
            None,
            "passive",
            false,
            Some(401),
            None,
            None,
        );
        // auth 分类冷却写入全局 scope（不区分模型）。
        let state = storage
            .aggregate_api_health_state(api_id, None, None)
            .expect("read health state")
            .expect("health state exists");
        assert_eq!(state.state, "cooldown");
        assert!(state.cooldown_until.is_some());

        // 开关开启：generic 连续失败重新触发 cooldown。
        storage
            .insert_aggregate_api(&AggregateApi {
                enable_consecutive_failure_freeze: true,
                ..aggregate_api(api_id)
            })
            .expect("insert aggregate api with freeze enabled");
        storage
            .reset_aggregate_api_health_state(api_id, Some(model), None)
            .expect("reset health state");
        for _ in 0..=FAILURE_THRESHOLD {
            record_observation_with_storage(
                &storage,
                api_id,
                Some(model),
                None,
                "passive",
                false,
                Some(502),
                None,
                Some("upstream 502"),
            );
        }
        let state = storage
            .aggregate_api_health_state(api_id, Some(model), None)
            .expect("read health state")
            .expect("health state exists");
        assert_eq!(state.state, "cooldown");
        assert!(state.cooldown_until.is_some());
    }
}
