use super::{
    active_policy_actions_for_target, clear_runtime_state, record_system_cooldown_action,
    EvidenceKind, PolicyTargetKind, RouteEvidenceInput,
};
use codexmanager_core::storage::now_ts;

#[test]
fn system_cooldown_action_expires_from_read_model() {
    clear_runtime_state();
    let now = now_ts();
    record_system_cooldown_action(
        PolicyTargetKind::Account,
        "acc-policy",
        "rate_limited",
        now + 30,
        vec![RouteEvidenceInput {
            kind: EvidenceKind::RateLimit,
            source: "test",
            target_kind: PolicyTargetKind::Account,
            target_id: Some("acc-policy".to_string()),
            confidence: "high",
            reason: "429".to_string(),
            status_code: Some(429),
            retry_after_secs: None,
            observed_at: now,
        }
        .summary()],
    );

    let active =
        active_policy_actions_for_target(PolicyTargetKind::Account, Some("acc-policy"));
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].owner, "system");
    assert_eq!(active[0].kind, "cooldown");
    assert!(active[0].remaining_secs > 0);
    assert_eq!(active[0].source_evidence[0].kind, "rate_limit");

    record_system_cooldown_action(
        PolicyTargetKind::Account,
        "acc-expired",
        "expired",
        now - 1,
        Vec::new(),
    );
    assert!(
        active_policy_actions_for_target(PolicyTargetKind::Account, Some("acc-expired"))
            .is_empty()
    );
    clear_runtime_state();
}
