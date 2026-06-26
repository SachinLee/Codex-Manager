use super::{platform_model_for_mapping_lookup, resolve_direct_upstream_model_for_log};

#[test]
fn direct_upstream_model_is_logged_for_override() {
    assert_eq!(
        resolve_direct_upstream_model_for_log(Some("gpt-5"), Some("gpt-5.4-openai-compact"),),
        Some("gpt-5.4-openai-compact")
    );
}

#[test]
fn direct_upstream_model_is_ignored_when_same_as_platform_model() {
    assert_eq!(
        resolve_direct_upstream_model_for_log(Some("gpt-5"), Some("gpt-5")),
        None
    );
}

#[test]
fn direct_upstream_model_skips_mapping_lookup() {
    assert_eq!(
        platform_model_for_mapping_lookup(Some("gpt-5"), Some("gpt-5.4-openai-compact")),
        None
    );
}

#[test]
fn mapping_lookup_uses_trimmed_platform_model_without_direct_override() {
    assert_eq!(
        platform_model_for_mapping_lookup(Some(" gpt-5 "), None),
        Some("gpt-5")
    );
    assert_eq!(platform_model_for_mapping_lookup(Some(" "), None), None);
}
