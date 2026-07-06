use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const DEFAULT_SCOPE: &str = "unknown";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReasoningGuardDecision {
    Block {
        consecutive: usize,
        threshold: usize,
    },
    InternalRetry {
        consecutive: usize,
        threshold: usize,
    },
    ObserveOnly {
        consecutive: usize,
        threshold: usize,
    },
    BypassDisabled,
    BypassAfterConsecutive {
        consecutive: usize,
        threshold: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReasoningGuardResponseMode {
    Stream,
    NonStream,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct ReasoningGuardScope {
    source_id: String,
    model: String,
    path: String,
}

impl ReasoningGuardScope {
    pub(crate) fn new(source_id: Option<&str>, model: Option<&str>, path: &str) -> Self {
        Self {
            source_id: normalize_scope_part(source_id),
            model: normalize_scope_part(model),
            path: normalize_path(path),
        }
    }
}

static REASONING_GUARD_CONSECUTIVE: OnceLock<Mutex<HashMap<ReasoningGuardScope, usize>>> =
    OnceLock::new();

fn normalize_scope_part(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_SCOPE)
        .to_ascii_lowercase()
}

fn normalize_path(path: &str) -> String {
    path.split('?')
        .next()
        .unwrap_or(path)
        .trim()
        .to_ascii_lowercase()
}

fn state() -> &'static Mutex<HashMap<ReasoningGuardScope, usize>> {
    REASONING_GUARD_CONSECUTIVE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn decide(
    scope: &ReasoningGuardScope,
    mode: ReasoningGuardResponseMode,
    retry_budget_remaining: usize,
) -> ReasoningGuardDecision {
    if !crate::gateway::reasoning_guard_enabled() {
        reset(scope);
        return ReasoningGuardDecision::BypassDisabled;
    }

    let threshold = crate::gateway::reasoning_guard_bypass_after_consecutive();
    let mut guard = crate::lock_utils::lock_recover(state(), "reasoning_guard_consecutive");
    let consecutive = guard
        .entry(scope.clone())
        .and_modify(|value| *value = value.saturating_add(1))
        .or_insert(1);
    let count = *consecutive;
    if threshold > 0 && count >= threshold {
        guard.remove(scope);
        return ReasoningGuardDecision::BypassAfterConsecutive {
            consecutive: count,
            threshold,
        };
    }
    let should_intercept = match mode {
        ReasoningGuardResponseMode::Stream => crate::gateway::reasoning_guard_intercept_streaming(),
        ReasoningGuardResponseMode::NonStream => {
            crate::gateway::reasoning_guard_intercept_non_streaming()
        }
    };
    if !should_intercept {
        return ReasoningGuardDecision::ObserveOnly {
            consecutive: count,
            threshold,
        };
    }
    if retry_budget_remaining > 0 {
        return ReasoningGuardDecision::InternalRetry {
            consecutive: count,
            threshold,
        };
    }
    ReasoningGuardDecision::Block {
        consecutive: count,
        threshold,
    }
}

pub(crate) fn reset(scope: &ReasoningGuardScope) {
    let mut guard = crate::lock_utils::lock_recover(state(), "reasoning_guard_consecutive");
    guard.remove(scope);
}

pub(super) fn clear_runtime_state() {
    let mut guard = crate::lock_utils::lock_recover(state(), "reasoning_guard_consecutive");
    guard.clear();
}

#[cfg(test)]
mod tests {
    use super::{
        decide, reset, ReasoningGuardDecision, ReasoningGuardResponseMode, ReasoningGuardScope,
    };

    fn reset_reasoning_guard_test_config() {
        crate::gateway::reload_runtime_config_from_env();
        crate::gateway::set_reasoning_guard_enabled(true);
        crate::gateway::set_reasoning_guard_intercept_streaming(true);
        crate::gateway::set_reasoning_guard_intercept_non_streaming(true);
        crate::gateway::set_reasoning_guard_bypass_after_consecutive(0);
    }

    #[test]
    fn bypasses_only_on_configured_consecutive_threshold() {
        let _guard = crate::test_env_guard();
        reset_reasoning_guard_test_config();
        crate::gateway::set_reasoning_guard_bypass_after_consecutive(3);
        let scope = ReasoningGuardScope::new(Some("agg-a"), Some("gpt-5.5"), "/v1/responses");
        reset(&scope);

        assert_eq!(
            decide(&scope, ReasoningGuardResponseMode::NonStream, 0),
            ReasoningGuardDecision::Block {
                consecutive: 1,
                threshold: 3
            }
        );
        assert_eq!(
            decide(&scope, ReasoningGuardResponseMode::NonStream, 0),
            ReasoningGuardDecision::Block {
                consecutive: 2,
                threshold: 3
            }
        );
        assert_eq!(
            decide(&scope, ReasoningGuardResponseMode::NonStream, 0),
            ReasoningGuardDecision::BypassAfterConsecutive {
                consecutive: 3,
                threshold: 3
            }
        );
        assert_eq!(
            decide(&scope, ReasoningGuardResponseMode::NonStream, 0),
            ReasoningGuardDecision::Block {
                consecutive: 1,
                threshold: 3
            }
        );
    }

    #[test]
    fn returns_internal_retry_when_budget_remains() {
        let _guard = crate::test_env_guard();
        reset_reasoning_guard_test_config();
        let scope = ReasoningGuardScope::new(Some("agg-a"), Some("gpt-5.5"), "/v1/responses");
        reset(&scope);

        assert_eq!(
            decide(&scope, ReasoningGuardResponseMode::NonStream, 2),
            ReasoningGuardDecision::InternalRetry {
                consecutive: 1,
                threshold: 0
            }
        );
    }

    #[test]
    fn observes_only_when_mode_intercept_disabled() {
        let _guard = crate::test_env_guard();
        reset_reasoning_guard_test_config();
        crate::gateway::set_reasoning_guard_intercept_non_streaming(false);
        let scope = ReasoningGuardScope::new(Some("agg-a"), Some("gpt-5.5"), "/v1/responses");
        reset(&scope);

        assert_eq!(
            decide(&scope, ReasoningGuardResponseMode::NonStream, 2),
            ReasoningGuardDecision::ObserveOnly {
                consecutive: 1,
                threshold: 0
            }
        );
    }
}
