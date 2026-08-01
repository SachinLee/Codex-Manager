use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use codexmanager_core::{rpc::types::AggregateApiRuntimeStatus, storage::now_ts};

use super::policy_action::{
    clear_system_policy_action, record_system_cooldown_action, EvidenceKind, PolicyTargetKind,
    RouteEvidenceInput,
};

const AGGREGATE_API_FAILURE_THRESHOLD: u32 = 5;
const AGGREGATE_API_COOLDOWN_SECS: i64 = 5 * 60;
const AGGREGATE_API_FAILURE_FORGET_AFTER_SECS: i64 = 30 * 60;
const AGGREGATE_API_CLEANUP_INTERVAL_SECS: i64 = 30;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AggregateApiCooldownKey {
    api_id: String,
    upstream_model: Option<String>,
}

impl AggregateApiCooldownKey {
    fn new(api_id: &str, upstream_model: Option<&str>) -> Self {
        Self {
            api_id: api_id.trim().to_string(),
            upstream_model: upstream_model
                .map(str::trim)
                .filter(|model| !model.is_empty())
                .map(str::to_string),
        }
    }

    fn model_label(&self) -> &str {
        self.upstream_model.as_deref().unwrap_or("unspecified")
    }
}

#[derive(Default)]
struct AggregateApiCooldownState {
    entries: HashMap<AggregateApiCooldownKey, AggregateApiCooldownEntry>,
    last_cleanup_at: i64,
}

#[derive(Clone, Copy, Default)]
struct AggregateApiCooldownEntry {
    consecutive_failures: u32,
    cooldown_until: i64,
    last_failure_at: i64,
}

static AGGREGATE_API_COOLDOWN_UNTIL: OnceLock<Mutex<AggregateApiCooldownState>> = OnceLock::new();

fn maybe_cleanup_expired_entries(state: &mut AggregateApiCooldownState, now: i64) {
    if state.last_cleanup_at != 0
        && now.saturating_sub(state.last_cleanup_at) < AGGREGATE_API_CLEANUP_INTERVAL_SECS
    {
        return;
    }
    state.last_cleanup_at = now;
    let stale_keys = state
        .entries
        .iter()
        .filter(|(_, entry)| {
            entry.cooldown_until <= now
                && now.saturating_sub(entry.last_failure_at)
                    > AGGREGATE_API_FAILURE_FORGET_AFTER_SECS
        })
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    for key in stale_keys {
        state.entries.remove(&key);
    }
}

