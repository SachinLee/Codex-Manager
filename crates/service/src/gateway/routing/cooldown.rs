use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use codexmanager_core::storage::now_ts;

use super::policy_action::{
    record_system_cooldown_action, EvidenceKind, PolicyTargetKind, RouteEvidenceInput,
};

const DEFAULT_ACCOUNT_COOLDOWN_SECS: i64 = 20;
const DEFAULT_ACCOUNT_COOLDOWN_NETWORK_SECS: i64 = DEFAULT_ACCOUNT_COOLDOWN_SECS;
const DEFAULT_ACCOUNT_COOLDOWN_429_SECS: i64 = 45;
const DEFAULT_ACCOUNT_COOLDOWN_5XX_SECS: i64 = 30;
const DEFAULT_ACCOUNT_COOLDOWN_4XX_SECS: i64 = DEFAULT_ACCOUNT_COOLDOWN_SECS;
const DEFAULT_ACCOUNT_COOLDOWN_CHALLENGE_SECS: i64 = 6;
const DEFAULT_ACCOUNT_COOLDOWN_ANTHROPIC_CHALLENGE_SECS: i64 = 60;
const ACCOUNT_RATE_LIMIT_COOLDOWN_LADDER_SECS: [i64; 4] =
    [DEFAULT_ACCOUNT_COOLDOWN_429_SECS, 300, 1800, 7200];
// 中文注释：offense 只用于“短时间内持续 429”场景；超过该时间视为新一轮，避免长期记仇导致误伤。
const ACCOUNT_RATE_LIMIT_OFFENSE_FORGET_AFTER_SECS: i64 = 30 * 60;

const ACCOUNT_COOLDOWN_CLEANUP_INTERVAL_SECS: i64 = 30;

// 绑定账号连续网络失败超过此阈值后，升级为标准冷却跳过，避免持续轰炸不通的账号
pub(super) const BOUND_ACCOUNT_NETWORK_CONSECUTIVE_GIVE_UP: u32 = 3;
// 超过此时间无新网络失败，则重置连续计数（认为网络已恢复）
const NETWORK_CONSECUTIVE_FORGET_AFTER_SECS: i64 = 5 * 60;

#[derive(Default)]
struct AccountCooldownState {
    entries: HashMap<String, i64>,
    offense_counts: HashMap<String, u32>,
    offense_last_at: HashMap<String, i64>,
    last_cleanup_at: i64,
    // Task 1: 记录每个账号最近一次冷却原因，供 bound 账号跳过决策使用
    last_reason: HashMap<String, CooldownReason>,
    // Task 6: 跨请求网络连续失败计数，防止持续中断时无效轰炸
    network_consecutive_failures: HashMap<String, u32>,
    network_last_failure_at: HashMap<String, i64>,
}

static ACCOUNT_COOLDOWN_UNTIL: OnceLock<Mutex<AccountCooldownState>> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CooldownReason {
    Default,
    Network,
    RateLimited,
    Upstream5xx,
    Upstream4xx,
    Challenge,
    AnthropicChallenge,
}

fn cooldown_reason_label(reason: CooldownReason) -> &'static str {
    match reason {
        CooldownReason::Default => "default",
        CooldownReason::Network => "network",
        CooldownReason::RateLimited => "rate_limited",
        CooldownReason::Upstream5xx => "upstream_5xx",
        CooldownReason::Upstream4xx => "upstream_4xx",
        CooldownReason::Challenge => "challenge",
        CooldownReason::AnthropicChallenge => "anthropic_challenge",
    }
}

fn evidence_kind_for_cooldown_reason(reason: CooldownReason) -> EvidenceKind {
    match reason {
        CooldownReason::RateLimited => EvidenceKind::RateLimit,
        CooldownReason::Network | CooldownReason::Upstream5xx => EvidenceKind::Transport,
        CooldownReason::Upstream4xx
        | CooldownReason::Challenge
        | CooldownReason::AnthropicChallenge => EvidenceKind::UpstreamStatus,
        CooldownReason::Default => EvidenceKind::Cooldown,
    }
}

