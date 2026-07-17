use super::*;
use codexmanager_core::storage::now_ts;

#[test]
fn aggregate_api_cooldown_enters_after_five_failures() {
    let _guard = crate::test_env_guard();
    clear_aggregate_api_cooldown_for_tests();

    for idx in 0..4 {
        assert!(!record_aggregate_api_failure("agg-a"), "failure {idx}");
        assert!(!is_aggregate_api_in_cooldown("agg-a"));
    }
    assert!(record_aggregate_api_failure("agg-a"));
    assert!(is_aggregate_api_in_cooldown("agg-a"));
    clear_aggregate_api_cooldown_for_tests();
}

#[test]
fn aggregate_api_cooldown_clears_after_success() {
    let _guard = crate::test_env_guard();
    clear_aggregate_api_cooldown_for_tests();

    for _ in 0..5 {
        record_aggregate_api_failure("agg-b");
    }
    assert!(is_aggregate_api_in_cooldown("agg-b"));

    clear_aggregate_api_cooldown("agg-b");
    assert!(!is_aggregate_api_in_cooldown("agg-b"));
    clear_aggregate_api_cooldown_for_tests();
}

#[test]
fn aggregate_api_cooldown_snapshot_and_reset_clear_policy_action() {
    let _guard = crate::test_env_guard();
    clear_aggregate_api_cooldown_for_tests();

    for _ in 0..5 {
        record_aggregate_api_failure("agg-runtime-status");
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
    assert_eq!(
        status.reason.as_deref(),
        Some("consecutive aggregate api failures")
    );
    assert_eq!(
        crate::gateway::active_policy_actions_for_target(
            crate::gateway::PolicyTargetKind::AggregateApi,
            Some("agg-runtime-status"),
        )
        .len(),
        1
    );

    clear_aggregate_api_cooldown("agg-runtime-status");

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
            "agg-c".to_string(),
            AggregateApiCooldownEntry {
                consecutive_failures: 5,
                cooldown_until: now - 1,
                last_failure_at: now - AGGREGATE_API_FAILURE_FORGET_AFTER_SECS - 1,
            },
        );
        state.last_cleanup_at = now - AGGREGATE_API_CLEANUP_INTERVAL_SECS - 1;
    }

    assert!(!is_aggregate_api_in_cooldown("agg-c"));
    let state = lock.lock().expect("cooldown state lock");
    assert!(!state.entries.contains_key("agg-c"));
    drop(state);
    clear_aggregate_api_cooldown_for_tests();
}
