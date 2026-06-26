use super::{derive_outgoing_session_affinity, has_thread_anchor_conflict};

/// 函数 `uses_conversation_anchor_when_prompt_cache_missing`
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
#[test]
fn uses_conversation_anchor_when_prompt_cache_missing() {
    let actual = derive_outgoing_session_affinity(
        Some("legacy_session_should_not_win"),
        Some("legacy_request_id_should_not_win"),
        Some("legacy_turn_state_should_not_win"),
        Some("conv_anchor_only"),
        None,
    );

    assert_eq!(actual.incoming_session_id, Some("conv_anchor_only"));
    assert_eq!(actual.incoming_client_request_id, Some("conv_anchor_only"));
    assert_eq!(
        actual.incoming_turn_state,
        Some("legacy_turn_state_should_not_win")
    );
    assert_eq!(actual.fallback_session_id, Some("conv_anchor_only"));
}

/// 函数 `uses_thread_anchor_for_fallback_headers`
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
#[test]
fn uses_thread_anchor_for_fallback_headers() {
    let actual = derive_outgoing_session_affinity(
        Some("legacy_session_should_not_win"),
        Some("legacy_request_id_should_not_win"),
        Some("legacy_turn_state_should_not_win"),
        Some("conv_anchor_fallback"),
        Some("conv_anchor_fallback"),
    );

    assert_eq!(actual.incoming_session_id, Some("conv_anchor_fallback"));
    assert_eq!(
        actual.incoming_client_request_id,
        Some("conv_anchor_fallback")
    );
    assert_eq!(
        actual.incoming_turn_state,
        Some("legacy_turn_state_should_not_win")
    );
    assert_eq!(actual.fallback_session_id, Some("conv_anchor_fallback"));
}

/// 函数 `clears_turn_state_when_thread_anchor_diverges`
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
#[test]
fn clears_turn_state_when_thread_anchor_diverges() {
    let actual = derive_outgoing_session_affinity(
        Some("legacy_session_should_not_win"),
        Some("legacy_request_id_should_not_win"),
        Some("legacy_turn_state_should_not_win"),
        Some("conversation_anchor"),
        Some("prompt_thread_anchor"),
    );

    assert_eq!(actual.incoming_session_id, Some("conversation_anchor"));
    assert_eq!(
        actual.incoming_client_request_id,
        Some("conversation_anchor")
    );
    assert_eq!(actual.incoming_turn_state, None);
    assert_eq!(actual.fallback_session_id, Some("prompt_thread_anchor"));
}

/// 函数 `drops_orphan_turn_state_without_conversation_anchor`
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
#[test]
fn drops_orphan_turn_state_without_conversation_anchor() {
    let actual = derive_outgoing_session_affinity(
        None,
        Some("explicit_client_request_id"),
        Some("turn_state_ok"),
        None,
        None,
    );

    assert_eq!(actual.incoming_session_id, None);
    assert_eq!(
        actual.incoming_client_request_id,
        Some("explicit_client_request_id")
    );
    assert_eq!(actual.incoming_turn_state, None);
    assert_eq!(actual.fallback_session_id, None);
}

/// 函数 `conflict_detection_matches_anchor_mismatch`
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
#[test]
fn conflict_detection_matches_anchor_mismatch() {
    assert!(has_thread_anchor_conflict(
        Some("conversation_anchor"),
        Some("prompt_thread_anchor")
    ));
    assert!(!has_thread_anchor_conflict(
        Some("conversation_anchor"),
        Some("conversation_anchor")
    ));
    assert!(!has_thread_anchor_conflict(
        Some("conversation_anchor"),
        None
    ));
}