/// 函数 `cooldown_secs_for_reason`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - reason: 参数 reason
///
/// # 返回
/// 返回函数执行结果
fn cooldown_secs_for_reason(reason: CooldownReason) -> i64 {
    match reason {
        CooldownReason::Default => DEFAULT_ACCOUNT_COOLDOWN_SECS,
        CooldownReason::Network => DEFAULT_ACCOUNT_COOLDOWN_NETWORK_SECS,
        CooldownReason::RateLimited => DEFAULT_ACCOUNT_COOLDOWN_429_SECS,
        CooldownReason::Upstream5xx => DEFAULT_ACCOUNT_COOLDOWN_5XX_SECS,
        CooldownReason::Upstream4xx => DEFAULT_ACCOUNT_COOLDOWN_4XX_SECS,
        CooldownReason::Challenge => DEFAULT_ACCOUNT_COOLDOWN_CHALLENGE_SECS,
        CooldownReason::AnthropicChallenge => DEFAULT_ACCOUNT_COOLDOWN_ANTHROPIC_CHALLENGE_SECS,
    }
}

/// 函数 `rate_limit_cooldown_secs_for_offense`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - offense_count: 参数 offense_count
///
/// # 返回
/// 返回函数执行结果
fn rate_limit_cooldown_secs_for_offense(offense_count: u32) -> i64 {
    let idx = offense_count
        .saturating_sub(1)
        .min((ACCOUNT_RATE_LIMIT_COOLDOWN_LADDER_SECS.len() - 1) as u32) as usize;
    ACCOUNT_RATE_LIMIT_COOLDOWN_LADDER_SECS[idx]
}

/// 函数 `cooldown_secs_for_mark`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - offense_counts: 参数 offense_counts
/// - offense_last_at: 参数 offense_last_at
/// - account_id: 参数 account_id
/// - reason: 参数 reason
/// - now: 参数 now
///
/// # 返回
/// 返回函数执行结果
fn cooldown_secs_for_mark(
    offense_counts: &mut HashMap<String, u32>,
    offense_last_at: &mut HashMap<String, i64>,
    account_id: &str,
    reason: CooldownReason,
    now: i64,
) -> i64 {
    match reason {
        CooldownReason::RateLimited => {
            if let Some(last) = offense_last_at.get(account_id).copied() {
                if now.saturating_sub(last) > ACCOUNT_RATE_LIMIT_OFFENSE_FORGET_AFTER_SECS {
                    offense_counts.remove(account_id);
                }
            }
            let offense_count = offense_counts
                .entry(account_id.to_string())
                .and_modify(|count| *count = count.saturating_add(1))
                .or_insert(1);
            offense_last_at.insert(account_id.to_string(), now);
            rate_limit_cooldown_secs_for_offense(*offense_count)
        }
        _ => cooldown_secs_for_reason(reason),
    }
}

/// 函数 `decay_offense_count_for_success`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - offense_counts: 参数 offense_counts
/// - offense_last_at: 参数 offense_last_at
/// - account_id: 参数 account_id
///
/// # 返回
/// 无
fn decay_offense_count_for_success(
    offense_counts: &mut HashMap<String, u32>,
    offense_last_at: &mut HashMap<String, i64>,
    account_id: &str,
) {
    let mut should_remove = false;
    if let Some(count) = offense_counts.get_mut(account_id) {
        if *count <= 1 {
            should_remove = true;
        } else {
            *count -= 1;
        }
    }
    if should_remove {
        offense_counts.remove(account_id);
        offense_last_at.remove(account_id);
    }
}

/// 函数 `cooldown_reason_for_status`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn cooldown_reason_for_status(status: u16) -> CooldownReason {
    match status {
        429 => CooldownReason::RateLimited,
        500..=599 => CooldownReason::Upstream5xx,
        401 | 403 => CooldownReason::Challenge,
        400..=499 => CooldownReason::Upstream4xx,
        _ => CooldownReason::Default,
    }
}

