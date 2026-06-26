use super::{is_challenge_failover_error, should_forward_thread_anchor_as_prompt_cache_key};

#[test]
fn gemini_native_does_not_forward_thread_anchor_as_prompt_cache_key() {
    assert!(!should_forward_thread_anchor_as_prompt_cache_key(
        crate::apikey_profile::PROTOCOL_GEMINI_NATIVE
    ));
}

#[test]
fn non_gemini_protocols_keep_thread_anchor_forwarding() {
    assert!(should_forward_thread_anchor_as_prompt_cache_key(
        crate::apikey_profile::PROTOCOL_ANTHROPIC_NATIVE
    ));
    assert!(should_forward_thread_anchor_as_prompt_cache_key(
        crate::apikey_profile::PROTOCOL_OPENAI_COMPAT
    ));
}

#[test]
fn challenge_failover_error_detection_matches_cloudflare_markers() {
    assert!(is_challenge_failover_error(Some(
        "upstream challenge blocked"
    )));
    assert!(is_challenge_failover_error(Some(
        "Cloudflare 安全验证页 [cf_ray=abc]"
    )));
    assert!(!is_challenge_failover_error(Some("upstream rate-limited")));
    assert!(!is_challenge_failover_error(None));
}
