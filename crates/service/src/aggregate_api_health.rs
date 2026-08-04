use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use codexmanager_core::rpc::types::{
    AggregateApiHealthConfigResult, AggregateApiHealthDetailResult, AggregateApiHealthEventResult,
    AggregateApiHealthListResult, AggregateApiHealthStateResult, AggregateApiTestResult,
};
use codexmanager_core::storage::{
    now_ts, AggregateApiHealthConfig, AggregateApiHealthEvent, AggregateApiHealthState,
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
            _ if state.consecutive_failures >= FAILURE_THRESHOLD => Some(TRANSIENT_COOLDOWN_SECS),
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

pub(crate) fn is_routing_blocked(api_id: &str, model: Option<&str>) -> bool {
    let Some(storage) = open_storage() else {
        return false;
    };
    let now = now_ts();
    let source_blocked = storage
        .aggregate_api_health_state(api_id, None, None)
        .ok()
        .flatten()
        .is_some_and(|state| {
            matches!(state.state.as_str(), "cooldown" | "unhealthy")
                && state.cooldown_until.map_or(true, |until| until > now)
        });
    if source_blocked {
        return true;
    }
    storage
        .aggregate_api_health_state(api_id, model, None)
        .ok()
        .flatten()
        .is_some_and(|state| {
            matches!(state.state.as_str(), "cooldown" | "unhealthy")
                && state.cooldown_until.map_or(true, |until| until > now)
        })
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
    Ok(result)
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