/// 返回账号最近一次被标记冷却时的原因，用于 bound 账号的跳过决策。
/// 若账号未曾被标记或冷却已过期清理，返回 None。
pub(super) fn account_last_cooldown_reason(account_id: &str) -> Option<CooldownReason> {
    let lock = ACCOUNT_COOLDOWN_UNTIL.get_or_init(|| Mutex::new(AccountCooldownState::default()));
    let state = crate::lock_utils::lock_recover(lock, "account_cooldown_until");
    state.last_reason.get(account_id).copied()
}

/// 累积网络连续失败计数，超过遗忘窗口则先重置再累积。
fn increment_network_consecutive_failure(
    consecutive: &mut HashMap<String, u32>,
    last_at: &mut HashMap<String, i64>,
    account_id: &str,
    now: i64,
) {
    if let Some(last) = last_at.get(account_id).copied() {
        if now.saturating_sub(last) > NETWORK_CONSECUTIVE_FORGET_AFTER_SECS {
            consecutive.remove(account_id);
        }
    }
    let count = consecutive
        .entry(account_id.to_string())
        .and_modify(|c| *c = c.saturating_add(1))
        .or_insert(1);
    let _ = count;
    last_at.insert(account_id.to_string(), now);
}

/// 返回账号当前跨请求的网络连续失败次数。
pub(super) fn network_consecutive_failure_count(account_id: &str) -> u32 {
    let lock = ACCOUNT_COOLDOWN_UNTIL.get_or_init(|| Mutex::new(AccountCooldownState::default()));
    let state = crate::lock_utils::lock_recover(lock, "account_cooldown_until");
    state
        .network_consecutive_failures
        .get(account_id)
        .copied()
        .unwrap_or(0)
}

/// 重试成功后重置网络连续失败计数，避免偶发抖动恢复后仍被误判为持续中断。
pub(super) fn reset_network_consecutive_failure(account_id: &str) {
    let lock = ACCOUNT_COOLDOWN_UNTIL.get_or_init(|| Mutex::new(AccountCooldownState::default()));
    let mut state = crate::lock_utils::lock_recover(lock, "account_cooldown_until");
    state.network_consecutive_failures.remove(account_id);
    state.network_last_failure_at.remove(account_id);
}

/// 函数 `is_account_in_cooldown`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn is_account_in_cooldown(account_id: &str) -> bool {
    let lock = ACCOUNT_COOLDOWN_UNTIL.get_or_init(|| Mutex::new(AccountCooldownState::default()));
    let mut state = crate::lock_utils::lock_recover(lock, "account_cooldown_until");
    let now = now_ts();
    match state.entries.get(account_id).copied() {
        Some(until) if until > now => true,
        Some(_) => {
            state.entries.remove(account_id);
            false
        }
        None => false,
    }
}

/// 函数 `mark_account_cooldown`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 无
pub(super) fn mark_account_cooldown(account_id: &str, reason: CooldownReason) {
    let lock = ACCOUNT_COOLDOWN_UNTIL.get_or_init(|| Mutex::new(AccountCooldownState::default()));
    let mut guard = crate::lock_utils::lock_recover(lock, "account_cooldown_until");
    let state = &mut *guard;
    super::record_gateway_cooldown_mark();
    let now = now_ts();
    maybe_cleanup_expired_cooldowns(state, now);
    let cooldown_until = now
        + cooldown_secs_for_mark(
            &mut state.offense_counts,
            &mut state.offense_last_at,
            account_id,
            reason,
            now,
        );
    // 中文注释：同账号短时间内可能触发不同失败类型；保留更晚的 until 可避免被较短冷却覆盖。
    // last_reason 仅在 entries 实际更新时同步，避免短冷却原因覆盖长冷却原因。
    let entry_updated = match state.entries.get_mut(account_id) {
        Some(until) => {
            if cooldown_until > *until {
                *until = cooldown_until;
                true
            } else {
                false
            }
        }
        None => {
            state.entries.insert(account_id.to_string(), cooldown_until);
            true
        }
    };
    if entry_updated {
        state.last_reason.insert(account_id.to_string(), reason);
        let evidence = RouteEvidenceInput {
            kind: evidence_kind_for_cooldown_reason(reason),
            source: "account_cooldown",
            target_kind: PolicyTargetKind::Account,
            target_id: Some(account_id.to_string()),
            confidence: "high",
            reason: cooldown_reason_label(reason).to_string(),
            status_code: None,
            retry_after_secs: None,
            observed_at: now,
        }
        .summary();
        record_system_cooldown_action(
            PolicyTargetKind::Account,
            account_id,
            cooldown_reason_label(reason),
            cooldown_until,
            vec![evidence],
        );
    }
    // Task 6: 网络失败时始终累积计数，无论冷却 entry 是否更新
    if reason == CooldownReason::Network {
        increment_network_consecutive_failure(
            &mut state.network_consecutive_failures,
            &mut state.network_last_failure_at,
            account_id,
            now,
        );
    }
}

