use super::*;
use codexmanager_core::storage::now_ts;

#[test]
fn aggregate_api_cooldown_enters_after_five_failures() {
    let _guard = crate::test_env_guard();
    clear_aggregate_api_cooldown_for_tests();

    for idx in 0..4 {
        assert!(
            !record_aggregate_api_failure("agg-a", Some("gpt-5")),
            "failure {idx}"
        );
        assert!(!is_aggregate_api_in_cooldown("agg-a", Some("gpt-5")));
    }
    assert!(record_aggregate_api_failure("agg-a", Some("gpt-5")));
    assert!(is_aggregate_api_in_cooldown("agg-a", Some("gpt-5")));
    clear_aggregate_api_cooldown_for_tests();
}

#[test]
fn aggregate_api_cooldown_clears_after_success() {
    let _guard = crate::test_env_guard();
    clear_aggregate_api_cooldown_for_tests();

    for _ in 0..5 {
        record_aggregate_api_failure("agg-b", Some("gpt-5"));
    }
    assert!(is_aggregate_api_in_cooldown("agg-b", Some("gpt-5")));

    clear_aggregate_api_cooldown("agg-b", Some("gpt-5"));
    assert!(!is_aggregate_api_in_cooldown("agg-b", Some("gpt-5")));
    clear_aggregate_api_cooldown_for_tests();
}

#[test]
fn aggregate_api_cooldown_snapshot_and_reset_clear_policy_action() {
    let _guard = crate::test_env_guard();
    clear_aggregate_api_cooldown_for_tests();

    for _ in 0..5 {
        record_aggregate_api_failure("agg-runtime-status", Some("gpt-5"));
    }

    let status = list_aggregate_api_cooldown_statuses()
        .into_iter()
        .find(|item| item.aggregate_api_id == "agg-runtime-status")
        .expect("runtime status");
    assert!(status.is_cooling_down);
    assert_eq!(status.consecutive_failures, 5);
    assert_eq!(status.failure_threshold, 5);
    assert!(status.cooldown_until.expect("cooldown deadline") > now_ts());
    assert!(status.remaining_secs > 0);
    assert_eq!(status.upstream_model.as_deref(), Some("gpt-5"));
    assert_eq!(
        status.reason.as_deref(),
        Some("consecutive aggregate api failures for model gpt-5")
    );
    assert_eq!(
        crate::gateway::active_policy_actions_for_target(
            crate::gateway::PolicyTargetKind::AggregateApi,
            Some("agg-runtime-status"),
        )
        .len(),
        1
    );

    clear_aggregate_api_cooldowns("agg-runtime-status");

    assert!(list_aggregate_api_cooldown_statuses()
        .into_iter()
        .all(|item| item.aggregate_api_id != "agg-runtime-status"));
    assert!(crate::gateway::active_policy_actions_for_target(
        crate::gateway::PolicyTargetKind::AggregateApi,
        Some("agg-runtime-status"),
    )
    .is_empty());
    clear_aggregate_api_cooldown_for_tests();
}

#[test]
fn aggregate_api_cooldown_forgets_stale_failures() {
    let _guard = crate::test_env_guard();
    clear_aggregate_api_cooldown_for_tests();
    let lock = AGGREGATE_API_COOLDOWN_UNTIL
        .get_or_init(|| Mutex::new(AggregateApiCooldownState::default()));
    let now = now_ts();
    {
        let mut state = lock.lock().expect("cooldown state lock");
        state.entries.insert(
            AggregateApiCooldownKey::new("agg-c", Some("gpt-5")),
            AggregateApiCooldownEntry {
                consecutive_failures: 5,
                cooldown_until: now - 1,
                last_failure_at: now - AGGREGATE_API_FAILURE_FORGET_AFTER_SECS - 1,
            },
        );
        state.last_cleanup_at = now - AGGREGATE_API_CLEANUP_INTERVAL_SECS - 1;
    }

    assert!(!is_aggregate_api_in_cooldown("agg-c", Some("gpt-5")));
    let state = lock.lock().expect("cooldown state lock");
    assert!(!state
        .entries
        .contains_key(&AggregateApiCooldownKey::new("agg-c", Some("gpt-5"))));
    drop(state);
    clear_aggregate_api_cooldown_for_tests();
}

#[test]
fn aggregate_api_cooldown_isolated_by_upstream_model() {
    let _guard = crate::test_env_guard();
    clear_aggregate_api_cooldown_for_tests();

    for _ in 0..5 {
        record_aggregate_api_failure("agg-model-scope", Some("gpt-5"));
    }

    assert!(is_aggregate_api_in_cooldown(
        "agg-model-scope",
        Some("gpt-5")
    ));
    assert!(!is_aggregate_api_in_cooldown(
        "agg-model-scope",
        Some("claude-sonnet-4")
    ));
    let statuses = list_aggregate_api_cooldown_statuses();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].upstream_model.as_deref(), Some("gpt-5"));
    clear_aggregate_api_cooldown_for_tests();
}

#[test]
fn aggregate_api_cooldown_success_only_clears_matching_model() {
    let _guard = crate::test_env_guard();
    clear_aggregate_api_cooldown_for_tests();

    for model in ["gpt-5", "claude-sonnet-4"] {
        for _ in 0..5 {
            record_aggregate_api_failure("agg-model-success", Some(model));
        }
    }

    clear_aggregate_api_cooldown("agg-model-success", Some("gpt-5"));

    assert!(!is_aggregate_api_in_cooldown(
        "agg-model-success",
        Some("gpt-5")
    ));
    assert!(is_aggregate_api_in_cooldown(
        "agg-model-success",
        Some("claude-sonnet-4")
    ));
    clear_aggregate_api_cooldown_for_tests();
}
