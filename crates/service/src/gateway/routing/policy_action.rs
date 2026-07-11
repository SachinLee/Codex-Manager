use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use codexmanager_core::rpc::types::{GatewayPolicyActionSummary, RouteEvidenceSummary};
use codexmanager_core::storage::now_ts;

const SYSTEM_OWNER: &str = "system";
const COOLDOWN_KIND: &str = "cooldown";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EvidenceKind {
    QuotaBalance,
    RateLimit,
    Transport,
    Capacity,
    CapabilityUnsupported,
    Cooldown,
    UpstreamStatus,
}

impl EvidenceKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::QuotaBalance => "quota_balance",
            Self::RateLimit => "rate_limit",
            Self::Transport => "transport",
            Self::Capacity => "capacity",
            Self::CapabilityUnsupported => "capability_unsupported",
            Self::Cooldown => "cooldown",
            Self::UpstreamStatus => "upstream_status",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PolicyTargetKind {
    Account,
    AggregateApi,
    Upstream,
}

impl PolicyTargetKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::AggregateApi => "aggregate_api",
            Self::Upstream => "upstream",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct RouteEvidenceInput {
    pub kind: EvidenceKind,
    pub source: &'static str,
    pub target_kind: PolicyTargetKind,
    pub target_id: Option<String>,
    pub confidence: &'static str,
    pub reason: String,
    pub status_code: Option<i64>,
    pub retry_after_secs: Option<i64>,
    pub observed_at: i64,
}

impl RouteEvidenceInput {
    pub(super) fn summary(self) -> RouteEvidenceSummary {
        RouteEvidenceSummary {
            kind: self.kind.as_str().to_string(),
            source: self.source.to_string(),
            target_kind: self.target_kind.as_str().to_string(),
            target_id: self.target_id,
            confidence: self.confidence.to_string(),
            reason: self.reason,
            status_code: self.status_code,
            retry_after_secs: self.retry_after_secs,
            observed_at: self.observed_at,
        }
    }
}

#[derive(Default)]
struct PolicyActionState {
    actions: HashMap<String, GatewayPolicyActionSummary>,
}

static POLICY_ACTIONS: OnceLock<Mutex<PolicyActionState>> = OnceLock::new();

fn action_key(target_kind: PolicyTargetKind, target_id: &str) -> String {
    format!("{}:{target_id}", target_kind.as_str())
}

fn make_action_id(target_kind: PolicyTargetKind, target_id: &str, created_at: i64) -> String {
    format!(
        "{}:{}:{created_at}",
        target_kind.as_str(),
        target_id.trim().replace(':', "_")
    )
}

fn cleanup_expired(state: &mut PolicyActionState, now: i64) {
    state
        .actions
        .retain(|_, action| action.expires_at > now && action.remaining_secs > 0);
}

pub(super) fn record_system_cooldown_action(
    target_kind: PolicyTargetKind,
    target_id: &str,
    reason: impl Into<String>,
    expires_at: i64,
    source_evidence: Vec<RouteEvidenceSummary>,
) {
    let target_id = target_id.trim();
    if target_id.is_empty() || expires_at <= 0 {
        return;
    }
    let now = now_ts();
    if expires_at <= now {
        return;
    }
    let lock = POLICY_ACTIONS.get_or_init(|| Mutex::new(PolicyActionState::default()));
    let mut state = crate::lock_utils::lock_recover(lock, "gateway_policy_actions");
    cleanup_expired(&mut state, now);
    let key = action_key(target_kind, target_id);
    let created_at = state
        .actions
        .get(&key)
        .map(|action| action.created_at)
        .unwrap_or(now);
    let action = GatewayPolicyActionSummary {
        id: make_action_id(target_kind, target_id, created_at),
        owner: SYSTEM_OWNER.to_string(),
        kind: COOLDOWN_KIND.to_string(),
        target_kind: target_kind.as_str().to_string(),
        target_id: target_id.to_string(),
        reason: reason.into(),
        created_at,
        expires_at,
        remaining_secs: expires_at.saturating_sub(now).max(0),
        source_evidence,
    };
    state.actions.insert(key, action);
}