fn sync_policy_action(api_id: &str, state: &AggregateApiCooldownState, now: i64) {
    let cooled_models = state
        .entries
        .iter()
        .filter(|(key, entry)| key.api_id == api_id && entry.cooldown_until > now)
        .map(|(key, entry)| (key.model_label().to_string(), entry.cooldown_until))
        .collect::<Vec<_>>();
    let Some(cooldown_until) = cooled_models.iter().map(|(_, until)| *until).max() else {
        clear_system_policy_action(PolicyTargetKind::AggregateApi, api_id);
        return;
    };
    let models = cooled_models
        .iter()
        .map(|(model, _)| model.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let reason = format!("consecutive aggregate api failures for model(s): {models}");
    let evidence = RouteEvidenceInput {
        kind: EvidenceKind::Transport,
        source: "aggregate_api_cooldown",
        target_kind: PolicyTargetKind::AggregateApi,
        target_id: Some(api_id.to_string()),
        confidence: "high",
        reason: reason.clone(),
        status_code: None,
        retry_after_secs: None,
        observed_at: now,
    }
    .summary();
    record_system_cooldown_action(
        PolicyTargetKind::AggregateApi,
        api_id,
        reason,
        cooldown_until,
        vec![evidence],
    );
}

/// 返回聚合 API 的指定有效上游模型是否处于内存冷却中。
pub(super) fn is_aggregate_api_in_cooldown(api_id: &str, upstream_model: Option<&str>) -> bool {
    let lock = AGGREGATE_API_COOLDOWN_UNTIL
        .get_or_init(|| Mutex::new(AggregateApiCooldownState::default()));
    let mut state = crate::lock_utils::lock_recover(lock, "aggregate_api_cooldown_until");
    let now = now_ts();
    maybe_cleanup_expired_entries(&mut state, now);
    let key = AggregateApiCooldownKey::new(api_id, upstream_model);

    match state.entries.get(&key).copied() {
        Some(entry) if entry.cooldown_until > now => true,
        Some(entry)
            if now.saturating_sub(entry.last_failure_at)
                > AGGREGATE_API_FAILURE_FORGET_AFTER_SECS =>
        {
            state.entries.remove(&key);
            false
        }
        Some(_) | None => false,
    }
}

fn runtime_status_from_entry(
    key: &AggregateApiCooldownKey,
    entry: AggregateApiCooldownEntry,
    now: i64,
) -> AggregateApiRuntimeStatus {
    let is_cooling_down = entry.cooldown_until > now;
    AggregateApiRuntimeStatus {
        aggregate_api_id: key.api_id.clone(),
        upstream_model: key.upstream_model.clone(),
        is_cooling_down,
        consecutive_failures: entry.consecutive_failures,
        failure_threshold: AGGREGATE_API_FAILURE_THRESHOLD,
        cooldown_until: (entry.cooldown_until > 0).then_some(entry.cooldown_until),
        remaining_secs: entry.cooldown_until.saturating_sub(now).max(0),
        last_failure_at: (entry.last_failure_at > 0).then_some(entry.last_failure_at),
        reason: is_cooling_down.then(|| {
            format!(
                "consecutive aggregate api failures for model {}",
                key.model_label()
            )
        }),
    }
}

pub(super) fn list_aggregate_api_cooldown_statuses() -> Vec<AggregateApiRuntimeStatus> {
    let lock = AGGREGATE_API_COOLDOWN_UNTIL
        .get_or_init(|| Mutex::new(AggregateApiCooldownState::default()));
    let mut state = crate::lock_utils::lock_recover(lock, "aggregate_api_cooldown_until");
    let now = now_ts();
    maybe_cleanup_expired_entries(&mut state, now);

    let mut statuses = state
        .entries
        .iter()
        .map(|(key, entry)| runtime_status_from_entry(key, *entry, now))
        .collect::<Vec<_>>();
    statuses.sort_by(|left, right| {
        left.aggregate_api_id
            .cmp(&right.aggregate_api_id)
            .then_with(|| left.upstream_model.cmp(&right.upstream_model))
    });
    statuses
}

pub(super) fn aggregate_api_cooldown_status(api_id: &str) -> AggregateApiRuntimeStatus {
    let lock = AGGREGATE_API_COOLDOWN_UNTIL
        .get_or_init(|| Mutex::new(AggregateApiCooldownState::default()));
    let mut state = crate::lock_utils::lock_recover(lock, "aggregate_api_cooldown_until");
    let now = now_ts();
    maybe_cleanup_expired_entries(&mut state, now);
    let key = AggregateApiCooldownKey::new(api_id, None);
    state
        .entries
        .get(&key)
        .copied()
        .map(|entry| runtime_status_from_entry(&key, entry, now))
        .unwrap_or_else(|| AggregateApiRuntimeStatus {
            aggregate_api_id: api_id.to_string(),
            failure_threshold: AGGREGATE_API_FAILURE_THRESHOLD,
            ..AggregateApiRuntimeStatus::default()
        })
}

/// 记录一次聚合 API 指定模型的失败。达到阈值后仅冷却该模型。
pub(super) fn record_aggregate_api_failure(api_id: &str, upstream_model: Option<&str>) -> bool {
    let lock = AGGREGATE_API_COOLDOWN_UNTIL
        .get_or_init(|| Mutex::new(AggregateApiCooldownState::default()));
    let mut state = crate::lock_utils::lock_recover(lock, "aggregate_api_cooldown_until");
    let now = now_ts();
    maybe_cleanup_expired_entries(&mut state, now);
    let key = AggregateApiCooldownKey::new(api_id, upstream_model);

    let (is_cooling_down, entered_cooldown, failures) = {
        let entry = state.entries.entry(key.clone()).or_default();
        if entry.last_failure_at != 0
            && now.saturating_sub(entry.last_failure_at) > AGGREGATE_API_FAILURE_FORGET_AFTER_SECS
        {
            entry.consecutive_failures = 0;
            entry.cooldown_until = 0;
        }
        entry.last_failure_at = now;
        entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
        let is_cooling_down = entry.consecutive_failures >= AGGREGATE_API_FAILURE_THRESHOLD;
        let entered_cooldown = if is_cooling_down {
            let cooldown_until = now + AGGREGATE_API_COOLDOWN_SECS;
            let entered = cooldown_until > entry.cooldown_until;
            entry.cooldown_until = cooldown_until;
            entered
        } else {
            false
        };
        (
            is_cooling_down,
            entered_cooldown,
            entry.consecutive_failures,
        )
    };

    if entered_cooldown {
        sync_policy_action(api_id, &state, now);
        log::info!(
            "event=aggregate_api_cooldown_mark api_id={} upstream_model={} failures={} cooldown_secs={}",
            api_id,
            key.model_label(),
            failures,
            AGGREGATE_API_COOLDOWN_SECS
        );
    }
    is_cooling_down
}

/// 成功后清除聚合 API 指定模型的失败计数和冷却状态。
pub(super) fn clear_aggregate_api_cooldown(api_id: &str, upstream_model: Option<&str>) {
    let lock = AGGREGATE_API_COOLDOWN_UNTIL
        .get_or_init(|| Mutex::new(AggregateApiCooldownState::default()));
    let mut state = crate::lock_utils::lock_recover(lock, "aggregate_api_cooldown_until");
    state
        .entries
        .remove(&AggregateApiCooldownKey::new(api_id, upstream_model));
    sync_policy_action(api_id, &state, now_ts());
}

/// 清除一个聚合 API 的全部模型冷却状态，供人工重置使用。
pub(super) fn clear_aggregate_api_cooldowns(api_id: &str) {
    let lock = AGGREGATE_API_COOLDOWN_UNTIL
        .get_or_init(|| Mutex::new(AggregateApiCooldownState::default()));
    {
        let mut state = crate::lock_utils::lock_recover(lock, "aggregate_api_cooldown_until");
        state.entries.retain(|key, _| key.api_id != api_id.trim());
    }
    clear_system_policy_action(PolicyTargetKind::AggregateApi, api_id);
}

/// 清空所有聚合 API 的运行时冷却状态。
pub(super) fn clear_runtime_state() {
    let lock = AGGREGATE_API_COOLDOWN_UNTIL
        .get_or_init(|| Mutex::new(AggregateApiCooldownState::default()));
    let mut state = crate::lock_utils::lock_recover(lock, "aggregate_api_cooldown_until");
    state.entries.clear();
    state.last_cleanup_at = 0;
    super::policy_action::clear_runtime_state();
}

#[cfg(test)]
fn clear_aggregate_api_cooldown_for_tests() {
    clear_runtime_state();
}

#[cfg(test)]
#[path = "tests/aggregate_api_cooldown_tests.rs"]
mod tests;