/// 函数 `mark_account_cooldown_for_status`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 无
pub(super) fn mark_account_cooldown_for_status(account_id: &str, status: u16) {
    mark_account_cooldown(account_id, cooldown_reason_for_status(status));
}

/// 函数 `clear_account_cooldown`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 无
pub(super) fn clear_account_cooldown(account_id: &str) {
    let lock = ACCOUNT_COOLDOWN_UNTIL.get_or_init(|| Mutex::new(AccountCooldownState::default()));
    let mut guard = crate::lock_utils::lock_recover(lock, "account_cooldown_until");
    let state = &mut *guard;
    state.entries.remove(account_id);
    state.last_reason.remove(account_id);
    decay_offense_count_for_success(
        &mut state.offense_counts,
        &mut state.offense_last_at,
        account_id,
    );
}

/// 函数 `maybe_cleanup_expired_cooldowns`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - state: 参数 state
/// - now: 参数 now
///
/// # 返回
/// 无
fn maybe_cleanup_expired_cooldowns(state: &mut AccountCooldownState, now: i64) {
    if state.last_cleanup_at != 0
        && now.saturating_sub(state.last_cleanup_at) < ACCOUNT_COOLDOWN_CLEANUP_INTERVAL_SECS
    {
        return;
    }
    state.last_cleanup_at = now;
    let expired_accounts: Vec<String> = state
        .entries
        .iter()
        .filter(|(_, until)| **until <= now)
        .map(|(id, _)| id.clone())
        .collect();
    for account_id in &expired_accounts {
        state.entries.remove(account_id);
        state.last_reason.remove(account_id);
    }
    let mut stale_offenses = Vec::new();
    for (account_id, last) in state.offense_last_at.iter() {
        if now.saturating_sub(*last) > ACCOUNT_RATE_LIMIT_OFFENSE_FORGET_AFTER_SECS {
            stale_offenses.push(account_id.clone());
        }
    }
    for account_id in stale_offenses {
        state.offense_last_at.remove(&account_id);
        state.offense_counts.remove(&account_id);
    }
    // Task 6: 清理已超过遗忘窗口的网络连续失败记录
    let mut stale_network = Vec::new();
    for (account_id, last) in state.network_last_failure_at.iter() {
        if now.saturating_sub(*last) > NETWORK_CONSECUTIVE_FORGET_AFTER_SECS {
            stale_network.push(account_id.clone());
        }
    }
    for account_id in stale_network {
        state.network_last_failure_at.remove(&account_id);
        state.network_consecutive_failures.remove(&account_id);
    }
}

/// 函数 `clear_runtime_state`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 无
pub(super) fn clear_runtime_state() {
    let lock = ACCOUNT_COOLDOWN_UNTIL.get_or_init(|| Mutex::new(AccountCooldownState::default()));
    let mut state = crate::lock_utils::lock_recover(lock, "account_cooldown_until");
    state.entries.clear();
    state.offense_counts.clear();
    state.offense_last_at.clear();
    state.last_cleanup_at = 0;
    state.last_reason.clear();
    state.network_consecutive_failures.clear();
    state.network_last_failure_at.clear();
    super::policy_action::clear_runtime_state();
}

/// 函数 `clear_account_cooldown_for_tests`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[cfg(test)]
fn clear_account_cooldown_for_tests() {
    clear_runtime_state();
}

#[cfg(test)]
#[path = "tests/cooldown_tests.rs"]
mod tests;