pub(crate) fn active_policy_actions_for_target(
    target_kind: PolicyTargetKind,
    target_id: Option<&str>,
) -> Vec<GatewayPolicyActionSummary> {
    let Some(target_id) = target_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Vec::new();
    };
    let lock = POLICY_ACTIONS.get_or_init(|| Mutex::new(PolicyActionState::default()));
    let mut state = crate::lock_utils::lock_recover(lock, "gateway_policy_actions");
    let now = now_ts();
    cleanup_expired(&mut state, now);
    state
        .actions
        .get(&action_key(target_kind, target_id))
        .cloned()
        .map(|mut action| {
            action.remaining_secs = action.expires_at.saturating_sub(now).max(0);
            action
        })
        .into_iter()
        .collect()
}

pub(crate) fn project_request_log_route_evidence(
    status_code: Option<i64>,
    error: Option<&str>,
    actual_source_kind: Option<&str>,
    actual_source_id: Option<&str>,
    aggregate_api_id: Option<&str>,
    created_at: i64,
) -> Vec<RouteEvidenceSummary> {
    let mut out = Vec::new();
    let lower_error = error.unwrap_or_default().to_ascii_lowercase();
    let target_kind = if aggregate_api_id.is_some() {
        PolicyTargetKind::AggregateApi
    } else if actual_source_kind == Some("account") {
        PolicyTargetKind::Account
    } else {
        PolicyTargetKind::Upstream
    };
    let target_id = aggregate_api_id
        .or(actual_source_id)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let mut push = |kind: EvidenceKind, reason: &str, confidence: &'static str| {
        out.push(
            RouteEvidenceInput {
                kind,
                source: "request_log_projection",
                target_kind,
                target_id: target_id.clone(),
                confidence,
                reason: reason.to_string(),
                status_code,
                retry_after_secs: None,
                observed_at: created_at,
            }
            .summary(),
        );
    };

    match status_code {
        Some(429) => push(
            EvidenceKind::RateLimit,
            "upstream returned HTTP 429",
            "high",
        ),
        Some(402) => push(
            EvidenceKind::QuotaBalance,
            "upstream returned HTTP 402",
            "high",
        ),
        Some(400 | 404 | 405 | 501) if lower_error.contains("unsupported") => push(
            EvidenceKind::CapabilityUnsupported,
            "capability unsupported",
            "high",
        ),
        Some(500..=599) => push(
            EvidenceKind::Transport,
            "upstream returned HTTP 5xx",
            "medium",
        ),
        Some(code) if code >= 400 => push(
            EvidenceKind::UpstreamStatus,
            "upstream error status",
            "medium",
        ),
        _ => {}
    }

    if lower_error.contains("quota") || lower_error.contains("insufficient") {
        push(
            EvidenceKind::QuotaBalance,
            "error indicates quota or balance exhaustion",
            "medium",
        );
    }
    if lower_error.contains("rate limit") || lower_error.contains("too many requests") {
        push(
            EvidenceKind::RateLimit,
            "error indicates upstream rate limiting",
            "medium",
        );
    }
    if lower_error.contains("capacity") {
        push(
            EvidenceKind::Capacity,
            "error indicates upstream capacity saturation",
            "medium",
        );
    }
    if lower_error.contains("unsupported") || lower_error.contains("not supported") {
        push(
            EvidenceKind::CapabilityUnsupported,
            "error indicates unsupported capability",
            "medium",
        );
    }
    if lower_error.contains("cooldown") {
        push(
            EvidenceKind::Cooldown,
            "gateway cooldown mentioned in error",
            "medium",
        );
    }

    out
}

pub(super) fn clear_runtime_state() {
    let lock = POLICY_ACTIONS.get_or_init(|| Mutex::new(PolicyActionState::default()));
    let mut state = crate::lock_utils::lock_recover(lock, "gateway_policy_actions");
    state.actions.clear();
}

#[cfg(test)]
#[path = "tests/policy_action_tests.rs"]
mod tests;
