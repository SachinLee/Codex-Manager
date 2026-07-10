use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use codexmanager_core::storage::now_ts;

use super::policy_action::{
    record_system_cooldown_action, EvidenceKind, PolicyTargetKind, RouteEvidenceInput,
};

const AGGREGATE_API_FAILURE_THRESHOLD: u32 = 5;
const AGGREGATE_API_COOLDOWN_SECS: i64 = 5 * 60;
const AGGREGATE_API_FAILURE_FORGET_AFTER_SECS: i64 = 30 * 60;
const AGGREGATE_API_CLEANUP_INTERVAL_SECS: i64 = 30;

#[derive(Default)]
struct AggregateApiCooldownState {
    entries: HashMap<String, AggregateApiCooldownEntry>,
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
    let stale_ids = state
        .entries
        .iter()
        .filter(|(_, entry)| {
            entry.cooldown_until <= now
                && now.saturating_sub(entry.last_failure_at)
                    > AGGREGATE_API_FAILURE_FORGET_AFTER_SECS
        })
        .map(|(api_id, _)| api_id.clone())
        .collect::<Vec<_>>();
    for api_id in stale_ids {
        state.entries.remove(&api_id);
    }
}

/// 返回 aggregate API 当前是否处于内存冷却中。
pub(super) fn is_aggregate_api_in_cooldown(api_id: &str) -> bool {
    let lock = AGGREGATE_API_COOLDOWN_UNTIL
        .get_or_init(|| Mutex::new(AggregateApiCooldownState::default()));
    let mut state = crate::lock_utils::lock_recover(lock, "aggregate_api_cooldown_until");
    let now = now_ts();
    maybe_cleanup_expired_entries(&mut state, now);

    match state.entries.get(api_id).copied() {
        Some(entry) if entry.cooldown_until > now => true,
        Some(entry)
            if now.saturating_sub(entry.last_failure_at)
                > AGGREGATE_API_FAILURE_FORGET_AFTER_SECS =>
        {
            state.entries.remove(api_id);
            false
        }
        Some(_) => false,
        None => false,
    }
}

/// 记录一次 aggregate API 失败。达到阈值后进入冷却。
pub(super) fn record_aggregate_api_failure(api_id: &str) -> bool {
    let lock = AGGREGATE_API_COOLDOWN_UNTIL
        .get_or_init(|| Mutex::new(AggregateApiCooldownState::default()));
    let mut state = crate::lock_utils::lock_recover(lock, "aggregate_api_cooldown_until");
    let now = now_ts();
    maybe_cleanup_expired_entries(&mut state, now);

    let entry = state.entries.entry(api_id.to_string()).or_default();
    if entry.last_failure_at != 0
        && now.saturating_sub(entry.last_failure_at) > AGGREGATE_API_FAILURE_FORGET_AFTER_SECS
    {
        entry.consecutive_failures = 0;
        entry.cooldown_until = 0;
    }
    entry.last_failure_at = now;
    entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);

    if entry.consecutive_failures >= AGGREGATE_API_FAILURE_THRESHOLD {
        let cooldown_until = now + AGGREGATE_API_COOLDOWN_SECS;
        let entered_cooldown = cooldown_until > entry.cooldown_until;
        entry.cooldown_until = cooldown_until;
        if entered_cooldown {
            let evidence = RouteEvidenceInput {
                kind: EvidenceKind::Transport,
                source: "aggregate_api_cooldown",
                target_kind: PolicyTargetKind::AggregateApi,
                target_id: Some(api_id.to_string()),
                confidence: "high",
                reason: "consecutive aggregate api failures".to_string(),
                status_code: None,
                retry_after_secs: None,
                observed_at: now,
            }
            .summary();
            record_system_cooldown_action(
                PolicyTargetKind::AggregateApi,
                api_id,
                "consecutive aggregate api failures",
                cooldown_until,
                vec![evidence],
            );
            log::info!(
                "event=aggregate_api_cooldown_mark api_id={} failures={} cooldown_secs={}",
                api_id,
                entry.consecutive_failures,
                AGGREGATE_API_COOLDOWN_SECS
            );
        }
        true
    } else {
        false
    }
}

/// 成功后清除 aggregate API 的失败计数和冷却状态。
pub(super) fn clear_aggregate_api_cooldown(api_id: &str) {
    let lock = AGGREGATE_API_COOLDOWN_UNTIL
        .get_or_init(|| Mutex::new(AggregateApiCooldownState::default()));
    let mut state = crate::lock_utils::lock_recover(lock, "aggregate_api_cooldown_until");
    state.entries.remove(api_id);
}

/// 清空所有 aggregate API 的运行时冷却状态。
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
