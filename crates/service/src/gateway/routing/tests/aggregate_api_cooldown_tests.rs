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
