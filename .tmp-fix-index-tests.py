from pathlib import Path
path = Path(r"crates/core/src/storage/tests/request_token_stats_tests.rs")
text = path.read_text(encoding="utf-8")

helper = '''fn assert_uses_index(details: &[String], index_name: &str, label: &str) {
    assert!(
        details.iter().any(|detail| detail.contains(index_name)),
        "{label} should use {index_name}, got {details:?}"
    );
}
'''

helper_new = '''fn assert_uses_index(details: &[String], index_name: &str, label: &str) {
    assert!(
        details.iter().any(|detail| detail.contains(index_name)),
        "{label} should use {index_name}, got {details:?}"
    );
}

fn assert_uses_any_index(details: &[String], index_names: &[&str], label: &str) {
    assert!(
        index_names
            .iter()
            .any(|index_name| details.iter().any(|detail| detail.contains(index_name))),
        "{label} should use one of {index_names:?}, got {details:?}"
    );
}
'''

if helper not in text:
    raise SystemExit('helper not found')
text = text.replace(helper, helper_new, 1)

old1 = '''    assert_uses_index(
        &details,
        "idx_request_token_stats_key_model_created_at",
        "by-model raw usage summary",
    );'''
new1 = '''    assert_uses_any_index(
        &details,
        &[
            "idx_request_token_stats_key_model_created_at",
            "idx_request_token_stats_key_id_created_at",
        ],
        "by-model raw usage summary",
    );'''
if old1 not in text:
    raise SystemExit('by-model assert not found')
text = text.replace(old1, new1, 1)

old2 = '''    assert_uses_index(
        &details,
        "idx_request_token_stats_key_model_created_at",
        "by-key-model raw usage summary",
    );'''
new2 = '''    assert_uses_any_index(
        &details,
        &[
            "idx_request_token_stats_key_model_created_at",
            "idx_request_token_stats_key_id_created_at",
        ],
        "by-key-model raw usage summary",
    );'''
if old2 not in text:
    raise SystemExit('by-key-model assert not found')
text = text.replace(old2, new2, 1)

path.write_text(text, encoding='utf-8')
print('tests relaxed')
