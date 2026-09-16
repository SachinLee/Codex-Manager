use super::*;
use crate::storage::RequestLogQueryFilters;

#[test]
fn request_log_filter_builder_marks_token_stats_usage_only_when_needed() {
    let exact_filters = build_request_log_filters(
        RequestLogQueryFilters {
            query: Some("model:=gpt-5"),
            status_filter: Some("2xx"),
            start_ts: Some(1000),
            end_ts: Some(2000),
            ..Default::default()
        },
        false,
        None,
        true,
    );
    assert!(!exact_filters.uses_token_stats);
    assert!(!exact_filters.uses_account_lookup);

    let global_filters = build_request_log_filters(
        RequestLogQueryFilters {
            query: Some("42"),
            ..Default::default()
        },
        true,
        None,
        true,
    );
    assert!(global_filters.uses_token_stats);
    assert!(global_filters.uses_account_lookup);

    let account_filters = build_request_log_filters(
        RequestLogQueryFilters {
            query: Some("account:team"),
            ..Default::default()
        },
        true,
        None,
        true,
    );
    assert!(!account_filters.uses_token_stats);
    assert!(account_filters.uses_account_lookup);

    let account_without_table_filters = build_request_log_filters(
        RequestLogQueryFilters {
            query: Some("account:team"),
            ..Default::default()
        },
        false,
        None,
        true,
    );
    assert!(!account_without_table_filters.uses_token_stats);
    assert!(!account_without_table_filters.uses_account_lookup);
}

#[test]
fn request_log_filter_builder_appends_explicit_filters_as_and_terms() {
    let session_ids = ["sess-a".to_string(), "sess-b".to_string()];
    let filters = build_request_log_filters(
        RequestLogQueryFilters {
            query: Some("model:=gpt-5"),
            status_filter: Some("2xx"),
            start_ts: Some(1000),
            end_ts: Some(2000),
            session_ids: &session_ids,
            model: Some("gpt-5-codex"),
            key_id: Some("gk_alpha"),
        },
        false,
        None,
        true,
    );

    // 三个显式条件都必须以 AND 追加，并且与 query/状态/时间范围共存。
    assert!(
        filters.where_clause.contains("IFNULL(r.session_id, '') IN (?, ?)"),
        "where clause missing session ids: {}",
        filters.where_clause
    );
    assert!(
        filters.where_clause.contains("r.model = ?"),
        "where clause missing model: {}",
        filters.where_clause
    );
    assert!(
        filters.where_clause.contains("r.key_id = ?"),
        "where clause missing key id: {}",
        filters.where_clause
    );
    assert!(filters.where_clause.contains(" AND "));

    // 参数顺序：query → 状态（整数）→ 时间 → 会话 → 模型 → 密钥。
    let text_params: Vec<&str> = filters
        .params
        .iter()
        .filter_map(|value| match value {
            rusqlite::types::Value::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        text_params,
        vec!["gpt-5", "sess-a", "sess-b", "gpt-5-codex", "gk_alpha"]
    );
    let int_params: Vec<i64> = filters
        .params
        .iter()
        .filter_map(|value| match value {
            rusqlite::types::Value::Integer(int) => Some(*int),
            _ => None,
        })
        .collect();
    assert_eq!(int_params, vec![200, 299, 1000, 2000]);
}

#[test]
fn request_log_filter_builder_ignores_empty_explicit_filters() {
    let filters = build_request_log_filters(
        RequestLogQueryFilters {
            query: None,
            status_filter: None,
            start_ts: None,
            end_ts: None,
            session_ids: &[],
            model: None,
            key_id: None,
        },
        false,
        None,
        true,
    );

    assert_eq!(filters.where_clause, "WHERE r.cleared_at IS NULL");
    assert!(filters.params.is_empty());
}
